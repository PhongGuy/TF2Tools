//! Finding a demo's missing map on GameBanana. Candidates come from the local
//! `store_gb_maps` catalogue only, ranked from an exact name to a looser stem match.

use rusqlite::{Connection, params};

use super::cards::like_pattern;
use super::pacer::Priority;
use super::{DownloadProgress, GbMapDownload, StoreClient, is_map_name};
use crate::error::GbMapSearchError;
use crate::library::GbId;

/// At most this many candidate profiles are asked for per search.
const MAX_PROFILES: usize = 6;

/// How far a map search has come: the download of candidate `candidate` of `of`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct GbMapSearchProgress {
    /// The 1-based candidate being downloaded.
    pub candidate: u8,
    /// How many archives may be downloaded in this search.
    pub of: u8,
    /// The candidate's download progress.
    pub download: DownloadProgress,
}

impl StoreClient {
    /// Searches the synced GameBanana map catalogue for an item holding exactly the map
    /// `name`, reporting each candidate's download through `progress`.
    pub async fn find_gb_map(
        &self,
        name: &str,
        _progress: &(dyn Fn(GbMapSearchProgress) + Send + Sync),
    ) -> Result<GbMapDownload, GbMapSearchError> {
        if !is_map_name(name) {
            return Err(GbMapSearchError::BadName {
                name: name.to_owned(),
            });
        }
        let ids = self.candidates(name).await?;
        let catalog = self.addon_catalog();
        for id in ids.into_iter().take(MAX_PROFILES) {
            // Every outcome is ignored until the download loop judges the listings.
            let _ = catalog.profile_at(id, Priority::Interactive).await;
        }
        Err(GbMapSearchError::NoMatch {
            name: name.to_owned(),
            tried: 0,
        })
    }

    /// The catalogue's candidate items for `name`, best first, or `NotSynced` when the
    /// catalogue is empty. The read blocks, so it runs off the async workers.
    async fn candidates(&self, name: &str) -> Result<Vec<GbId>, GbMapSearchError> {
        let db = self.db.clone();
        let name = name.to_owned();
        let read = tokio::task::spawn_blocking(move || {
            crate::panic_mark::marked(|| db.read(|conn| rank_candidates(conn, &name)))
        })
        .await
        .map_err(|e| GbMapSearchError::Interrupted {
            message: e.to_string(),
        })?;
        match read {
            Ok(Some(ids)) => Ok(ids),
            Ok(None) => Err(GbMapSearchError::NotSynced),
            Err(e) => Err(GbMapSearchError::Interrupted {
                message: e.to_string(),
            }),
        }
    }
}

/// Live catalogue ids in three tiers: the exact name, the name anywhere, then the name
/// without its version suffix anywhere. Each tier runs in download order and an id is
/// kept where it first appears. `None` when the catalogue has no rows at all.
fn rank_candidates(conn: &Connection, name: &str) -> rusqlite::Result<Option<Vec<GbId>>> {
    let synced: bool = conn.query_row("SELECT EXISTS (SELECT 1 FROM store_gb_maps)", [], |r| {
        r.get(0)
    })?;
    if !synced {
        return Ok(None);
    }
    const LIVE: &str = "SELECT gb_id FROM store_gb_maps WHERE obsolete = 0 AND ";
    const ORDER: &str = " ORDER BY download_rank, gb_id";
    let exact = format!("{LIVE}name = ?1 COLLATE NOCASE{ORDER}");
    let within = format!("{LIVE}name LIKE ?1 ESCAPE '\\'{ORDER}");
    let mut tiers = vec![
        (exact, name.to_owned()),
        (within.clone(), like_pattern(name)),
    ];
    let stem = strip_version(name);
    if stem != name && !stem.is_empty() {
        tiers.push((within, like_pattern(stem)));
    }

    let mut ids: Vec<GbId> = Vec::new();
    for (sql, param) in tiers {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![param], |r| r.get::<_, i64>(0))?;
        for row in rows {
            let Ok(raw) = u64::try_from(row?) else {
                continue;
            };
            let id = GbId::new(raw);
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    Ok(Some(ids))
}

/// `name` without one trailing `_final`, `_rc<n>`, `_b<n>`, `_a<n>` or `_v<n>`, matched
/// ASCII case-insensitively; `name` itself when it has none.
fn strip_version(name: &str) -> &str {
    let Some((stem, tail)) = name.rsplit_once('_') else {
        return name;
    };
    let lower = tail.to_ascii_lowercase();
    let numbered = ["rc", "b", "a", "v"].iter().any(|prefix| {
        lower
            .strip_prefix(prefix)
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    });
    if lower == "final" || numbered {
        stem
    } else {
        name
    }
}
