//! The class model files, read from the game's own `tf2_misc_dir.vpk` and never from `tf/custom`.

use std::path::{Path, PathBuf};

use crate::error::RigLoadError;
use crate::pak::{open_vpk, read_entry};

/// The pak every class model ships in.
const MISC_PAK: &str = "tf2_misc_dir.vpk";

/// The game's misc pak, opened once so every file of a rig comes from the same directory.
pub(crate) struct PakFiles {
    path: PathBuf,
    pak: vpk::VPK,
}

impl PakFiles {
    /// NoInstall when `<tf_dir>/tf2_misc_dir.vpk` is not a file; Malformed { file: "tf2_misc_dir.vpk" } on a bad pak.
    pub(crate) fn open(tf_dir: &Path) -> Result<PakFiles, RigLoadError> {
        let path = tf_dir.join(MISC_PAK);
        if !path.is_file() {
            return Err(RigLoadError::NoInstall);
        }
        let pak = open_vpk(&path).map_err(|e| RigLoadError::Malformed {
            file: MISC_PAK.to_owned(),
            what: e.to_string(),
        })?;
        Ok(PakFiles { path, pak })
    }

    /// The entry `name` (case-insensitive), None when absent; TooLarge over `cap` bytes.
    pub(crate) fn read(&self, name: &str, cap: u64) -> Result<Option<Vec<u8>>, RigLoadError> {
        // Pak paths are stored lower case, so the exact key usually hits before the scan.
        let key = name.to_ascii_lowercase();
        let Some((entry_name, entry)) = self.pak.tree.get_key_value(&key).or_else(|| {
            self.pak
                .tree
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(&key))
        }) else {
            return Ok(None);
        };
        // The directory states the size, so an oversized entry is refused before it is read.
        let size =
            u64::from(entry.dir_entry.preload_length) + u64::from(entry.dir_entry.file_length);
        if size > cap {
            return Err(RigLoadError::TooLarge {
                file: name.to_owned(),
            });
        }
        read_entry(&self.path, entry_name, entry)
            .map(Some)
            .map_err(|e| RigLoadError::Malformed {
                file: name.to_owned(),
                what: e.to_string(),
            })
    }
}
