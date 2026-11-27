use std::sync::Arc;

use super::{ExtensionBlock, InlineSpec};
use bumpalo::Bump;

use crate::{
    Anchor, Block, BlockMetadata, Caption, CaptionKind, CompatibilityMode, DocumentAttributes,
    Error, Image, InlineNode, ListItem, Location, Paragraph, Plain, Source, Title, UnorderedList,
    Verbatim, grammar::ParserState, model::Delimited,
};

#[derive(Clone, Debug)]
pub(crate) struct SourceContext<'a> {
    pub(crate) attributes: DocumentAttributes<'a>,
    pub(crate) mode: CompatibilityMode,
}

impl<'a> SourceContext<'a> {
    pub(crate) fn capture(state: &crate::grammar::ParserState<'a>) -> Option<Arc<Self>> {
        if !state.options.extensions.as_ref()?.has_tree_processors() {
            return None;
        }
        Some(Arc::new(Self {
            attributes: state.document_attributes.as_ref().clone(),
            mode: state.options.compatibility_mode,
        }))
    }
}

/// Ordinary content whose strings are allocated by the document host.
///
/// Body text is literal. Converters apply the block's normal output policies.
/// Attribute lists use ordinary metadata interpretation and source-position
/// substitutions. Construction never invokes block grammar or callbacks.
#[derive(Clone, Debug)]
pub struct BlockBuilder {
    pub(crate) content: Content,
    pub(super) id: Option<String>,
    pub(crate) attributes: Vec<String>,
    pub(crate) defaults: Option<String>,
    pub(crate) unsets: Vec<String>,
    caption: Option<CaptionKind>,
    pub(crate) source_body: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum Content {
    Paragraph(String),
    UnorderedList(Vec<InlineSpec>),
    ParagraphInlines(Vec<InlineSpec>),
    Image(String),
    Literal(String),
    Listing(String),
}

impl BlockBuilder {
    pub(crate) fn is_image(&self) -> bool {
        matches!(self.content, Content::Image(_))
    }
    pub(crate) fn is_verbatim(&self) -> bool {
        matches!(self.content, Content::Literal(_) | Content::Listing(_))
    }
    pub(crate) fn is_listing(&self) -> bool {
        matches!(self.content, Content::Listing(_))
    }
    pub(super) fn caption_kind(&self, style: Option<&str>) -> Option<CaptionKind> {
        if self.caption.is_some() {
            return self.caption;
        }
        match &self.content {
            Content::Paragraph(_) | Content::ParagraphInlines(_) => CaptionKind::for_style(style),
            Content::Image(_) => Some(CaptionKind::Figure),
            Content::Listing(_) => CaptionKind::for_verbatim(style, Some(CaptionKind::Listing)),
            Content::Literal(_) => CaptionKind::for_verbatim(style, None),
            Content::UnorderedList(_) => None,
        }
    }

    fn new(content: Content) -> Self {
        Self {
            content,
            id: None,
            attributes: Vec::new(),
            defaults: None,
            unsets: Vec::new(),
            caption: None,
            source_body: false,
        }
    }

    /// Build a paragraph of literal text.
    #[must_use]
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::new(Content::Paragraph(text.into()))
    }

    /// Build an unordered list with literal text or structured inline content.
    ///
    /// Strings remain literal. Arrays and vectors of [`InlineSpec`] group
    /// fragments into one item; an individual specification can also be an item.
    ///
    /// ```
    /// use acdc_parser::extensions::{BlockBuilder, InlineSpec};
    ///
    /// let literal = BlockBuilder::unordered_list(["One", "Two"]);
    /// let owned = BlockBuilder::unordered_list(vec![String::from("Owned")]);
    /// let structured = BlockBuilder::unordered_list([
    ///     InlineSpec::from("Literal *markup*"),
    ///     InlineSpec::from([
    ///         InlineSpec::from("Several "),
    ///         InlineSpec::from("fragments"),
    ///     ]),
    /// ]);
    /// ```
    #[must_use]
    pub fn unordered_list(items: impl IntoIterator<Item = impl Into<InlineSpec>>) -> Self {
        Self::new(Content::UnorderedList(
            items.into_iter().map(Into::into).collect(),
        ))
    }

    /// Build a paragraph from literal text, hard breaks, groups, and captured displays.
    /// Sequences preserve fragment order without adding spacing or breaks.
    #[must_use]
    pub fn paragraph_inlines(content: impl IntoIterator<Item = impl Into<InlineSpec>>) -> Self {
        Self::new(Content::ParagraphInlines(
            content.into_iter().map(Into::into).collect(),
        ))
    }

    pub(super) fn has_displays(&self) -> bool {
        match &self.content {
            Content::ParagraphInlines(content) | Content::UnorderedList(content) => {
                content.iter().any(InlineSpec::has_displays)
            }
            Content::Paragraph(_)
            | Content::Image(_)
            | Content::Literal(_)
            | Content::Listing(_) => false,
        }
    }

    /// Build an ordinary image without interpreting its target as markup.
    #[must_use]
    pub fn image(target: impl Into<String>) -> Self {
        Self::new(Content::Image(target.into()))
    }

    /// Build literal text, preserving whitespace and markup characters.
    #[must_use]
    pub fn literal(text: impl Into<String>) -> Self {
        Self::new(Content::Literal(text.into()))
    }

    /// Build listing text, preserving whitespace and markup characters.
    #[must_use]
    pub fn listing(text: impl Into<String>) -> Self {
        Self::new(Content::Listing(text.into()))
    }

    /// Interpret an attribute list through the parser's ordinary attribute rules.
    /// Supply the list contents without square brackets. This interprets metadata,
    /// not the generated body, and never invokes block processors.
    #[must_use]
    pub fn with_attributes(mut self, source: impl Into<String>) -> Self {
        self.attributes.push(source.into());
        self
    }

    /// Supply weak attribute defaults. Inherited and explicit attributes take precedence.
    #[must_use]
    pub fn with_default_attributes(mut self, source: impl Into<String>) -> Self {
        self.defaults = Some(source.into());
        self
    }

    /// Remove an inherited or default attribute, including dedicated metadata.
    #[must_use]
    pub fn without_attribute(mut self, name: impl Into<String>) -> Self {
        self.unsets.push(name.into());
        self
    }

    /// Request a host caption category, including for textual diagram output.
    #[must_use]
    pub fn with_caption_kind(mut self, kind: CaptionKind) -> Self {
        self.caption = Some(kind);
        self
    }

    /// Request an explicit output ID. The editor rejects empty or duplicate IDs.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub(super) fn build_at_invocation<'a>(
        self,
        extension: &mut ExtensionBlock<'a>,
        state: &ParserState<'a>,
    ) -> Result<Block<'a>, Error> {
        if self.id.as_deref() == Some("") {
            return Err(Error::Extension(
                Box::new(extension.invocation.location().clone()),
                "output ID must not be empty".into(),
            ));
        }
        let prepared = state.prepare_block_output(&self, extension)?;
        let mut metadata = prepared.metadata;
        for warning in prepared.warnings {
            state.add_warning(warning);
        }
        if let Some(kind) = self.caption_kind(metadata.style) {
            metadata.caption = Some(Caption::resolve(
                &metadata,
                extension
                    .invocation
                    .document_attributes(&state.document_attributes),
                kind,
            ));
        }
        let location = &extension.location;
        let mut block = self.build(
            state.arena,
            location.clone(),
            metadata,
            prepared.title,
            |inline| {
                inline.build(state.arena, location, |_, _| {
                    Err(Error::Extension(
                        Box::new(extension.invocation.location().clone()),
                        "captured displays require a tree processor".into(),
                    ))
                })
            },
        )?;
        if let Block::Image(image) = &mut block {
            image.alt = prepared.alt;
        }
        Ok(block)
    }

    pub(super) fn build<'a>(
        self,
        arena: &'a Bump,
        location: Location,
        mut metadata: BlockMetadata<'a>,
        title: Title<'a>,
        mut inline: impl FnMut(InlineSpec) -> Result<Vec<InlineNode<'a>>, crate::Error>,
    ) -> Result<Block<'a>, crate::Error> {
        if let Some(id) = self.id {
            metadata.id = Some(Anchor::new(arena.alloc_str(&id), location.clone()));
        }
        let plain = |text: &str| {
            vec![InlineNode::Plain(Plain {
                content: arena.alloc_str(text),
                location: location.clone(),
                escaped: true,
            })]
        };
        Ok(match self.content {
            Content::Paragraph(text) => {
                let mut block = Paragraph::new(plain(&text), location);
                block.metadata = metadata;
                block.title = title;
                Block::Paragraph(block)
            }
            Content::UnorderedList(items) => build_list(
                items
                    .into_iter()
                    .map(&mut inline)
                    .collect::<Result<Vec<_>, _>>()?,
                location,
                metadata,
                title,
            ),
            Content::ParagraphInlines(content) => {
                let mut block = Paragraph::new(
                    content
                        .into_iter()
                        .map(&mut inline)
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .flatten()
                        .collect(),
                    location,
                );
                block.metadata = metadata;
                block.title = title;
                Block::Paragraph(block)
            }
            Content::Image(target) => Block::Image(Image {
                source: Source::from_str_borrowed(arena.alloc_str(&target))?,
                alt: None,
                title,
                metadata,
                location,
            }),
            Content::Literal(text) => {
                build_verbatim(arena, &text, false, location, metadata, title)
            }
            Content::Listing(text) => build_verbatim(arena, &text, true, location, metadata, title),
        })
    }
}

fn build_list<'a>(
    items: Vec<Vec<InlineNode<'a>>>,
    location: Location,
    metadata: BlockMetadata<'a>,
    title: Title<'a>,
) -> Block<'a> {
    Block::UnorderedList(UnorderedList {
        title,
        metadata,
        marker: "*",
        items: items
            .into_iter()
            .map(|principal| ListItem {
                level: 1,
                marker: "*",
                checked: None,
                principal,
                blocks: Vec::new(),
                location: location.clone(),
            })
            .collect(),
        location,
    })
}

fn build_verbatim<'a>(
    arena: &'a Bump,
    text: &str,
    listing: bool,
    location: Location,
    metadata: BlockMetadata<'a>,
    title: Title<'a>,
) -> Block<'a> {
    let text = arena.alloc_str(text);
    let nodes = vec![InlineNode::Verbatim(Verbatim::new(text, location.clone()))];
    let mut block = Delimited::new(nodes, if listing { "----" } else { "...." }, location);
    block.metadata = metadata;
    block.title = title;
    block.source_text = Some(text);
    if listing {
        Block::DelimitedListing(crate::Listing(block))
    } else {
        Block::DelimitedLiteral(crate::Literal(block))
    }
}
