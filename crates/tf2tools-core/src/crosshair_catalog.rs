//! The builtin crosshair catalog: entries of the bundled `crosshairs.json` that have
//! a matching VTF in `vtf/`, with the optional VMT in `vmt/` and preview PNG in `png/`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::CatalogError;

/// The bundled crosshairs present in both `crosshairs.json` and `vtf/`, in manifest order.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BuiltinCrosshairs {
    entries: Vec<BuiltinCrosshair>,
    index: HashMap<String, usize>,
    missing: Vec<String>,
}

/// One bundled crosshair texture.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BuiltinCrosshair {
    name: String,
    display_name: String,
    vtf_path: PathBuf,
    vmt_path: Option<PathBuf>,
    preview_png: Option<PathBuf>,
}

/// One manifest record; kept private so serde stays out of the public API.
#[derive(Deserialize)]
struct ManifestEntry {
    filename: String,
    display_name: String,
}

impl BuiltinCrosshairs {
    /// Loads the catalog from `dir`, which holds `crosshairs.json`, `vtf/`, `vmt/` and an
    /// optional `png/` (the repo's `assets/crosshairs`). Fails with `Io` when any of the
    /// first three cannot be read and with `Manifest` when the JSON is not a list of
    /// `{filename, display_name}`.
    pub fn load(dir: &Path) -> Result<Self, CatalogError> {
        let manifest_path = dir.join("crosshairs.json");
        let text = fs::read_to_string(&manifest_path).map_err(|source| CatalogError::Io {
            path: manifest_path.clone(),
            source,
        })?;
        let manifest: Vec<ManifestEntry> =
            serde_json::from_str(&text).map_err(|e| CatalogError::Manifest {
                path: manifest_path,
                message: e.to_string(),
            })?;

        // Match against directory listings rather than probing paths, so a name only
        // counts when its exact case exists, whatever the filesystem's case rules.
        let vtf_dir = dir.join("vtf");
        let vmt_dir = dir.join("vmt");
        let vtfs = file_names(&vtf_dir)?;
        let vmts = file_names(&vmt_dir)?;
        // Previews are optional: without a `png/` folder every preview path is None.
        let png_dir = dir.join("png");
        let pngs = if png_dir.exists() {
            file_names(&png_dir)?
        } else {
            HashSet::new()
        };

        let mut entries = Vec::with_capacity(manifest.len());
        let mut index = HashMap::with_capacity(manifest.len());
        let mut missing = Vec::new();
        for ManifestEntry {
            filename,
            display_name,
        } in manifest
        {
            let vtf_file = format!("{filename}.vtf");
            if !vtfs.contains(vtf_file.as_str()) {
                missing.push(filename);
                continue;
            }
            let vmt_file = format!("{filename}.vmt");
            let vmt_path = vmts
                .contains(vmt_file.as_str())
                .then(|| vmt_dir.join(&vmt_file));
            let png_file = format!("{filename}.png");
            let preview_png = pngs
                .contains(png_file.as_str())
                .then(|| png_dir.join(&png_file));
            index.insert(filename.clone(), entries.len());
            entries.push(BuiltinCrosshair {
                vtf_path: vtf_dir.join(&vtf_file),
                vmt_path,
                preview_png,
                name: filename,
                display_name,
            });
        }
        Ok(Self {
            entries,
            index,
            missing,
        })
    }

    /// Every available crosshair, in manifest order.
    pub fn all(&self) -> &[BuiltinCrosshair] {
        &self.entries
    }

    /// The crosshair whose manifest filename is exactly `name`.
    pub fn get(&self, name: &str) -> Option<&BuiltinCrosshair> {
        self.index.get(name).map(|&i| &self.entries[i])
    }

    /// Manifest filenames that have no bundled VTF, in manifest order.
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// The number of available crosshairs.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no crosshair is available.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl BuiltinCrosshair {
    /// The manifest filename, which is also the VTF and VMT file stem.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The human-readable name from the manifest.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// `dir/vtf/<name>.vtf`.
    pub fn vtf_path(&self) -> &Path {
        &self.vtf_path
    }

    /// `dir/vmt/<name>.vmt` when that exact file exists.
    pub fn vmt_path(&self) -> Option<&Path> {
        self.vmt_path.as_deref()
    }

    /// `dir/png/<name>.png` when that exact file exists.
    pub fn preview_png_path(&self) -> Option<&Path> {
        self.preview_png.as_deref()
    }
}

/// The names of the entries in `dir`, which must exist and be readable.
fn file_names(dir: &Path) -> Result<HashSet<String>, CatalogError> {
    let io = |source| CatalogError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut names = HashSet::new();
    for entry in fs::read_dir(dir).map_err(io)? {
        // A non-UTF-8 name can never equal a manifest name, so it is skipped.
        if let Ok(name) = entry.map_err(io)?.file_name().into_string() {
            names.insert(name);
        }
    }
    Ok(names)
}
