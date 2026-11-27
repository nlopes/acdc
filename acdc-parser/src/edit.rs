//! The owned document tree, for building and editing documents.
//!
//! [`crate::ParseResult::document`] returns read views from [`crate::ast`]. Copy
//! a document or block into this tree with `Document::from(document_ref)` or
//! `Block::from(block_ref)` when you need to change it, and read it again with
//! `DocumentRef::from(&document)`.
//!
//! [`Block`] and [`InlineNode`] have the same variants as
//! [`crate::ast::BlockKind`] and [`crate::ast::InlineKind`], so code that reads
//! and code that edits match on the same names. [`InlineNode::Extension`] is the
//! one addition: retained extension spellings read as plain text.
//!
//! Copies can borrow text from the parse result, so they cannot outlive it.

pub use crate::{
    error::{CaptionRepairError, ReferenceRepairError},
    grammar::footnotes::FootnoteRebuildError,
    model::{
        Admonition, Anchor, Attribution, Audio, Author, Autolink, Block, BlockMetadata, Bold,
        Button, CalloutList, CalloutListItem, CalloutMarker, Caption, CharacterReference,
        CiteTitle, ColumnFormat, Comment, ContentAttribute, CrossReference, DelimitedComment,
        DescriptionList, DescriptionListDefinition, DescriptionListItem, DescriptionListTerm,
        DiscreteHeading, Document, DocumentAttribute, DoubleQuotedSpan, ElementId, Example,
        Footnote, FootnoteDefinition, Header, Highlight, Icon, Image, IndexTerm, IndexTermKind,
        IndexTermRelationship, InlineImage, InlineMetadata, InlineNode, InlinePassthrough, Italic,
        KeyboardShortcut, LineBreak, Link, ListItem, Listing, Literal, Mailto, MenuPath, Monospace,
        Open, OrderedList, PageBreak, Paragraph, Passthrough, Plain, Quote, Raw, ReferenceTarget,
        ReservedAttribute, Role, Section, Sidebar, SingleQuotedSpan, Source, SourceUrl,
        StandaloneCurvedApostrophe, Stem, StemBlock, Subscript, Subtitle, Superscript, Symbol,
        Table, TableCell, TableOfContents, TableRow, ThematicBreak, Title, TocEntry,
        TokenAssignment, UnorderedList, UrlMacro, Verbatim, Verse, Video,
    },
};
