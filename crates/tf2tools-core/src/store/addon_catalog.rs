//! The Addon Store's live GameBanana catalog: the public query and card types, and the
//! paged step that reads Index pages, filters them and streams the kept cards.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

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

/// How long a fetched listing page is served from the cache.
pub(crate) const PAGE_TTL: Duration = Duration::from_secs(600);
/// The most listing pages the cache holds.
pub(crate) const PAGE_CACHE_CAP: usize = 256;

/// The add-on browsing caches, shared by every clone of a `StoreClient`.
#[derive(Debug, Default)]
pub(crate) struct Caches {
    pub(crate) pages: PageCache,
    /// Each parent's unfiltered child categories, fetched once a session.
    pub(crate) categories: Mutex<HashMap<u32, Vec<AddonCategory>>>,
}

/// What a cached listing page is filed under: the effective category, the order and the page.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum PageKey {
    Index {
        category: u32,
        sort: AddonSort,
        page: u32,
    },
    Search {
        text: String,
        page: u32,
    },
}

/// Listing pages kept for `PAGE_TTL`, at most `PAGE_CACHE_CAP` of them.
#[derive(Debug, Default)]
pub(crate) struct PageCache {
    entries: Mutex<HashMap<PageKey, (Instant, Arc<ListingPage>)>>,
}

impl PageCache {
    /// The page under `key` if it was stored less than `PAGE_TTL` before `now`.
    pub(crate) fn get(&self, key: &PageKey, now: Instant) -> Option<Arc<ListingPage>> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(key)
            .filter(|(stored, _)| now.saturating_duration_since(*stored) < PAGE_TTL)
            .map(|(_, page)| Arc::clone(page))
    }

    /// Stores `page` under `key` as of `now`, making room by dropping expired pages and then
    /// the oldest ones.
    pub(crate) fn put(&self, key: PageKey, page: Arc<ListingPage>, now: Instant) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|_, (stored, _)| now.saturating_duration_since(*stored) < PAGE_TTL);
        entries.remove(&key);
        while entries.len() >= PAGE_CACHE_CAP {
            let oldest = entries
                .iter()
                .min_by_key(|(_, (stored, _))| *stored)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => entries.remove(&k),
                None => break,
            };
        }
        entries.insert(key, (now, page));
    }

    /// The number of pages held, expired ones included until the next `put`.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

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

/// One GameBanana subcategory a section can be narrowed to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonCategory {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) items: u64,
}

impl AddonCategory {
    /// The GameBanana category id.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The category's name, such as "HUDs".
    pub fn name(&self) -> &str {
        &self.name
    }

    /// How many items GameBanana counts in it, across every game.
    pub fn items(&self) -> u64 {
        self.items
    }
}

/// One item's GameBanana page: its description, gallery, files and credits.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonProfile {
    pub(crate) id: GbId,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) profile_url: String,
    pub(crate) description: String,
    pub(crate) images: Vec<String>,
    pub(crate) download_count: u64,
    pub(crate) like_count: i64,
    pub(crate) obsolete: bool,
    pub(crate) mature: bool,
    pub(crate) ratings: Vec<String>,
    pub(crate) credits: Vec<AddonCreditGroup>,
    pub(crate) files: Vec<AddonFile>,
}

impl AddonProfile {
    /// The GameBanana item id.
    pub fn id(&self) -> GbId {
        self.id
    }

    /// The item's title.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The submitter's name.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// The item's GameBanana page.
    pub fn profile_url(&self) -> &str {
        &self.profile_url
    }

    /// The description as plain text, paragraphs split by an empty line.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The gallery, preferring 530 px renditions.
    pub fn images(&self) -> &[String] {
        &self.images
    }

    /// GameBanana downloads across all files.
    pub fn download_count(&self) -> u64 {
        self.download_count
    }

    /// GameBanana likes.
    pub fn like_count(&self) -> i64 {
        self.like_count
    }

    /// The author marked it obsolete.
    pub fn obsolete(&self) -> bool {
        self.obsolete
    }

    /// Warned or carrying content ratings.
    pub fn mature(&self) -> bool {
        self.mature
    }

    /// The content rating labels, such as "Skimpy Attire", sorted.
    pub fn ratings(&self) -> &[String] {
        &self.ratings
    }

    /// The credit groups in GameBanana's order.
    pub fn credits(&self) -> &[AddonCreditGroup] {
        &self.credits
    }

    /// The downloadable files, newest first.
    pub fn files(&self) -> &[AddonFile] {
        &self.files
    }
}

/// One downloadable file of an add-on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonFile {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) size: Option<u64>,
    pub(crate) date_added: i64,
    pub(crate) md5: Option<String>,
    pub(crate) av_result: String,
    pub(crate) download_url: String,
}

impl AddonFile {
    /// The GameBanana file id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The file name, such as "toon_muzzle_flashes_v12.rar".
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The size in bytes, when GameBanana gives one.
    pub fn size(&self) -> Option<u64> {
        self.size
    }

    /// When it was uploaded, in Unix seconds.
    pub fn date_added(&self) -> i64 {
        self.date_added
    }

    /// The MD5 checksum in hex, when GameBanana gives one.
    pub fn md5(&self) -> Option<&str> {
        self.md5.as_deref()
    }

    /// GameBanana's virus scan verdict, such as "clean".
    pub fn av_result(&self) -> &str {
        &self.av_result
    }

    /// Where the file downloads from.
    pub fn download_url(&self) -> &str {
        &self.download_url
    }
}

/// One named group of credits, such as "Key Authors".
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonCreditGroup {
    pub(crate) name: String,
    pub(crate) credits: Vec<AddonCredit>,
}

impl AddonCreditGroup {
    /// The group's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The people credited in it.
    pub fn credits(&self) -> &[AddonCredit] {
        &self.credits
    }
}

/// One credited person and what they did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonCredit {
    pub(crate) name: String,
    pub(crate) role: String,
}

impl AddonCredit {
    /// The person's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Their role, possibly empty.
    pub fn role(&self) -> &str {
        &self.role
    }
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

    /// `parent`'s child categories, A to Z, fetched once a session; `section`'s excluded ids
    /// left out.
    pub async fn categories(
        &self,
        section: AddonSection,
        parent: u32,
    ) -> Result<Vec<AddonCategory>, BrowseError> {
        let caches = &self.store.addon_caches;
        let cached = caches
            .categories
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&parent)
            .cloned();
        let all = match cached {
            Some(all) => all,
            None => {
                let url = addon_api::categories_url(self.store.endpoints.gamebanana(), parent);
                let all = self
                    .store
                    .gamebanana_call(&url, Priority::Interactive, read_categories)
                    .await?
                    .map_err(|message| BrowseError::Malformed {
                        url: url.clone(),
                        message,
                    })?;
                caches
                    .categories
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(parent, all.clone());
                all
            }
        };
        // The cache keeps the unfiltered list, since sections may share a parent.
        Ok(all
            .into_iter()
            .filter(|c| !section.exclude().contains(&c.id))
            .collect())
    }

    /// Item `id`'s profile page. Fails `Gone` for a trashed, withheld or private item; a
    /// missing id keeps its HTTP status.
    pub async fn profile(&self, id: GbId) -> Result<AddonProfile, BrowseError> {
        let url = addon_api::profile_url(self.store.endpoints.gamebanana(), id);
        let parsed = self
            .store
            .gamebanana_call(&url, Priority::Interactive, read_profile)
            .await?
            .map_err(|message| BrowseError::Malformed {
                url: url.clone(),
                message,
            })?;
        if parsed.gone {
            return Err(BrowseError::Gone { gb_id: id });
        }
        Ok(parsed.profile)
    }

    /// The page `page` of `query`'s listing, from the cache while it is fresh.
    async fn listing(
        &self,
        query: &AddonQuery,
        page: u32,
    ) -> Result<Arc<ListingPage>, BrowseError> {
        let (key, url) = self.page_key(query, page);
        let caches = &self.store.addon_caches;
        if let Some(hit) = caches.pages.get(&key, Instant::now()) {
            return Ok(hit);
        }
        let parsed = self
            .store
            .gamebanana_call(&url, Priority::Interactive, read_listing)
            .await?
            .map_err(|message| BrowseError::Malformed {
                url: url.clone(),
                message,
            })?;
        let parsed = Arc::new(parsed);
        caches.pages.put(key, Arc::clone(&parsed), Instant::now());
        Ok(parsed)
    }

    /// The cache key and URL of one listing page. Search ignores the subcategory; otherwise a
    /// chosen subcategory stands in for the section's root, which fully describes a section.
    fn page_key(&self, query: &AddonQuery, page: u32) -> (PageKey, String) {
        let base = self.store.endpoints.gamebanana();
        if let Some(text) = &query.search {
            let url = addon_api::search_url(base, text, page);
            let key = PageKey::Search {
                text: text.clone(),
                page,
            };
            return (key, url);
        }
        let category = query.category.unwrap_or(query.section.root_category());
        let url = addon_api::index_url(base, category, query.sort, page);
        let key = PageKey::Index {
            category,
            sort: query.sort,
            page,
        };
        (key, url)
    }
}

/// Sorts one listing body for the pacer. A blank body, or no records on a page that is not
/// the end, is `Empty`, so the pacer retries and throttles; an unparseable body is
/// `Rejected`, keeping any backoff as `sync.rs::fetch_page` does; anything else is `Done`.
fn read_listing(body: &[u8]) -> Reply<Result<ListingPage, String>> {
    if body.trim_ascii().is_empty() {
        return Reply::Empty;
    }
    match addon_api::parse_listing(body) {
        Ok(page) if page.records.is_empty() && !page.complete => Reply::Empty,
        Ok(page) => Reply::Done(Ok(page)),
        Err(e) => Reply::Rejected(Err(e.to_string())),
    }
}

/// Sorts one categories body for the pacer the way `read_listing` does.
fn read_categories(body: &[u8]) -> Reply<Result<Vec<AddonCategory>, String>> {
    if body.trim_ascii().is_empty() {
        return Reply::Empty;
    }
    match addon_api::parse_categories(body) {
        Ok(all) => Reply::Done(Ok(all)),
        Err(e) => Reply::Rejected(Err(e.to_string())),
    }
}

/// Sorts one profile body for the pacer the way `read_listing` does.
fn read_profile(body: &[u8]) -> Reply<Result<addon_api::ParsedProfile, String>> {
    if body.trim_ascii().is_empty() {
        return Reply::Empty;
    }
    match addon_api::parse_profile(body) {
        Ok(parsed) => Reply::Done(Ok(parsed)),
        Err(e) => Reply::Rejected(Err(e)),
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
    use std::time::Instant;

    fn key(page: u32) -> PageKey {
        PageKey::Index {
            category: 1644,
            sort: AddonSort::Newest,
            page,
        }
    }

    #[test]
    fn a_blank_body_or_an_unfinished_empty_page_is_an_empty_reply() {
        assert!(matches!(read_listing(b" \n"), Reply::Empty));
        assert!(matches!(
            read_listing(br#"{"_aMetadata":{"_bIsComplete":false},"_aRecords":[]}"#),
            Reply::Empty
        ));
    }

    #[test]
    fn a_page_expires_after_ten_minutes() {
        let (cache, t0) = (PageCache::default(), Instant::now());
        cache.put(key(1), Arc::new(ListingPage::default()), t0);
        assert!(cache.get(&key(1), t0 + Duration::from_secs(599)).is_some());
        assert!(cache.get(&key(1), t0 + PAGE_TTL).is_none());
        assert!(cache.get(&key(2), t0).is_none());
    }

    #[test]
    fn the_cache_drops_its_oldest_page_past_its_cap() {
        let (cache, t0) = (PageCache::default(), Instant::now());
        let cap = u32::try_from(PAGE_CACHE_CAP).unwrap();
        for page in 0..=cap {
            cache.put(
                key(page),
                Arc::new(ListingPage::default()),
                t0 + Duration::from_millis(page.into()),
            );
        }
        let now = t0 + Duration::from_secs(1);
        assert!(cache.get(&key(0), now).is_none());
        assert!(cache.get(&key(cap), now).is_some());
        assert_eq!(cache.len(), PAGE_CACHE_CAP);
    }

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
