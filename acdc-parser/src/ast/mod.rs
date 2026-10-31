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
//! currently cover headers and authors, document attributes, document/section
//! children and titles, block bodies and metadata, lists and their items, tables,
//! media, discrete headings, TOC blocks, breaks, and comments. Inline reads cover
//! plain/raw/verbatim text, formatting spans, images, icons, hyperlinks,
//! cross-references, index labels and relationships, footnotes, anchors, callouts,
//! UI labels, inline formulas, and locations.
//! The owner API remains available for other reads and for construction and editing.
//!
//! Formatting spans keep distinct types, with `content()`, `form()`, `id()`,
//! `roles()`, and `attributes()`. Span IDs retain the source range that supplied
//! the ID, including the authored reference when an attribute was expanded.
//!
//! ```
//! use acdc_parser::{Form, Options, parse, ast::{BlockKind, BoldRef, InlineKind}};
//!
//! fn has_warning_role(span: BoldRef<'_>) -> bool {
//!     span.form() == Form::Constrained && span.roles().contains("warning")
//! }
//!
//! let parsed = parse("A [.warning]*bold* word.", &Options::default())?;
//! let block = parsed.document_ref().blocks().next().unwrap();
//! if let BlockKind::Paragraph(paragraph) = block.kind() {
//!     for inline in paragraph.content().inlines() {
//!         if let InlineKind::Bold(span) = inline.kind() {
//!             assert!(has_warning_role(span));
//!             assert!(!span.content().is_empty());
//!         }
//!     }
//! }
//! # Ok::<(), acdc_parser::Error>(())
//! ```
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
//! Document `attributes()` is the end-of-header snapshot. Later accepted changes
//! remain in the block tree; an unset is distinct from an absent assignment:
//!
//! ```
//! use acdc_parser::{DocumentAttributeValue, Options, parse, ast::BlockKind};
//!
//! let parsed = parse(":audience: everyone\n\nBefore.\n\n:audience!:\n\nAfter.\n", &Options::default())?;
//! let document = parsed.document_ref();
//! let mut audience = document.attributes().get("audience");
//! for block in document.blocks() {
//!     if let BlockKind::DocumentAttribute(event) = block.kind() {
//!         if event.name() == "audience" {
//!             audience = event.assignment().value();
//!         }
//!     }
//! }
//! assert!(audience.is_none());
//! assert_eq!(document.attributes().get("audience").and_then(DocumentAttributeValue::text), Some("everyone"));
//! # Ok::<(), acdc_parser::Error>(())
//! ```
//!
//! `header()` reads title, subtitle and author data. Revision values use the
//! document attributes `revnumber`, `revdate`, and `revremark`. A table cell's
//! `initial_attributes()` initializes its local scope before its child blocks.
//!
//! Media sources retain their full operand. Provider controls are separate values:
//!
//! ```
//! use acdc_parser::{Options, parse, ast::{AttributeValueRef, BlockKind, SourceKind}};
//!
//! let parsed = parse("video::one,,three[youtube]", &Options::default())?;
//! let block = parsed.document_ref().blocks().next().unwrap();
//! if let BlockKind::Video(video) = block.kind() {
//!     assert_eq!(video.source().kind(), SourceKind::Name("one,,three"));
//!     assert_eq!(video.attributes().get("poster"), Some(AttributeValueRef::Text("youtube")));
//! }
//! # Ok::<(), acdc_parser::Error>(())
//! ```
//!
//! Icon identity is retained independently of converter emission:
//!
//! ```
//! use acdc_parser::{Options, parse, ast::{BlockKind, InlineKind}};
//!
//! let parsed = parse("icon:star[2x,id=favorite,role=gold]", &Options::default())?;
//! let block = parsed.document_ref().blocks().next().unwrap();
//! if let BlockKind::Paragraph(paragraph) = block.kind() {
//!     for inline in paragraph.content().inlines() {
//!         if let InlineKind::Icon(icon) = inline.kind() {
//!             assert_eq!(icon.size(), Some("2x"));
//!             assert_eq!(icon.id().map(|id| id.as_str()), Some("favorite"));
//!             assert!(icon.roles().contains("gold"));
//!         }
//!     }
//! }
//! # Ok::<(), acdc_parser::Error>(())
//! ```
//!
//! Retained line/paragraph comments expose `kind()` and literal `text()`;
//! delimited comments expose `content()`. Includes and conditional directives
//! have already been processed and are not reconstructed by these reads.
//!
//! Handles and direct iterators support shared immutable access (`Send + Sync`).
//! Direct reads do not allocate collections, render content, or update document catalogs.
//! Descendant traversal can allocate a stack of pending siblings as nesting increases.
//! Iterator order follows the owner's content order; concrete storage, iterator
//! types, and attribute serialization are not part of this interface.

macro_rules! node_ref {
    ($name:ident<$lifetime:lifetime>, $owner:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug)]
        pub struct $name<$lifetime> {
            pub(in crate::ast) node: &$lifetime $owner,
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
mod catalogs;
mod content;
mod content_fields;
mod content_occurrences;
mod identity;
mod inlines;
mod location;
mod media;
mod substitutions;
mod tables;
mod traversal;
mod values;

pub use attributes::AttributesRef;
pub use blocks::*;
pub use catalogs::{ReferenceRef, ReferencesRef, TocEntryRef};
pub use content::{ContentListRef, ContentRef};
pub use content_fields::{ContentFieldRef, ContentOwnerRef, ContentRole};
pub use content_occurrences::ContentOccurrences;
pub use inlines::*;
pub use location::{LocationRef, PositionRef};
pub use media::{ImageRef, InlineImageRef, SourceKind, SourceRef};
pub use substitutions::{SubstitutionEntryRef, SubstitutionSpecKind, SubstitutionSpecRef};
pub use tables::{ColumnFormatRef, TableCellRef, TableRef, TableRowRef};
pub use traversal::StructuralRef;
pub use values::{AttributeValueRef, ElementIdRef, IdOrigin, TokensRef};
