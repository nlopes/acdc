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
            factory: Arc::new(move || Box::new(factory())),
        }
    }
}

impl fmt::Debug for TreeProcessorRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TreeProcessorRegistration")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
