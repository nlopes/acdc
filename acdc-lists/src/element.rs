//! The block kinds a list can be built from.
//!
//! asciidoctor-lists passes the macro target straight to Asciidoctor's
//! `find_by(context:)`, so the names a document writes are Asciidoctor's
//! context names — `image`, `listing`, `olist` — rather than anything the
//! extension invents. The same names are accepted here, and mapped onto acdc's
//! AST, which splits some of them differently: a table and a listing are both
//! `DelimitedBlock` in acdc, told apart by their inner content.

use acdc_parser::{Block, DelimitedBlockType};

/// A block kind that `list-of::` can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Element {
    /// A block image, controlled by `figure-caption`.
    Image,
    /// A table, controlled by `table-caption`.
    Table,
    /// A listing or source block, controlled by `listing-caption`.
    Listing,
    /// A literal block.
    Literal,
    /// An example block, controlled by `example-caption`.
    Example,
    /// A quote block.
    Quote,
    /// A verse block.
    Verse,
    /// A sidebar block.
    Sidebar,
    /// An open block.
    Open,
    /// A passthrough block.
    Pass,
    /// A block of mathematical notation.
    Stem,
    /// An audio block.
    Audio,
    /// A video block.
    Video,
    /// An admonition.
    Admonition,
    /// A paragraph.
    Paragraph,
    /// A section.
    Section,
    /// An ordered list.
    OrderedList,
    /// An unordered list.
    UnorderedList,
    /// A description list.
    DescriptionList,
    /// A callout list.
    CalloutList,
}

/// Every element name `list-of::` accepts, in the order they are reported.
pub(crate) const NAMES: &[&str] = &[
    "image",
    "table",
    "listing",
    "literal",
    "example",
    "quote",
    "verse",
    "sidebar",
    "open",
    "pass",
    "stem",
    "audio",
    "video",
    "admonition",
    "paragraph",
    "section",
    "olist",
    "ulist",
    "dlist",
    "colist",
];

/// Advice listing every name a `list-of::` call may use.
pub(crate) fn names_advice() -> String {
    format!("Use one of: {}.", NAMES.join(", "))
}

impl Element {
    /// Recognise an element name, as Asciidoctor spells its block contexts.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.trim() {
            "image" => Element::Image,
            "table" => Element::Table,
            "listing" => Element::Listing,
            "literal" => Element::Literal,
            "example" => Element::Example,
            "quote" => Element::Quote,
            "verse" => Element::Verse,
            "sidebar" => Element::Sidebar,
            "open" => Element::Open,
            "pass" => Element::Pass,
            "stem" => Element::Stem,
            "audio" => Element::Audio,
            "video" => Element::Video,
            "admonition" => Element::Admonition,
            "paragraph" => Element::Paragraph,
            "section" => Element::Section,
            "olist" => Element::OrderedList,
            "ulist" => Element::UnorderedList,
            "dlist" => Element::DescriptionList,
            "colist" => Element::CalloutList,
            _ => return None,
        })
    }

    /// The name this element is written as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Element::Image => "image",
            Element::Table => "table",
            Element::Listing => "listing",
            Element::Literal => "literal",
            Element::Example => "example",
            Element::Quote => "quote",
            Element::Verse => "verse",
            Element::Sidebar => "sidebar",
            Element::Open => "open",
            Element::Pass => "pass",
            Element::Stem => "stem",
            Element::Audio => "audio",
            Element::Video => "video",
            Element::Admonition => "admonition",
            Element::Paragraph => "paragraph",
            Element::Section => "section",
            Element::OrderedList => "olist",
            Element::UnorderedList => "ulist",
            Element::DescriptionList => "dlist",
            Element::CalloutList => "colist",
        }
    }

    /// Whether `block` is one of these.
    pub(crate) fn matches(self, block: &Block<'_>) -> bool {
        match block {
            Block::Image(_) => self == Element::Image,
            Block::Audio(_) => self == Element::Audio,
            Block::Video(_) => self == Element::Video,
            Block::Admonition(_) => self == Element::Admonition,
            Block::Paragraph(_) => self == Element::Paragraph,
            Block::Section(_) => self == Element::Section,
            Block::OrderedList(_) => self == Element::OrderedList,
            Block::UnorderedList(_) => self == Element::UnorderedList,
            Block::DescriptionList(_) => self == Element::DescriptionList,
            Block::CalloutList(_) => self == Element::CalloutList,
            Block::DelimitedBlock(delimited) => self.matches_delimited(&delimited.inner),
            // `Block` is non-exhaustive, and none of these carries a context a
            // list can be built from.
            Block::TableOfContents(_)
            | Block::DiscreteHeader(_)
            | Block::DocumentAttribute(_)
            | Block::ThematicBreak(_)
            | Block::PageBreak(_)
            | Block::Comment(_)
            | _ => false,
        }
    }

    fn matches_delimited(self, inner: &DelimitedBlockType<'_>) -> bool {
        let kind = match inner {
            DelimitedBlockType::DelimitedTable(_) => Element::Table,
            // A `[source]` block is a listing that carries a style, which is
            // how Asciidoctor models it too, so `list-of::listing[]` lists
            // both.
            DelimitedBlockType::DelimitedListing(_) => Element::Listing,
            DelimitedBlockType::DelimitedLiteral(_) => Element::Literal,
            DelimitedBlockType::DelimitedExample(_) => Element::Example,
            DelimitedBlockType::DelimitedQuote(_) => Element::Quote,
            DelimitedBlockType::DelimitedVerse(_) => Element::Verse,
            DelimitedBlockType::DelimitedSidebar(_) => Element::Sidebar,
            DelimitedBlockType::DelimitedOpen(_) => Element::Open,
            DelimitedBlockType::DelimitedPass(_) => Element::Pass,
            DelimitedBlockType::DelimitedStem(_) => Element::Stem,
            // A comment is not rendered, so it is never listed; the wildcard
            // covers the non-exhaustive enum growing a variant later.
            DelimitedBlockType::DelimitedComment(_) | _ => return false,
        };
        self == kind
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn every_listed_name_parses_and_round_trips() {
        for name in NAMES {
            let element = Element::parse(name).expect("listed name parses");
            assert_eq!(element.as_str(), *name);
        }
    }

    #[test]
    fn rejects_unknown_names() {
        assert!(Element::parse("figure").is_none());
        assert!(Element::parse("").is_none());
    }

    #[test]
    fn ignores_surrounding_whitespace() {
        assert_eq!(Element::parse(" table "), Some(Element::Table));
    }
}
