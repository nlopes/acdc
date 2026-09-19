//! How the diagram pass is configured by its caller.
//!
//! Everything a diagram block can control comes from document attributes; what
//! the *caller* has to supply is the surrounding context the parser does not
//! record — where the document lives, where its output is going, which backend
//! is rendering it, and whether the run is allowed to execute arbitrary
//! commands.

use std::path::{Path, PathBuf};

/// Configuration for one run of the diagram pass.
///
/// Build with [`Options::builder`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Options {
    base_dir: PathBuf,
    output_dir: PathBuf,
    backend: String,
    unsafe_mode: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl Options {
    /// Start building a configuration.
    #[must_use]
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::default()
    }

    /// The directory relative paths in the document resolve against.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// The directory the converted document is written to, which is what
    /// `imagesdir` and the diagram cache are resolved against.
    #[must_use]
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// The backend name, for example `html5`.
    #[must_use]
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Whether the backend renders HTML.
    ///
    /// HTML is the one backend that wants explicit `width`/`height` on the
    /// image tag and that prefers any format over PDF.
    #[must_use]
    pub fn is_html_backend(&self) -> bool {
        self.backend.to_ascii_lowercase().contains("html")
    }

    /// Whether diagrams that run author-supplied commands may be generated.
    #[must_use]
    pub fn unsafe_mode(&self) -> bool {
        self.unsafe_mode
    }
}

/// Builder for [`Options`].
#[derive(Debug, Clone, Default)]
pub struct OptionsBuilder {
    base_dir: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    backend: Option<String>,
    unsafe_mode: bool,
}

impl OptionsBuilder {
    /// Set the directory relative paths resolve against, normally the
    /// directory holding the document being converted.
    #[must_use]
    pub fn base_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    /// Set the directory the converted document is written to.
    #[must_use]
    pub fn output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }

    /// Set the backend name.
    #[must_use]
    pub fn backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }

    /// Allow diagram types that execute author-supplied commands.
    #[must_use]
    pub fn unsafe_mode(mut self, unsafe_mode: bool) -> Self {
        self.unsafe_mode = unsafe_mode;
        self
    }

    /// Finish building.
    ///
    /// Unset directories fall back to the current working directory, which is
    /// what a document read from stdin resolves against.
    #[must_use]
    pub fn build(self) -> Options {
        let cwd = || std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let base_dir = self.base_dir.unwrap_or_else(cwd);
        let output_dir = self.output_dir.unwrap_or_else(|| base_dir.clone());
        Options {
            base_dir,
            output_dir,
            backend: self.backend.unwrap_or_else(|| "html5".to_string()),
            unsafe_mode: self.unsafe_mode,
        }
    }
}
