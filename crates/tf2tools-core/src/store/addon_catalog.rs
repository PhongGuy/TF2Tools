//! The Addon Store's live GameBanana catalog: the public query and card types.

use crate::GbId;

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
