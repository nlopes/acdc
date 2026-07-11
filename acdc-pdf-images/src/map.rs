use std::collections::HashMap;
use std::path::PathBuf;

/// A resolved image, ready to embed: the on-disk file its bytes live in plus the
/// stable virtual path both the emitter (which references it) and the renderer
/// (which serves the bytes) agree on.
///
/// The bytes are deliberately **not** held here. The renderer reads [`path`]
/// on demand, so a document with many large images does not pin every image's
/// bytes in memory — the Typst compiler becomes the sole holder once it loads
/// one.
///
/// [`path`]: ResolvedImage::path
#[derive(Debug)]
pub struct ResolvedImage {
    /// The project-root-relative path used in the generated `#image("…")` call
    /// and as the file-resolver key. Content-hashed, so identical images share
    /// a path.
    pub virtual_path: String,
    /// The validated snapshot under `ResolveConfig::spool_dir`.
    pub path: PathBuf,
}

/// Maps each source image URL/path to its resolved, embeddable form.
///
/// Built by [`crate::resolve`]. Emitters and `acdc-pdf-render` read the same
/// [`ResolvedImage::virtual_path`], so the reference and the file can never
/// drift apart.
#[derive(Debug, Default)]
pub struct ImageMap {
    by_url: HashMap<String, ResolvedImage>,
}

impl ImageMap {
    /// An empty map (no images resolved).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a resolved image by its original source URL/path.
    #[must_use]
    pub fn get(&self, url: &str) -> Option<&ResolvedImage> {
        self.by_url.get(url)
    }

    /// Every resolved image, for registration with the renderer's file resolver.
    pub fn images(&self) -> impl Iterator<Item = &ResolvedImage> {
        self.by_url.values()
    }

    /// Merge another map's entries into this one.
    pub fn extend(&mut self, other: ImageMap) {
        self.by_url.extend(other.by_url);
    }

    /// Insert a resolved image under its source URL. Used by [`crate::resolve`].
    pub(super) fn insert(&mut self, url: String, image: ResolvedImage) {
        self.by_url.insert(url, image);
    }
}
