//! The Addon Store's live GameBanana catalog: the public query and card types, and the
//! paged step that reads Index pages, filters them and streams the kept cards.

use std::sync::Arc;

use super::StoreClient;
use super::addon_api::{self, AddonRecord, ListingPage};
use super::addon_sections::AddonSection;
use super::pacer::{Priority, Reply};
use crate::{BrowseError, GbId};

/// The most Index pages one step reads.
pub const INDEX_STEP_CALLS: u32 = 4;
/// The most Search pages one step reads; Search serves at most 15 a page.
pub const SEARCH_STEP_CALLS: u32 = 6;
/// A step stops after the page that brings its kept cards to this many.
pub const STEP_CARDS: usize = 50;

/// The order an Addon Store section lists its items in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AddonSort {
    /// Most downloaded first.
    MostDownloaded,
    /// Newest upload first.
    Newest,
}

/// One GameBanana item as an Addon Store card shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonCard {
    /// The GameBanana item id.
    pub id: GbId,
    /// The item's title.
    pub name: String,
    /// The submitter's name.
    pub author: String,
    /// The first 100 px preview rendition, if the item has one.
    pub thumbnail_url: Option<String>,
    /// GameBanana likes.
    pub like_count: i64,
    /// When it was added, in Unix seconds.
    pub date_added: i64,
    /// The subcategory id; None when the item sits directly in its root category.
    pub category_id: Option<u32>,
    /// The subcategory's name, alongside `category_id`.
    pub category_name: Option<String>,
    /// The author marked it obsolete.
    pub obsolete: bool,
    /// Warned or carrying content ratings; hidden unless the player shows mature add-ons.
    pub mature: bool,
    /// The item's GameBanana page.
    pub profile_url: String,
}

/// What one Addon Store grid shows: a section, its order, an optional subcategory or
/// search text, and whether mature items are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonQuery {
    section: AddonSection,
    sort: AddonSort,
    category: Option<u32>,
    search: Option<String>,
    mature: bool,
}

impl AddonQuery {
    /// The whole of `section` in `sort` order, mature items hidden.
    pub fn new(section: AddonSection, sort: AddonSort) -> Self {
        Self {
            section,
            sort,
            category: None,
            search: None,
            mature: false,
        }
    }

    /// Narrows the listing to subcategory `id`; None lists the whole section.
    pub fn with_category(mut self, id: Option<u32>) -> Self {
        self.category = id;
        self
    }

    /// Searches for `text`, trimmed; blank text clears the search.
    pub fn with_search(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        let text = text.trim();
        self.search = (!text.is_empty()).then(|| text.to_owned());
        self
    }

    /// Keeps mature items when `show` is true.
    pub fn with_mature(mut self, show: bool) -> Self {
        self.mature = show;
        self
    }

    /// The section listed.
    pub fn section(&self) -> AddonSection {
        self.section
    }

    /// The order listed in.
    pub fn sort(&self) -> AddonSort {
        self.sort
    }

    /// The subcategory, if one is chosen.
    pub fn category(&self) -> Option<u32> {
        self.category
    }

    /// The trimmed search text, if any.
    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    /// Whether mature items are kept.
    pub fn mature(&self) -> bool {
        self.mature
    }
}

/// Where the next step starts reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonCursor {
    page: u32,
}

impl AddonCursor {
    /// The first page.
    pub const START: AddonCursor = AddonCursor { page: 1 };

    /// The 1-based page the next step reads first.
    pub fn page(self) -> u32 {
        self.page
    }
}

/// How one step went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonStep {
    next: Option<AddonCursor>,
    pages: u32,
    cards: usize,
}

impl AddonStep {
    /// Where the next step starts; None once the listing has ended.
    pub fn next(self) -> Option<AddonCursor> {
        self.next
    }

    /// The pages this step read.
    pub fn pages(self) -> u32 {
        self.pages
    }

    /// The cards this step kept and streamed, before the app removes duplicates.
    pub fn cards(self) -> usize {
        self.cards
    }
}

/// Live GameBanana add-on browsing over a store client's pacer. Owned and `'static`, so
/// it can move to a background task.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AddonCatalog {
    store: StoreClient,
}

impl AddonCatalog {
    pub(super) fn new(store: StoreClient) -> Self {
        Self { store }
    }

    /// One scroll step from `from`: at most 4 Index (6 Search) pages, each page's kept cards
    /// passed to `sink` as it lands (never an empty Vec); stops after the page reaching 50 cards
    /// or at the listing's end (`next` None). Fails with the first page that cannot be read.
    pub async fn step(
        &self,
        query: &AddonQuery,
        from: AddonCursor,
        sink: &(dyn Fn(Vec<AddonCard>) + Send + Sync),
    ) -> Result<AddonStep, BrowseError> {
        // Whole pages only: a page that crosses 50 cards is streamed in full, so the
        // next cursor never points into the middle of a page.
        let cap = if query.search.is_some() {
            SEARCH_STEP_CALLS
        } else {
            INDEX_STEP_CALLS
        };
        let (mut page, mut pages, mut cards) = (from.page, 0, 0);
        while pages < cap {
            let listing = self.listing(query, page).await?;
            pages += 1;
            page += 1;
            let kept: Vec<AddonCard> = listing
                .records
                .iter()
                .filter(|r| keeps(query, r))
                .map(|r| r.card.clone())
                .collect();
            cards += kept.len();
            if !kept.is_empty() {
                sink(kept);
            }
            if listing.complete {
                return Ok(AddonStep {
                    next: None,
                    pages,
                    cards,
                });
            }
            if cards >= STEP_CARDS {
                break;
            }
        }
        Ok(AddonStep {
            next: Some(AddonCursor { page }),
            pages,
            cards,
        })
    }

    async fn listing(
        &self,
        query: &AddonQuery,
        page: u32,
    ) -> Result<Arc<ListingPage>, BrowseError> {
        let category = query.category.unwrap_or(query.section.root_category());
        let url = addon_api::index_url(
            self.store.endpoints.gamebanana(),
            category,
            query.sort,
            page,
        );
        let parsed = self
            .store
            .gamebanana_call(&url, Priority::Interactive, read_listing)
            .await?
            .map_err(|message| BrowseError::Malformed {
                url: url.clone(),
                message,
            })?;
        Ok(Arc::new(parsed))
    }
}

/// Sorts one listing body for the pacer: a parsed page is `Done`; an unparseable body is
/// `Rejected`, so it keeps any backoff, as `sync.rs::fetch_page` does.
fn read_listing(body: &[u8]) -> Reply<Result<ListingPage, String>> {
    match addon_api::parse_listing(body) {
        Ok(page) => Reply::Done(Ok(page)),
        Err(e) => Reply::Rejected(Err(e.to_string())),
    }
}

/// Search spans every category, so its records must sit under the section's root; every
/// listing drops the section's excluded subcategories and, unless shown, mature items.
fn keeps(query: &AddonQuery, record: &AddonRecord) -> bool {
    let section = query.section;
    (query.search.is_none() || record.root_category == Some(section.root_category()))
        && !record
            .card
            .category_id
            .is_some_and(|c| section.exclude().contains(&c))
        && (query.mature || !record.card.mature)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pacer's test `a_rejected_reply_is_returned_but_does_not_count_as_clean` shows
    /// `Rejected` keeps the backoff; this pins that a malformed listing is sorted there.
    #[test]
    fn a_malformed_body_is_rejected_so_the_backoff_is_kept() {
        assert!(matches!(
            read_listing(b"<html>busy</html>"),
            Reply::Rejected(Err(_))
        ));
        assert!(matches!(
            read_listing(br#"{"_aMetadata":{"_bIsComplete":true},"_aRecords":[]}"#),
            Reply::Done(Ok(ListingPage { complete: true, .. }))
        ));
    }
}
