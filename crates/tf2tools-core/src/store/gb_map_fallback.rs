//! Finding a demo's missing map on GameBanana. Candidates come from the local
//! `store_gb_maps` catalogue only, ranked from an exact name to a looser stem match.

use rusqlite::{Connection, params};

use super::cards::like_pattern;
use super::pacer::Priority;
use super::{DownloadProgress, GbMapDownload, StoreClient, is_map_name};
use crate::error::{ArchiveError, BrowseError, GbAddonFetchError, GbMapSearchError};
use crate::library::GbId;

/// At most this many candidate profiles are asked for per search.
const MAX_PROFILES: usize = 6;

/// At most this many candidate archives are downloaded per search.
const MAX_DOWNLOADS: usize = 3;

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
        progress: &(dyn Fn(GbMapSearchProgress) + Send + Sync),
    ) -> Result<GbMapDownload, GbMapSearchError> {
        if !is_map_name(name) {
            return Err(GbMapSearchError::BadName {
                name: name.to_owned(),
            });
        }
        let ids = self.candidates(name).await?;

        // Every profile is fetched before any download, so the count shown is known up front.
        // Listings the map cap refuses are dropped; a file named for the map jumps the queue.
        let catalog = self.addon_catalog();
        let mut kept = Vec::new();
        for id in ids.into_iter().take(MAX_PROFILES) {
            match catalog.profile_at(id, Priority::Interactive).await {
                Ok(profile) if self.gb_map_refusal(&profile).is_none() => kept.push(profile),
                Ok(_) => {}
                Err(e) => on_profile(e)?,
            }
        }
        let stem = strip_version(name).to_ascii_lowercase();
        let (mut named, rest): (Vec<_>, Vec<_>) = kept.into_iter().partition(|p| {
            p.files()
                .first()
                .is_some_and(|f| f.name().to_ascii_lowercase().contains(&stem))
        });
        named.extend(rest);
        named.truncate(MAX_DOWNLOADS);
        let of = u8::try_from(named.len()).unwrap_or(u8::MAX);

        // Only an archive holding exactly one payload of this name counts, and the download
        // keeps only that payload. Anything else is dropped, removing its scratch dir.
        let mut tried = 0u8;
        for (candidate, profile) in (1u8..).zip(named.iter()) {
            let report = |download: DownloadProgress| {
                progress(GbMapSearchProgress {
                    candidate,
                    of,
                    download,
                });
            };
            report(DownloadProgress::starting());
            tried += 1;
            let mut download = match self.download_gb_map(profile, &report).await {
                Ok(download) => download,
                Err(e) => {
                    on_fetch(e)?;
                    continue;
                }
            };
            let matching = download
                .maps
                .iter()
                .filter(|m| m.name().eq_ignore_ascii_case(name))
                .count();
            if matching == 1 {
                download
                    .maps
                    .retain(|m| m.name().eq_ignore_ascii_case(name));
                return Ok(download);
            }
            // Removing a large scratch tree blocks, so a rejected download is dropped off
            // the async workers. A failed join only means the removal itself panicked.
            let _ = tokio::task::spawn_blocking(move || drop(download)).await;
        }
        Err(GbMapSearchError::NoMatch {
            name: name.to_owned(),
            tried,
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

/// Sorts a failed candidate download: `Ok` skips to the next candidate, while a failure
/// a later retry may fix stops the search. No wildcard, so a new variant must be sorted here.
fn on_fetch(e: GbAddonFetchError) -> Result<(), GbMapSearchError> {
    match e {
        GbAddonFetchError::RateLimited { url } => Err(GbMapSearchError::RateLimited { url }),
        GbAddonFetchError::Http { url, status } => http_status(url, status),
        GbAddonFetchError::Network { url, message } => {
            Err(GbMapSearchError::Network { url, message })
        }
        GbAddonFetchError::Io { path, source }
        | GbAddonFetchError::Archive(ArchiveError::Io { path, source }) => {
            Err(GbMapSearchError::Io { path, source })
        }
        GbAddonFetchError::Interrupted { message } => {
            Err(GbMapSearchError::Interrupted { message })
        }
        GbAddonFetchError::Archive(
            ArchiveError::Encrypted { .. }
            | ArchiveError::Malformed { .. }
            | ArchiveError::UnsafePath { .. }
            | ArchiveError::TooLarge { .. }
            | ArchiveError::TooManyEntries { .. },
        )
        | GbAddonFetchError::NoFiles { .. }
        | GbAddonFetchError::VirusScan { .. }
        | GbAddonFetchError::TooLarge { .. }
        | GbAddonFetchError::EmptyBody { .. }
        | GbAddonFetchError::ChecksumMismatch { .. }
        | GbAddonFetchError::OnlyExe { .. }
        | GbAddonFetchError::UnknownLayout { .. } => Ok(()),
    }
}

/// Sorts a failed profile fetch the same way: rate limits and outages stop the search,
/// while a missing, gone or odd listing is skipped.
fn on_profile(e: BrowseError) -> Result<(), GbMapSearchError> {
    match e {
        BrowseError::RateLimited { url } | BrowseError::Throttled { url } => {
            Err(GbMapSearchError::RateLimited { url })
        }
        BrowseError::Http { url, status } => http_status(url, status),
        BrowseError::Network { url, message } => Err(GbMapSearchError::Network { url, message }),
        BrowseError::TooLarge { .. } | BrowseError::Malformed { .. } | BrowseError::Gone { .. } => {
            Ok(())
        }
    }
}

/// A 429 is a rate limit and a 5xx an outage, both worth stopping for; any other
/// status, such as a 404, only rules this candidate out.
fn http_status(url: String, status: u16) -> Result<(), GbMapSearchError> {
    match status {
        429 => Err(GbMapSearchError::RateLimited { url }),
        500.. => Err(GbMapSearchError::Network {
            url,
            message: format!("HTTP {status}"),
        }),
        _ => Ok(()),
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
