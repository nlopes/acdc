//! Trusted fragments inserted into the selected backend's document frame.
use std::{fmt, sync::Arc};

use acdc_parser::ast::DocumentRef;

use crate::{Diagnostics, TraversalContext, render::RenderError};

/// Full HTML supports these positions, in order. Embedded HTML has no document frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocinfoPosition {
    /// Immediately before the closing head tag.
    Head,
    /// Immediately after the opening body tag.
    Header,
    /// After the document footer and before closing the body.
    Footer,
}

/// Document and attributes at this insertion point. Footer hooks observe body assignments.
#[derive(Clone, Copy)]
pub struct DocinfoContext<'a> {
    /// Actual backend variant.
    pub backend: &'a str,
    /// Insertion point reached during host traversal.
    pub position: DocinfoPosition,
    /// Prepared document being rendered.
    pub document: DocumentRef<'a>,
    /// Attribute values at the insertion point.
    pub attributes: &'a TraversalContext<'a>,
}

/// One conversion's callback. Return trusted backend markup or omit the fragment.
/// The host appends a newline to a present fragment and never interprets or escapes it.
/// Authors must escape document-derived values for their insertion context.
pub type DocinfoProcessor =
    dyn FnMut(DocinfoContext<'_>, &mut Diagnostics<'_>) -> Result<Option<String>, RenderError>;

/// A docinfo factory for an explicitly selected backend family.
#[derive(Clone)]
pub struct DocinfoHook {
    pub(crate) backend: &'static str,
    pub(crate) factory: Arc<dyn Fn() -> Box<DocinfoProcessor> + Send + Sync>,
}

impl fmt::Debug for DocinfoHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocinfoHook")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl DocinfoHook {
    /// Create one mutable callback per full document, shared by its insertion points.
    /// Registration and embedded conversions never execute the factory.
    pub fn new<F>(backend: &'static str, factory: impl Fn() -> F + Send + Sync + 'static) -> Self
    where
        F: FnMut(DocinfoContext<'_>, &mut Diagnostics<'_>) -> Result<Option<String>, RenderError>
            + 'static,
    {
        Self {
            backend,
            factory: Arc::new(move || Box::new(factory())),
        }
    }
}
