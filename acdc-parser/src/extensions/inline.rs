//! Owned specifications lowered through the host's checked construction path.

use bumpalo::Bump;

use crate::{Document, Error, InlineNode, LineBreak, Plain, SourceLocation};

use super::{DisplayRequest, target::DocumentIdentity};

/// Declarative input for generated inline content.
///
/// Text is literal. Sequences preserve fragment order without adding spaces or
/// breaks. A sequence supplied as one list item remains one item, even when empty.
/// The document host allocates nodes and validates captured display requests
/// when applying an edit; creating a specification does not interpret markup.
///
/// ```
/// use acdc_parser::extensions::{BlockBuilder, InlineSpec};
///
/// let paragraph = BlockBuilder::paragraph_inlines([
///     InlineSpec::Text("Literal *markup*".into()),
///     InlineSpec::LineBreak,
///     InlineSpec::Sequence(vec!["Second ".into(), "line".into()]),
/// ]);
/// let text = BlockBuilder::paragraph_inlines(["Several ", "fragments"]);
/// let list = BlockBuilder::unordered_list([
///     InlineSpec::from(["One ".into(), "item".into()]),
///     InlineSpec::Sequence(Vec::new()),
/// ]);
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum InlineSpec {
    /// Literal text, escaped by the ordinary converter pipeline.
    Text(String),
    /// An explicit hard break at the containing generated block's location.
    LineBreak,
    /// A symbolic reference created by [`super::CapturedTarget::display`].
    Display(DisplayRequest),
    /// Transparent composition of zero or more inline specifications.
    Sequence(Vec<Self>),
}

impl InlineSpec {
    pub(super) fn has_displays(&self) -> bool {
        match self {
            Self::Text(_) | Self::LineBreak => false,
            Self::Display(_) => true,
            Self::Sequence(content) => content.iter().any(Self::has_displays),
        }
    }

    pub(super) fn build<'a>(
        self,
        arena: &'a Bump,
        document: &Document<'a>,
        identity: &DocumentIdentity,
        origin: &SourceLocation,
    ) -> Result<Vec<InlineNode<'a>>, Error> {
        let mut nodes = Vec::new();
        self.build_into(arena, document, identity, origin, &mut nodes)?;
        Ok(nodes)
    }

    fn build_into<'a>(
        self,
        arena: &'a Bump,
        document: &Document<'a>,
        identity: &DocumentIdentity,
        origin: &SourceLocation,
        nodes: &mut Vec<InlineNode<'a>>,
    ) -> Result<(), Error> {
        match self {
            Self::Text(text) => nodes.push(plain(arena.alloc_str(&text), origin)),
            Self::LineBreak => {
                nodes.push(InlineNode::LineBreak(LineBreak::new(
                    origin.location.clone(),
                )));
            }
            Self::Display(request) => request.build_into(document, identity, origin, nodes)?,
            Self::Sequence(content) => {
                for inline in content {
                    inline.build_into(arena, document, identity, origin, nodes)?;
                }
            }
        }
        Ok(())
    }
}

impl From<String> for InlineSpec {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for InlineSpec {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}

impl From<&String> for InlineSpec {
    fn from(text: &String) -> Self {
        Self::Text(text.clone())
    }
}

impl FromIterator<Self> for InlineSpec {
    fn from_iter<T: IntoIterator<Item = Self>>(content: T) -> Self {
        Self::Sequence(content.into_iter().collect())
    }
}

impl From<Vec<Self>> for InlineSpec {
    fn from(content: Vec<Self>) -> Self {
        Self::Sequence(content)
    }
}

impl<const N: usize> From<[Self; N]> for InlineSpec {
    fn from(content: [Self; N]) -> Self {
        content.into_iter().collect()
    }
}

pub(super) fn plain<'a>(text: &'a str, origin: &SourceLocation) -> InlineNode<'a> {
    InlineNode::PlainText(Plain {
        content: text,
        location: origin.location.clone(),
        escaped: true,
    })
}
