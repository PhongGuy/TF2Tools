//! GameBanana maps installed into `tf/custom`: where the game already has a map.

use super::{GbId, Library, library_db};
use crate::error::LibraryError;
use crate::store::{MapIndex, MapSource, downloaded_names, lowercase_all};
use crate::tf2::Tf2Install;

impl Library {
    /// Where `tf2` already has map `name`, ignoring case, or None. `except` skips that
    /// GameBanana item's own folder, so an update does not find the map it replaces.
    pub fn map_presence(
        &self,
        tf2: &Tf2Install,
        name: &str,
        except: Option<GbId>,
    ) -> Result<Option<MapSource>, LibraryError> {
        let serveme = lowercase_all(self.db.read(downloaded_names).map_err(library_db)?);
        Ok(MapIndex::scan(tf2).source(name, except, &serveme))
    }
}
