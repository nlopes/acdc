use acdc_parser::extensions::Registry;

use crate::render::{BlockRenderer, RenderSession};

/// Immutable parser and renderer registrations, available or explicitly selected.
/// Cloning shares factories; every parse and conversion gets fresh mutable processors.
#[derive(Clone, Debug, Default)]
pub struct Extensions {
    pub(super) parser: Option<Registry>,
    pub(super) renderers: Vec<BlockRenderer>,
}

impl From<Registry> for Extensions {
    fn from(registry: Registry) -> Self {
        Self {
            parser: (!registry.is_empty()).then_some(registry),
            renderers: Vec::new(),
        }
    }
}

impl Extensions {
    /// Append a renderer without constructing or executing it.
    #[must_use]
    pub fn with_renderer(mut self, registration: BlockRenderer) -> Self {
        self.renderers.push(registration);
        self
    }

    /// Parser registrations, for callers that need a working edit session.
    #[must_use]
    pub fn parser_registry(&self) -> Option<&Registry> {
        self.parser.as_ref()
    }

    /// Whether this collection has no parser or rendering capabilities.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parser.is_none() && self.renderers.is_empty()
    }

    /// Construct document-local renderers for a backend family, in registration order.
    /// Other backends invoke no factories. Empty selections allocate no session.
    #[must_use]
    pub fn render_session(&self, backend: &str) -> Option<RenderSession> {
        let processors: Vec<_> = self
            .renderers
            .iter()
            .filter(|registration| registration.backend == backend)
            .map(|registration| (registration.factory)())
            .collect();
        (!processors.is_empty()).then_some(RenderSession { processors })
    }
}
