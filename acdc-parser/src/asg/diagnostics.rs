use crate::{Block, DelimitedBlockType, InlineMacro, InlineNode, Location};

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
        InlineNode::PlainText(_) => "plain text",
        InlineNode::RawText(_) => "raw text",
        InlineNode::VerbatimText(_) => "verbatim text",
        InlineNode::Symbol(_) => "symbol",
        InlineNode::CharacterReference(_) => "character reference",
        InlineNode::StandaloneCurvedApostrophe(_) => "apostrophe",
        InlineNode::BoldText(_) => "bold span",
        InlineNode::ItalicText(_) => "italic span",
        InlineNode::MonospaceText(_) => "monospace span",
        InlineNode::HighlightText(_) => "highlight span",
        InlineNode::SubscriptText(_) => "subscript span",
        InlineNode::SuperscriptText(_) => "superscript span",
        InlineNode::DoubleQuotedSpan(_) => "curved quotation",
        InlineNode::SingleQuotedSpan(_) => "curved apostrophe",
        InlineNode::LineBreak(_) => "line break",
        InlineNode::InlineAnchor(_) => "inline anchor",
        InlineNode::CalloutMarker(_) => "callout",
        InlineNode::Macro(value) => match value {
            InlineMacro::Footnote(_) => "footnote",
            InlineMacro::Icon(_) => "icon",
            InlineMacro::Image(_) => "inline image",
            InlineMacro::KeyboardShortcut(_) => "keyboard",
            InlineMacro::Button(_) => "button",
            InlineMacro::MenuPath(_) => "menu",
            InlineMacro::UrlMacro(_) => "URL",
            InlineMacro::Link(_) => "link",
            InlineMacro::Mailto(_) => "mail link",
            InlineMacro::Autolink(_) => "autolink",
            InlineMacro::CrossReference(_) => "cross-reference",
            InlineMacro::Passthrough(_) => "passthrough",
            InlineMacro::Stem(_) => "STEM",
            InlineMacro::IndexTerm(_) => "index term",
        },
    }
}

pub(super) fn block_construct(value: &Block<'_>) -> &'static str {
    match value {
        Block::Paragraph(_) => "paragraph",
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
        Block::DelimitedBlock(value) => match &value.inner {
            DelimitedBlockType::DelimitedComment(_) => "delimited comment",
            DelimitedBlockType::DelimitedExample(_) => "example",
            DelimitedBlockType::DelimitedListing(_) => "listing",
            DelimitedBlockType::DelimitedLiteral(_) => "literal",
            DelimitedBlockType::DelimitedOpen(_) => "open block",
            DelimitedBlockType::DelimitedSidebar(_) => "sidebar",
            DelimitedBlockType::DelimitedTable(_) => "table",
            DelimitedBlockType::DelimitedPass(_) => "passthrough block",
            DelimitedBlockType::DelimitedQuote(_) => "quote",
            DelimitedBlockType::DelimitedVerse(_) => "verse",
            DelimitedBlockType::DelimitedStem(_) => "STEM block",
        },
    }
}
