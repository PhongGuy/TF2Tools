//! The curated HUD model and v5's store filters.
//!
//! `CuratedHud` reads a hud-db record out of a store row; `CuratedFilter` narrows a list
//! of them in memory with v5's rules (`store-state.svelte.ts`).

use std::collections::BTreeSet;

use serde_json::Value;
use tf2tools_core::{StoreItem, StoreItemId};

/// An operating system a curated HUD declares support for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    /// Every OS, in filter order.
    pub const ALL: [Os; 3] = [Os::Windows, Os::Mac, Os::Linux];

    /// The OS this build runs on; anything that is neither macOS nor Linux counts as Windows.
    pub fn current() -> Os {
        match std::env::consts::OS {
            "macos" => Os::Mac,
            "linux" => Os::Linux,
            _ => Os::Windows,
        }
    }

    /// The value hud-db uses in `traits.os`.
    pub fn key(self) -> &'static str {
        match self {
            Os::Windows => "windows",
            Os::Mac => "mac",
            Os::Linux => "linux",
        }
    }

    /// The display label.
    pub fn label(self) -> &'static str {
        match self {
            Os::Windows => "Windows",
            Os::Mac => "Mac",
            Os::Linux => "Linux",
        }
    }
}

/// A multi-value attribute the store filters on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Facet {
    Flag,
    Tag,
    Gamemode,
    Resolution,
}

impl Facet {
    /// The facet's id key: "flag", "tag", "gamemode" or "resolution".
    pub fn key(self) -> &'static str {
        match self {
            Facet::Flag => "flag",
            Facet::Tag => "tag",
            Facet::Gamemode => "gamemode",
            Facet::Resolution => "resolution",
        }
    }
}

/// A curated HUD: the store row plus the fields of its hud-db record.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CuratedHud {
    pub hud_id: String,
    pub name: String,
    pub author: String,
    pub repo: Option<String>,
    pub images: Vec<String>,
    pub verified: bool,
    pub prerelease: bool,
    pub release_date: Option<String>,
    pub contributors: Vec<String>,
    pub flags: Vec<String>,
    pub tags: Vec<String>,
    pub os: Vec<String>,
    pub gamemodes: Vec<String>,
    pub resolutions: Vec<String>,
    /// (key, value), sorted by key.
    pub socials: Vec<(String, String)>,
}

impl CuratedHud {
    /// Builds a HUD from its row fields and hud-db JSON. Unparseable JSON gives the row
    /// fields only, as v5's `{}` fallback.
    pub fn parse(
        hud_id: &str,
        name: &str,
        author: &str,
        repo: Option<&str>,
        images: Vec<String>,
        data_json: &str,
    ) -> Self {
        let data: Value = serde_json::from_str(data_json).unwrap_or(Value::Null);
        let traits = &data["traits"];
        // Socials are sorted explicitly so the order never depends on serde_json's map type.
        let mut socials: Vec<(String, String)> = data["social"]
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        socials.sort();
        Self {
            hud_id: hud_id.to_string(),
            name: name.to_string(),
            author: author.to_string(),
            repo: repo.map(str::to_string),
            images,
            verified: data["verified"].as_bool().unwrap_or(false),
            prerelease: data["prerelease"].as_bool().unwrap_or(false),
            release_date: data["releaseDate"].as_str().map(str::to_string),
            contributors: strings(&data["contributors"]),
            flags: strings(&data["flags"]),
            tags: strings(&data["tags"]),
            os: strings(&traits["os"]),
            gamemodes: strings(&traits["gamemode"]),
            resolutions: strings(&traits["res"]),
            socials,
        }
    }

    /// Reads a curated store row; `None` for any other store's item.
    pub fn from_item(item: &StoreItem) -> Option<Self> {
        let StoreItemId::Curated(hud_id) = item.id() else {
            return None;
        };
        Some(Self::parse(
            hud_id,
            item.name(),
            item.author(),
            item.repo(),
            item.images().to_vec(),
            item.data_json().unwrap_or("{}"),
        ))
    }

    /// The HUD's values for one facet.
    pub fn values(&self, facet: Facet) -> &[String] {
        match facet {
            Facet::Flag => &self.flags,
            Facet::Tag => &self.tags,
            Facet::Gamemode => &self.gamemodes,
            Facet::Resolution => &self.resolutions,
        }
    }
}

/// The string items of a JSON array; anything else reads as empty.
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The Curated tab's filter state. An empty set leaves its facet unfiltered.
#[derive(Clone, Debug, PartialEq)]
pub struct CuratedFilter {
    pub search: String,
    pub os: BTreeSet<Os>,
    pub hide_prerelease: bool,
    pub flags: BTreeSet<String>,
    pub tags: BTreeSet<String>,
    pub gamemodes: BTreeSet<String>,
    pub resolutions: BTreeSet<String>,
}

impl CuratedFilter {
    /// The first-load state: the given OS preset and prereleases hidden.
    pub fn new(os: Os) -> Self {
        let mut filter = Self {
            search: String::new(),
            os: BTreeSet::new(),
            hide_prerelease: true,
            flags: BTreeSet::new(),
            tags: BTreeSet::new(),
            gamemodes: BTreeSet::new(),
            resolutions: BTreeSet::new(),
        };
        filter.os.insert(os);
        filter
    }

    /// Whether a HUD passes: search on name or author, flags all-of, everything else any-of.
    pub fn matches(&self, hud: &CuratedHud) -> bool {
        self.matches_query(hud, &self.search.to_lowercase())
    }

    // `matches` with the search already lowercased, so `apply` lowercases it once per list.
    fn matches_query(&self, hud: &CuratedHud, query: &str) -> bool {
        let searched = query.is_empty()
            || hud.name.to_lowercase().contains(query)
            || hud.author.to_lowercase().contains(query);
        let any_of = |wanted: &BTreeSet<String>, have: &[String]| {
            wanted.is_empty() || have.iter().any(|v| wanted.contains(v))
        };
        searched
            && !(self.hide_prerelease && hud.prerelease)
            // A HUD that lists no OS claims none, so the OS filter cannot rule it out.
            && (self.os.is_empty()
                || hud.os.is_empty()
                || self
                    .os
                    .iter()
                    .any(|os| hud.os.iter().any(|v| v == os.key())))
            && self.flags.iter().all(|f| hud.flags.contains(f))
            && any_of(&self.tags, &hud.tags)
            && any_of(&self.gamemodes, &hud.gamemodes)
            && any_of(&self.resolutions, &hud.resolutions)
    }

    /// v5's `hasActiveFilters`: a search, an OS, any facet value, or prereleases shown.
    pub fn is_active(&self) -> bool {
        !self.search.is_empty()
            || !self.os.is_empty()
            || !self.hide_prerelease
            || [Facet::Flag, Facet::Tag, Facet::Gamemode, Facet::Resolution]
                .into_iter()
                .any(|facet| !self.selected(facet).is_empty())
    }

    /// The indices of the HUDs that pass, in list order.
    pub fn apply(&self, huds: &[CuratedHud]) -> Vec<usize> {
        let query = self.search.to_lowercase();
        (0..huds.len())
            .filter(|&i| self.matches_query(&huds[i], &query))
            .collect()
    }

    /// Selects a facet value, or deselects it when already selected.
    pub fn toggle(&mut self, facet: Facet, value: &str) {
        let set = self.selected_mut(facet);
        if !set.remove(value) {
            set.insert(value.to_string());
        }
    }

    /// Selects an OS, or deselects it when already selected.
    pub fn toggle_os(&mut self, os: Os) {
        if !self.os.remove(&os) {
            self.os.insert(os);
        }
    }

    /// The selected values of one facet.
    pub fn selected(&self, facet: Facet) -> &BTreeSet<String> {
        match facet {
            Facet::Flag => &self.flags,
            Facet::Tag => &self.tags,
            Facet::Gamemode => &self.gamemodes,
            Facet::Resolution => &self.resolutions,
        }
    }

    fn selected_mut(&mut self, facet: Facet) -> &mut BTreeSet<String> {
        match facet {
            Facet::Flag => &mut self.flags,
            Facet::Tag => &mut self.tags,
            Facet::Gamemode => &mut self.gamemodes,
            Facet::Resolution => &mut self.resolutions,
        }
    }

    /// v5's `clearFilters`: everything empty, including the OS preset, and prereleases hidden.
    pub fn clear(&mut self) {
        *self = Self {
            os: BTreeSet::new(),
            ..Self::new(Os::Windows)
        };
    }
}

/// Every value of a facet across the HUDs, deduplicated and sorted.
pub fn facets(huds: &[CuratedHud], facet: Facet) -> Vec<String> {
    let values: BTreeSet<&String> = huds.iter().flat_map(|h| h.values(facet)).collect();
    values.into_iter().cloned().collect()
}

/// The count line: "{shown} of {total} HUDs".
pub fn count_label(shown: usize, total: usize) -> String {
    format!("{shown} of {total} HUDs")
}
