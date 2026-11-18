use std::{fmt, sync::Arc};

use crate::Error;

use super::DocumentEditor;

/// A document-local processor invoked once after block macros finish.
///
/// Processors run in registration order. Each sees earlier edits; later edits
/// do not repeat an earlier processor's selection or side effects.
pub trait TreeProcessor {
    /// Inspect and edit the accepted document through the host.
    ///
    /// # Errors
    /// Return an error to stop processing and discard the document result.
    fn process(&mut self, document: &mut DocumentEditor<'_>) -> Result<(), Error>;
}

/// An immutable factory for a fresh tree processor in each document session.
#[derive(Clone)]
pub struct TreeProcessorRegistration {
    pub(super) name: String,
    pub(super) handles_deferred: bool,
    pub(super) factory: Arc<dyn Fn() -> Box<dyn TreeProcessor> + Send + Sync>,
}

impl TreeProcessorRegistration {
    /// Factories can share resources; their document-local processors need not be
    /// `Send` or `Sync`.
    pub fn new<P: TreeProcessor + 'static>(
        name: impl Into<String>,
        factory: impl Fn() -> P + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            handles_deferred: false,
            factory: Arc::new(move || Box::new(factory())),
        }
    }

    /// Resolve the pending calls of one previously registered block macro.
    /// The processor runs in tree registration order and can query the entire document.
    pub fn for_macro<P: TreeProcessor + 'static>(
        macro_name: impl Into<String>,
        factory: impl Fn() -> P + Send + Sync + 'static,
    ) -> Self {
        Self {
            handles_deferred: true,
            ..Self::new(macro_name, factory)
        }
    }
}

impl fmt::Debug for TreeProcessorRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TreeProcessorRegistration")
            .field("name", &self.name)
            .field("handles_deferred", &self.handles_deferred)
            .finish_non_exhaustive()
    }
}
