//! Registered styles that consume the preprocessed text of a delimited block.

use std::sync::Arc;

use crate::{
    BlockMetadata, ElementAttributes, SourceLocation, Substitution, ast::DocumentAttributesRef,
};

use super::{BlockBuilder, TitleSource, source::SourceRegistration};

/// Delimited source forms supported by a raw block style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RawBlockContext {
    Listing,
    Literal,
}

/// Input borrowed by a [`RawBlockProcessor`] before the host interprets the block body.
#[derive(Debug)]
#[non_exhaustive]
pub struct RawBlockInput<'a> {
    pub context: RawBlockContext,
    /// Text between the delimiter lines, including its final newline when present.
    pub body: &'a str,
    pub body_location: &'a SourceLocation,
    /// The entire invocation, starting at its first metadata line.
    pub location: &'a SourceLocation,
    pub title: Option<&'a TitleSource<'a>>,
    pub metadata: &'a BlockMetadata<'a>,
    pub document_attributes: DocumentAttributesRef<'a>,
}

impl RawBlockInput<'_> {
    /// Ordered substitutions the source block would apply to its body.
    #[must_use]
    pub fn substitutions(&self) -> Vec<Substitution> {
        self.metadata.substitutions.as_ref().map_or_else(
            || crate::VERBATIM.to_vec(),
            |spec| spec.resolve(crate::VERBATIM),
        )
    }
}

/// A document-local handler for a registered raw block style.
pub trait RawBlockProcessor {
    /// Return an ordinary block, or `None` to consume this invocation.
    ///
    /// # Errors
    /// An error stops parsing at the styled block's source location.
    fn process(&mut self, block: RawBlockInput<'_>) -> Result<Option<BlockBuilder>, String>;
}

impl<F> RawBlockProcessor for F
where
    F: FnMut(RawBlockInput<'_>) -> Result<Option<BlockBuilder>, String>,
{
    fn process(&mut self, block: RawBlockInput<'_>) -> Result<Option<BlockBuilder>, String> {
        self(block)
    }
}

/// A named block style, its supported source forms, and its per-document factory.
#[derive(Clone, Debug)]
pub struct BlockStyle {
    pub(super) handler: SourceRegistration<dyn RawBlockProcessor>,
    pub(super) contexts: Vec<RawBlockContext>,
    positional: Vec<String>,
    defaults: ElementAttributes<'static>,
}

impl BlockStyle {
    /// Consume a raw body once its listing or literal delimiter has committed.
    ///
    /// ```
    /// use acdc_parser::{Options, extensions::{BlockBuilder, BlockStyle, RawBlockContext, RawBlockInput, Registry}};
    ///
    /// let mut registry = Registry::builder();
    /// registry.register_block_style(BlockStyle::raw("sketch", [RawBlockContext::Listing], || {
    ///     |input: RawBlockInput<'_>| Ok(Some(BlockBuilder::literal(input.body)))
    /// }))?;
    /// let parsed = registry.build().parse("[sketch]\n----\n{x} *raw*\n----", &Options::default())?.finish()?;
    /// assert_eq!(parsed.document().blocks().count(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn raw<P: RawBlockProcessor + 'static>(
        name: impl Into<String>,
        contexts: impl IntoIterator<Item = RawBlockContext>,
        factory: impl Fn() -> P + Send + Sync + 'static,
    ) -> Self {
        Self {
            handler: SourceRegistration {
                name: name.into(),
                factory: Arc::new(move || Box::new(factory())),
            },
            contexts: contexts.into_iter().collect(),
            positional: Vec::new(),
            defaults: ElementAttributes::default(),
        }
    }

    /// Name positional arguments after the style. Explicit named arguments win.
    #[must_use]
    pub fn with_positional_attributes(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.positional = names.into_iter().map(Into::into).collect();
        self
    }

    /// Supply fallback values for missing processor arguments.
    #[must_use]
    pub fn with_defaults(mut self, defaults: ElementAttributes<'static>) -> Self {
        self.defaults = defaults;
        self
    }

    pub(crate) fn bind_arguments(&self, metadata: &mut BlockMetadata<'_>) {
        let mut attributes = std::mem::take(&mut metadata.attributes);
        super::recognition::bind_arguments(
            &mut attributes,
            &self.positional,
            metadata.positional_attributes(),
            &self.defaults,
        );
        metadata.attributes = attributes;
    }
}
