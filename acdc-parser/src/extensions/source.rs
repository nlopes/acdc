//! Document-local hooks for the entry source.

use std::{cell::RefCell, fmt, path::Path, sync::Arc};

use crate::{
    DocumentAttributes, Error, Position, SourceLocation, Warning, WarningKind,
    warning::WarningCollection,
};

/// An edit to one line of the normalized entry source supplied to this processor.
/// Line numbers start at one. Return edits in ascending order, once per line.
#[derive(Debug)]
#[non_exhaustive]
pub enum SourceEdit {
    /// Remove the line and its separator.
    Remove(usize),
    /// Replace a line with text. The reader trims trailing whitespace on each
    /// replacement line. Each LF-separated line in the
    /// replacement originates at the edited line. Empty text leaves a blank line.
    Replace { line: usize, text: String },
}

impl SourceEdit {
    pub(crate) fn line(&self) -> usize {
        match self {
            Self::Remove(line) | Self::Replace { line, .. } => *line,
        }
    }
}

/// Read-only document attributes and the source position of the current hook.
/// Entry preprocessors run at the start of the document.
pub struct SourceProcessorContext<'a> {
    pub file: Option<&'a Path>,
    pub line: usize,
    pub(crate) offset: usize,
    pub attributes: &'a DocumentAttributes<'a>,
    pub(crate) warnings: &'a RefCell<WarningCollection>,
}

impl SourceProcessorContext<'_> {
    /// The original position where this hook runs.
    #[must_use]
    pub fn location(&self) -> SourceLocation {
        let mut source = SourceLocation::at_position(
            self.file.map(Path::to_path_buf),
            Position::from_line_col(self.line, 1),
        );
        source.location.absolute_start = self.offset;
        source.location.absolute_end = self.offset;
        source
    }

    pub(crate) fn error(&self, message: String) -> Error {
        Error::Extension(Box::new(self.location()), message)
    }

    /// Add a recoverable warning at the hook's source position.
    pub fn warn(&self, message: impl Into<String>) {
        let warning = Warning::new(
            WarningKind::Other(message.into().into()),
            Some(self.location()),
        );
        warning.emit_trace();
        self.warnings.borrow_mut().push(warning);
    }
}

/// Runs once on the normalized entry source, before directive or block processing.
pub trait SourcePreprocessor {
    /// Return edits to the current input. An empty vector preserves its storage.
    ///
    /// # Errors
    /// Return a message to stop parsing with a located extension error.
    fn process(
        &mut self,
        input: &str,
        context: &SourceProcessorContext<'_>,
    ) -> Result<Vec<SourceEdit>, String>;
}

impl<F> SourcePreprocessor for F
where
    F: FnMut(&str, &SourceProcessorContext<'_>) -> Result<Vec<SourceEdit>, String>,
{
    fn process(
        &mut self,
        input: &str,
        context: &SourceProcessorContext<'_>,
    ) -> Result<Vec<SourceEdit>, String> {
        self(input, context)
    }
}

pub(super) struct SourceRegistration<P: ?Sized> {
    pub(super) name: String,
    pub(super) factory: Arc<dyn Fn() -> Box<P> + Send + Sync>,
}

impl<P: ?Sized> Clone for SourceRegistration<P> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            factory: Arc::clone(&self.factory),
        }
    }
}

impl<P: ?Sized> fmt::Debug for SourceRegistration<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SourceRegistration")
            .field(&self.name)
            .finish()
    }
}
