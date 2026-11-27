use crate::{Block, InlineNode, Location};

use super::Error;

impl Error {
    pub(super) fn with_context(mut self, owner: &'static str, source: &Location) -> Self {
        if let Self::Unsupported {
            construct,
            location,
            ..
        }
        | Self::InvalidLocation {
            construct,
            location,
            ..
        } = &mut self
            && construct.is_none()
        {
            *construct = Some(owner);
            if source.start.line > 0
                && source.end.line > 0
                && [&source.start, &source.end]
                    .iter()
                    .all(|position| position.file.as_ref().is_none_or(|chain| !chain.is_empty()))
            {
                *location = Some(Box::new(source.clone()));
            }
        }
        self
    }
}

pub(super) fn inline_construct(value: &InlineNode<'_>) -> &'static str {
    match value {
        InlineNode::Extension(_) => "pending inline macro",
        InlineNode::Plain(_) => "plain text",
        InlineNode::Raw(_) => "raw text",
        InlineNode::Verbatim(_) => "verbatim text",
        InlineNode::Symbol(_) => "symbol",
        InlineNode::CharacterReference(_) => "character reference",
        InlineNode::StandaloneCurvedApostrophe(_) => "apostrophe",
        InlineNode::Bold(_) => "bold span",
        InlineNode::Italic(_) => "italic span",
        InlineNode::Monospace(_) => "monospace span",
        InlineNode::Highlight(_) => "highlight span",
        InlineNode::Subscript(_) => "subscript span",
        InlineNode::Superscript(_) => "superscript span",
        InlineNode::DoubleQuotedSpan(_) => "curved quotation",
        InlineNode::SingleQuotedSpan(_) => "curved apostrophe",
        InlineNode::LineBreak(_) => "line break",
        InlineNode::Anchor(_) => "inline anchor",
        InlineNode::Callout(_) => "callout",
        InlineNode::Footnote(_) => "footnote",
        InlineNode::Icon(_) => "icon",
        InlineNode::Image(_) => "inline image",
        InlineNode::KeyboardShortcut(_) => "keyboard",
        InlineNode::Button(_) => "button",
        InlineNode::MenuPath(_) => "menu",
        InlineNode::UrlMacro(_) => "URL",
        InlineNode::Link(_) => "link",
        InlineNode::Mailto(_) => "mail link",
        InlineNode::Autolink(_) => "autolink",
        InlineNode::CrossReference(_) => "cross-reference",
        InlineNode::Passthrough(_) => "passthrough",
        InlineNode::Stem(_) => "STEM",
        InlineNode::IndexTerm(_) => "index term",
    }
}

pub(super) fn block_construct(value: &Block<'_>) -> &'static str {
    match value {
        Block::Paragraph(_) => "paragraph",
        Block::Extension(_) => "pending extension block",
        Block::Section(_) => "section",
        Block::Comment(_) => "comment",
        Block::TableOfContents(_) => "table of contents",
        Block::Admonition(_) => "admonition",
        Block::DiscreteHeading(_) => "discrete heading",
        Block::DocumentAttribute(_) => "document attribute",
        Block::ThematicBreak(_) => "thematic break",
        Block::PageBreak(_) => "page break",
        Block::UnorderedList(_) => "unordered list",
        Block::OrderedList(_) => "ordered list",
        Block::CalloutList(_) => "callout list",
        Block::DescriptionList(_) => "description list",
        Block::Image(_) => "block image",
        Block::Audio(_) => "audio",
        Block::Video(_) => "video",
        Block::DelimitedComment(_) => "delimited comment",
        Block::DelimitedExample(_) => "example",
        Block::DelimitedListing(_) => "listing",
        Block::DelimitedLiteral(_) => "literal",
        Block::DelimitedOpen(_) => "open block",
        Block::DelimitedSidebar(_) => "sidebar",
        Block::DelimitedTable(_) => "table",
        Block::DelimitedPassthrough(_) => "passthrough block",
        Block::DelimitedQuote(_) => "quote",
        Block::DelimitedVerse(_) => "verse",
        Block::DelimitedStem(_) => "STEM block",
    }
}
