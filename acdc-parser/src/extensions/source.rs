//! Document-local hooks for the entry source and resolved includes.

use std::{cell::RefCell, fmt, path::Path, sync::Arc};

use crate::{
    DocumentAttributes, Error, IncludeSource, IncludeSourceTarget, Position, SourceLocation,
    Warning, WarningKind, warning::WarningCollection,
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
/// For entry preprocessors the position is the start of the document. For
/// include processors it is the directive in its containing source.
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

/// Supplies bytes for a resolved include. The host selects lines, enforces
/// budgets, resolves nested targets and maps locations in the supplied source.
pub trait IncludeProcessor {
    /// The first processor returning a source handles the directive.
    /// Return `Ok(None)` to try the next processor or the configured loader.
    ///
    /// # Errors
    /// Return a message to stop parsing at the include directive.
    fn process(
        &mut self,
        target: &IncludeSourceTarget,
        context: &SourceProcessorContext<'_>,
    ) -> Result<Option<IncludeSource>, String>;
}

impl<F> IncludeProcessor for F
where
    F: FnMut(
        &IncludeSourceTarget,
        &SourceProcessorContext<'_>,
    ) -> Result<Option<IncludeSource>, String>,
{
    fn process(
        &mut self,
        target: &IncludeSourceTarget,
        context: &SourceProcessorContext<'_>,
    ) -> Result<Option<IncludeSource>, String> {
        self(target, context)
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

/// Nested include scopes share the active document's processor instances.
pub(crate) struct IncludeProcessors(RefCell<Box<[Box<dyn IncludeProcessor>]>>);

impl IncludeProcessors {
    pub(super) fn new(registrations: &[SourceRegistration<dyn IncludeProcessor>]) -> Option<Self> {
        (!registrations.is_empty()).then(|| {
            Self(RefCell::new(
                registrations.iter().map(|r| (r.factory)()).collect(),
            ))
        })
    }

    pub(crate) fn process(
        &self,
        target: &IncludeSourceTarget,
        context: &SourceProcessorContext<'_>,
    ) -> Result<Option<IncludeSource>, Error> {
        for processor in self.0.borrow_mut().iter_mut() {
            if let Some(source) = processor
                .process(target, context)
                .map_err(|message| context.error(message))?
            {
                return Ok(Some(source));
            }
        }
        Ok(None)
    }
}

impl fmt::Debug for IncludeProcessors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IncludeProcessors").finish_non_exhaustive()
    }
}
