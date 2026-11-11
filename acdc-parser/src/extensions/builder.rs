use bumpalo::Bump;

use crate::{
    Anchor, Block, BlockMetadata, CaptionKind, InlineNode, ListItem, Location, Paragraph, Plain,
    Title, UnorderedList,
};

/// Ordinary content whose strings are allocated by the document host.
///
/// Text is literal. Converters apply the block's normal output policies.
/// Construction does not invoke inline parsing, attribute substitution, or
/// extension callbacks.
#[derive(Debug)]
pub struct BlockBuilder {
    content: Content,
    pub(super) id: Option<String>,
}

#[derive(Debug)]
enum Content {
    Paragraph(String),
    UnorderedList(Vec<String>),
}

impl BlockBuilder {
    pub(super) fn caption_kind(&self, style: Option<&str>) -> Option<CaptionKind> {
        match &self.content {
            Content::Paragraph(_) => CaptionKind::for_style(style),
            Content::UnorderedList(_) => None,
        }
    }

    /// Build a paragraph of literal text.
    #[must_use]
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self {
            content: Content::Paragraph(text.into()),
            id: None,
        }
    }

    /// Build an unordered list whose items contain literal text.
    #[must_use]
    pub fn unordered_list(items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            content: Content::UnorderedList(items.into_iter().map(Into::into).collect()),
            id: None,
        }
    }

    /// Request an explicit output ID. The editor rejects empty or duplicate IDs.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub(super) fn build<'a>(
        self,
        arena: &'a Bump,
        location: Location,
        mut metadata: BlockMetadata<'a>,
        title: Title<'a>,
    ) -> Block<'a> {
        if let Some(id) = self.id {
            metadata.id = Some(Anchor::new(arena.alloc_str(&id), location.clone()));
        }
        let plain = |text: &str| {
            vec![InlineNode::PlainText(Plain {
                content: arena.alloc_str(text),
                location: location.clone(),
                escaped: true,
            })]
        };
        match self.content {
            Content::Paragraph(text) => {
                let mut block = Paragraph::new(plain(&text), location);
                block.metadata = metadata;
                block.title = title;
                Block::Paragraph(block)
            }
            Content::UnorderedList(items) => Block::UnorderedList(UnorderedList {
                title,
                metadata,
                items: items
                    .into_iter()
                    .map(|text| ListItem {
                        level: 1,
                        marker: "*",
                        checked: None,
                        principal: plain(&text),
                        blocks: Vec::new(),
                        location: location.clone(),
                    })
                    .collect(),
                marker: "*",
                location,
            }),
        }
    }
}
