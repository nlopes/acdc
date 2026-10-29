//! AST nodes and content.
//!
//! Start with [`crate::ParseResult::document_ref`]. Handles borrow the tree;
//! copying a handle does not copy the tree. Iterators remain valid after the
//! handle is dropped, as long as the tree remains alive.
//!
//! ```
//! use acdc_parser::{Options, parse, ast::{BlockKind, InlineKind}};
//!
//! let parsed = parse("A paragraph.", &Options::default())?;
//! for block in parsed.document_ref().blocks() {
//!     if let BlockKind::Paragraph(paragraph) = block.kind() {
//!         for inline in paragraph.content().inlines() {
//!             if let InlineKind::Plain(text) = inline.kind() {
//!                 assert_eq!(text.text(), "A paragraph.");
//!             }
//!         }
//!     }
//! }
//! # Ok::<(), acdc_parser::Error>(())
//! ```
//!
//! To read a constructed node:
//!
//! ```
//! use acdc_parser::{Location, Paragraph, ast::ParagraphRef};
//!
//! let paragraph = Paragraph::new(Vec::new(), Location::default());
//! let node = ParagraphRef::from(&paragraph);
//! assert!(node.content().is_empty());
//! ```
//!
//! Kind matching covers every supported block and inline kind. Detailed reads
//! currently cover document/section children, their titles, paragraph bodies,
//! delimited blocks and admonitions, lists and their items, table rows and cells,
//! image targets and content, plain/raw/verbatim text, bold content, and locations.
//! The existing
//! owner API remains available for other reads and for construction and editing.
//!
//! Container blocks expose `blocks()`; inline bodies expose `content()`.
//! Formula blocks expose `text()` and `notation()`. Source text stays separate
//! from parsed content:
//!
//! ```
//! use acdc_parser::{Options, parse, ast::{BlockKind, InlineKind}};
//!
//! let parsed = parse("----\n*literal code*\n----\n", &Options::default())?;
//! let block = parsed.document_ref().blocks().next().unwrap();
//! if let BlockKind::Listing(listing) = block.kind() {
//!     assert_eq!(listing.source_text(), Some("*literal code*\n"));
//!     for inline in listing.content().inlines() {
//!         if let InlineKind::Verbatim(code) = inline.kind() {
//!             assert_eq!(code.text(), "*literal code*");
//!         }
//!     }
//! }
//! # Ok::<(), acdc_parser::Error>(())
//! ```
//!
//! Lists expose `items()`. An item's `principal()` contains inline content;
//! `blocks()` contains attached blocks, including nested lists:
//!
//! ```
//! use acdc_parser::{Options, parse, ast::BlockKind};
//!
//! let parsed = parse("* Task\n+\nDetails.\n", &Options::default())?;
//! let block = parsed.document_ref().blocks().next().unwrap();
//! if let BlockKind::UnorderedList(list) = block.kind() {
//!     for item in list.items() {
//!         assert_eq!(item.marker(), "*");
//!         assert_eq!(item.checked(), None);
//!         assert!(!item.principal().is_empty());
//!         for attached in item.blocks() {
//!             if let BlockKind::Paragraph(paragraph) = attached.kind() {
//!                 assert!(!paragraph.content().is_empty());
//!             }
//!         }
//!     }
//! }
//! # Ok::<(), acdc_parser::Error>(())
//! ```
//!
//! Description items add `term()`, `delimiter()`, and `anchors()`; each parsed
//! term remains a separate entry. Callout items expose their marker through
//! `callout()`. List styles and `start` attributes remain declared controls;
//! these reads do not calculate rendered ordinals.
//!
//! Handles and direct iterators support shared immutable access (`Send + Sync`).
//! Reads do not allocate collections, render content, or update document catalogs.
//! Iterator order follows the owner's content order; concrete storage, iterator
//! types, and attribute serialization are not part of this interface.

macro_rules! node_ref {
    ($name:ident<$lifetime:lifetime>, $owner:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug)]
        pub struct $name<$lifetime> {
            node: &$lifetime $owner,
        }

        impl<$lifetime> $name<$lifetime> {
            pub(super) fn new(node: &$lifetime $owner) -> Self {
                Self { node }
            }

            /// The source span of this node.
            #[must_use]
            pub fn location(self) -> $crate::ast::LocationRef<$lifetime> {
                $crate::ast::LocationRef::new(&self.node.location)
            }
        }
    };
}

mod attributes;
mod blocks;
mod content;
mod inlines;
mod location;
mod media;
mod tables;
mod values;

pub use attributes::AttributesRef;
pub use blocks::*;
pub use content::ContentRef;
pub use inlines::*;
pub use location::{LocationRef, PositionRef};
pub use media::{ImageRef, InlineImageRef, SourceKind, SourceRef};
pub use tables::{ColumnFormatRef, TableCellRef, TableRef, TableRowRef};
pub use values::{AttributeValueRef, ElementIdRef, TokensRef};
