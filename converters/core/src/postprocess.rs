//! Ordered transformations of one completed UTF-8 document.
//!
//! Selected hooks edit the rendered text in place after all docinfo insertions.
//! Register against the converter name: `html`, `html5s`, `markdown`, `manpage`,
//! or `terminal`. Both Markdown variants use `markdown`. Other converters can opt
//! in through [`crate::Converter::supports_text_output`].
//! If rendering or a callback fails, the pipeline discards the text before writing
//! the destination. Earlier diagnostics, companion files and other external effects
//! persist. Once writing starts, ordinary destination I/O errors may leave partial output.

use std::{fmt, path::Path, sync::Arc};

use acdc_parser::ast::DocumentRef;

use crate::{Diagnostics, render::RenderError};

/// The converter and paths used for this document. Streams can have no path.
#[derive(Clone, Copy)]
pub struct PostprocessContext<'a> {
    /// Configured converter name.
    pub backend: &'static str,
    /// Document used to generate the text.
    pub document: DocumentRef<'a>,
    /// Input file when available.
    pub source_file: Option<&'a Path>,
    /// Final file destination when available.
    pub output_path: Option<&'a Path>,
}

/// Error carried by the converter's I/O error when postprocessing fails.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PostprocessError {
    /// The backend has not opted into completed UTF-8 output.
    #[error("backend '{0}' does not support text postprocessing")]
    Unsupported(&'static str),
    /// A backend that opted into UTF-8 produced invalid bytes.
    #[error("invalid UTF-8 from backend '{backend}': {source}")]
    InvalidText {
        /// Configured converter name.
        backend: &'static str,
        /// Invalid byte position.
        source: std::str::Utf8Error,
    },
    /// A selected callback failed before output was delivered.
    #[error("postprocessor for '{backend}' failed: {source}")]
    Callback {
        /// Configured converter name.
        backend: &'static str,
        /// Original callback failure.
        source: RenderError,
    },
}

type Callback = dyn Fn(&mut String, PostprocessContext<'_>, &mut Diagnostics<'_>) -> Result<(), RenderError>
    + Send
    + Sync;

/// A callback for one named backend. The pipeline calls it once per document.
/// Local variables are per call; explicitly captured shared state belongs to the author.
#[derive(Clone)]
pub struct TextPostprocessor {
    pub(crate) backend: &'static str,
    pub(crate) process: Arc<Callback>,
}

impl fmt::Debug for TextPostprocessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextPostprocessor")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl TextPostprocessor {
    /// Register a complete-text transformation without executing it.
    pub fn new(
        backend: &'static str,
        process: impl Fn(
            &mut String,
            PostprocessContext<'_>,
            &mut Diagnostics<'_>,
        ) -> Result<(), RenderError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            backend,
            process: Arc::new(process),
        }
    }
}
