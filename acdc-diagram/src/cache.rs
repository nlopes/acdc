//! Reusing diagrams that have already been generated.
//!
//! Running `PlantUML` or `Graphviz` on every build is the dominant cost of a
//! document full of diagrams, so each generated image is paired with a small
//! JSON sidecar recording what produced it. On the next run the sidecar is
//! compared against the diagram's current checksum and the converter options;
//! when both match and the image is still on disk, nothing is executed.
//!
//! The sidecar also carries the measured image size, so the HTML backend can
//! set `width`/`height` without re-decoding a cached PNG.
//!
//! With the `cache-images` option the image itself is kept in the cache
//! directory and hard-linked into the image output directory, so wiping the
//! output tree costs a relink rather than a regeneration.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// What was recorded about a previously generated image.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ImageMetadata {
    /// Digest of the diagram code and its attributes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) checksum: Option<String>,
    /// Converter options the image was generated with.
    #[serde(default)]
    pub(crate) options: BTreeMap<String, String>,
    /// Measured width in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) width: Option<f64>,
    /// Measured height in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) height: Option<f64>,
}

impl ImageMetadata {
    /// Read a sidecar, treating anything unreadable as "no cache entry".
    ///
    /// A corrupt or half-written sidecar should cost one regeneration, not a
    /// failed build, so parse failures are logged and swallowed.
    pub(crate) fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|error| {
            tracing::debug!(path = %path.display(), %error, "ignoring unreadable diagram cache entry");
            Self::default()
        })
    }

    /// Write a sidecar, creating its directory.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the directory or the file cannot be written.
    pub(crate) fn store(&self, path: &Path) -> Result<()> {
        create_parent(path)?;
        let text = serde_json::to_string(self).map_err(|error| {
            Error::config(format!("could not encode diagram cache entry: {error}"))
        })?;
        std::fs::write(path, text)
            .map_err(|source| Error::io(format!("could not write {}", path.display()), source))
    }
}

/// Create the directory `path` lives in.
///
/// # Errors
///
/// Returns [`Error::Io`] when the directory cannot be created.
pub(crate) fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| {
            Error::io(format!("could not create {}", parent.display()), source)
        })?;
    }
    Ok(())
}

/// Write `data` to `path`, creating its directory.
///
/// # Errors
///
/// Returns [`Error::Io`] when the file cannot be written.
pub(crate) fn write_file(path: &Path, data: &[u8]) -> Result<()> {
    create_parent(path)?;
    std::fs::write(path, data)
        .map_err(|source| Error::io(format!("could not write {}", path.display()), source))
}

/// Publish a cached image into the output tree.
///
/// A hard link keeps the two copies in step and costs no disk space; it fails
/// across filesystems and on platforms without links, so a copy is the
/// fallback.
///
/// # Errors
///
/// Returns [`Error::Io`] when neither linking nor copying succeeds.
pub(crate) fn link_or_copy(from: &Path, to: &Path) -> Result<()> {
    create_parent(to)?;
    if to.exists() {
        std::fs::remove_file(to)
            .map_err(|source| Error::io(format!("could not replace {}", to.display()), source))?;
    }
    if std::fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|source| Error::io(format!("could not copy to {}", to.display()), source))
}

/// Remove `path` if it exists, ignoring a missing file.
///
/// # Errors
///
/// Returns [`Error::Io`] when the file exists but cannot be removed.
pub(crate) fn remove_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::io(
            format!("could not remove {}", path.display()),
            source,
        )),
    }
}

/// The sidecar path for an image.
#[must_use]
pub(crate) fn metadata_path(cache_dir: &Path, image_name: &str) -> PathBuf {
    cache_dir.join(format!("{image_name}.cache"))
}
