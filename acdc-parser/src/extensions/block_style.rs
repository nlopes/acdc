//! Registered styles that consume the preprocessed text of a delimited block.

use std::sync::Arc;

use crate::{
    BlockMetadata, ElementAttributes, SourceLocation, Substitution, ast::DocumentAttributesRef,
};

use super::{BlockBuilder, TitleSource, registry::ProcessorFactory};

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
    pub(super) warnings: &'a mut Vec<String>,
}

impl RawBlockInput<'_> {
    /// Report a nonfatal warning at the invocation's source location.
    /// The handler can still return any output supported by the style.
    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    /// Ordered substitutions the source block would apply to its body.
    #[must_use]
    pub fn substitutions(&self) -> Vec<Substitution> {
        self.metadata.substitutions.as_ref().map_or_else(
            || crate::VERBATIM.to_vec(),
            |spec| spec.resolve(crate::VERBATIM),
        )
    }
}

/// Result of a style callback registered with [`BlockStyle::compound`].
#[derive(Debug)]
#[non_exhaustive]
pub enum BlockStyleOutput {
    /// Parse this invocation's body as child blocks of an ordinary open block.
    ParseChildren,
    /// Emit the builder at the invocation's source location.
    Block(BlockBuilder),
    /// Consume the invocation and its body.
    None,
}

/// Maximum number of simultaneously active style-requested child parses.
pub const MAX_STYLE_NESTING: usize = 8;

/// A document-local handler for a registered raw block style.
pub trait RawBlockProcessor<Output = Option<BlockBuilder>> {
    /// Select the output of the registered style.
    ///
    /// # Errors
    /// An error stops parsing at the styled block's source location.
    fn process(&mut self, block: RawBlockInput<'_>) -> Result<Output, String>;
}

impl<F, Output> RawBlockProcessor<Output> for F
where
    F: FnMut(RawBlockInput<'_>) -> Result<Output, String>,
{
    fn process(&mut self, block: RawBlockInput<'_>) -> Result<Output, String> {
        self(block)
    }
}

/// A named block style, its supported source forms, and its per-document factory.
#[derive(Clone, Debug)]
pub struct BlockStyle {
    pub(super) handler: ProcessorFactory<dyn RawBlockProcessor<BlockStyleOutput>>,
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
        Self::compound(name, contexts, move || {
            let mut handler = factory();
            move |input: RawBlockInput<'_>| {
                handler
                    .process(input)
                    .map(|block| block.map_or(BlockStyleOutput::None, BlockStyleOutput::Block))
            }
        })
    }

    /// Register a processor that may ask the host to parse this body's blocks.
    /// Like [`Self::raw`], the processor receives the complete preprocessed body.
    /// Child callbacks run in source order after this callback returns.
    ///
    /// ```
    /// use acdc_parser::{Options, ast::BlockKind, extensions::{
    ///     BlockStyle, BlockStyleOutput, RawBlockContext, RawBlockInput, Registry,
    /// }};
    ///
    /// let mut registry = Registry::builder();
    /// registry.register_block_style(BlockStyle::compound("panel", [RawBlockContext::Listing], || {
    ///     |_: RawBlockInput<'_>| Ok(BlockStyleOutput::ParseChildren)
    /// }))?;
    /// let parsed = registry.build().parse("[panel]\n----\nA paragraph.\n----", &Options::default())?.finish()?;
    /// let block = parsed.document().blocks().next().unwrap();
    /// assert!(matches!(block.kind(), BlockKind::Open(panel) if panel.blocks().count() == 1));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn compound<P: RawBlockProcessor<BlockStyleOutput> + 'static>(
        name: impl Into<String>,
        contexts: impl IntoIterator<Item = RawBlockContext>,
        factory: impl Fn() -> P + Send + Sync + 'static,
    ) -> Self {
        Self {
            handler: ProcessorFactory {
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
