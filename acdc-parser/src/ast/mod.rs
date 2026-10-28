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
//! plain text, bold content, and locations. The existing owner API remains
//! available for other reads and for construction and editing.
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

mod blocks;
mod content;
mod inlines;
mod location;

pub use blocks::*;
pub use content::ContentRef;
pub use inlines::*;
pub use location::{LocationRef, PositionRef};
