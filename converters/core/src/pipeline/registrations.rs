use acdc_parser::extensions::Registry;

use crate::{
    BackendTarget,
    docinfo::{DocinfoHook, DocinfoProcessor},
    postprocess::TextPostprocessor,
    render::{BlockRenderer, RenderSession},
};

/// Immutable parser and renderer registrations, available or explicitly selected.
/// Cloning shares factories; every parse and conversion gets fresh mutable processors.
#[derive(Clone, Debug, Default)]
pub struct Extensions {
    pub(super) parser: Option<Registry>,
    pub(super) postprocessors: Vec<TextPostprocessor>,
    pub(super) docinfo: Vec<DocinfoHook>,
    pub(super) renderers: Vec<BlockRenderer>,
}

impl From<Registry> for Extensions {
    fn from(registry: Registry) -> Self {
        Self {
            parser: (!registry.is_empty()).then_some(registry),
            renderers: Vec::new(),
            docinfo: Vec::new(),
            postprocessors: Vec::new(),
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

    /// Append a completed-text callback. Matching callbacks edit the same string
    /// in package selection and registration order.
    #[must_use]
    pub fn with_postprocessor(mut self, processor: TextPostprocessor) -> Self {
        self.postprocessors.push(processor);
        self
    }

    pub(super) fn postprocessors(
        &self,
        name: &'static str,
        active: BackendTarget<'_>,
    ) -> impl Iterator<Item = &TextPostprocessor> {
        self.postprocessors
            .iter()
            .filter(move |p| p.matches(name, active))
    }

    /// Append a docinfo hook. At each insertion point hooks run in selection
    /// and registration order, following any file-based docinfo.
    #[must_use]
    pub fn with_docinfo(mut self, hook: DocinfoHook) -> Self {
        self.docinfo.push(hook);
        self
    }

    /// Whether a backend family has selected renderers or supported docinfo hooks.
    #[must_use]
    pub fn has_render_hooks<'a>(
        &self,
        backend: impl Into<BackendTarget<'a>>,
        full_document: bool,
    ) -> bool {
        let active = backend.into();
        self.renderers.iter().any(|r| r.target.matches(active))
            || (full_document && self.docinfo.iter().any(|r| r.target.matches(active)))
    }

    /// Instantiate docinfo processors for a backend's available insertion points.
    #[must_use]
    pub fn docinfo_processors<'a>(
        &self,
        backend: impl Into<BackendTarget<'a>>,
    ) -> Vec<Box<DocinfoProcessor>> {
        let active = backend.into();
        self.docinfo
            .iter()
            .filter(|r| r.target.matches(active))
            .map(|r| (r.factory)())
            .collect()
    }

    /// Parser registrations, for callers that need a working edit session.
    #[must_use]
    pub fn parser_registry(&self) -> Option<&Registry> {
        self.parser.as_ref()
    }

    /// Whether this collection has no parser or rendering capabilities.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parser.is_none()
            && self.renderers.is_empty()
            && self.docinfo.is_empty()
            && self.postprocessors.is_empty()
    }

    /// Trusted script hashes from renderers registered for the whole HTML family.
    pub fn html_script_hashes(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.html_script_hashes_for_variant(None)
    }

    /// Trusted hashes for renderers matching the active HTML family and variant.
    pub fn html_script_hashes_for_variant<'a>(
        &'a self,
        variant: Option<&'a str>,
    ) -> impl Iterator<Item = &'static str> + 'a {
        let active = BackendTarget {
            family: "html",
            variant,
        };
        self.renderers
            .iter()
            .filter(move |r| r.target.matches(active))
            .flat_map(|r| r.html_script_hashes().iter().copied())
    }

    /// Construct document-local renderers for a backend family, in registration order.
    /// Other backends invoke no factories. Empty selections allocate no session.
    #[must_use]
    pub fn render_session<'a>(
        &self,
        backend: impl Into<BackendTarget<'a>>,
    ) -> Option<RenderSession> {
        let active = backend.into();
        let processors: Vec<_> = self
            .renderers
            .iter()
            .filter(|registration| registration.target.matches(active))
            .map(|registration| (registration.factory)())
            .collect();
        (!processors.is_empty()).then_some(RenderSession::new(processors))
    }
}
