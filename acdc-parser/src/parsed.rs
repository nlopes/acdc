//! Self-referential wrappers returned by the public `parse_*` entry points.
//!
//! `ParsedDocument` pins three things together — the preprocessed source,
//! the `bumpalo::Bump` arena that backs parser-allocated strings, and the
//! `Document<'_>` AST that borrows from both — behind an opaque accessor
//! API. Consumers reach the AST via `.document()`; bumpalo never appears in
//! a public signature.
//!
//! `ParsedInline` plays the same role for the stand-alone inline parsing
//! entry point used by TCK tests.
//!
//! `OwnedSource` covers the parse-failure case (we have the text but no
//! AST) without paying for an empty arena.

use bumpalo::Bump;

use crate::{Document, InlineNode};

/// Owner-side of the self-referential parse cell: holds the preprocessed
/// source text and the arena that parser-allocated strings live in. The
/// AST dependent borrows from both fields simultaneously via their shared
/// owner lifetime.
#[derive(Debug)]
pub(crate) struct OwnedInput {
    pub(crate) source: Box<str>,
    pub(crate) arena: Bump,
}

impl OwnedInput {
    pub(crate) fn new(source: Box<str>) -> Self {
        // Seed the arena with one chunk sized to the input. Most arena
        // memory ends up holding interned strings + AST nodes whose total
        // footprint correlates with source length, so this avoids the
        // first ~10 chunk-grow round-trips through the global allocator
        // on documents larger than a few KB.
        let arena = Bump::with_capacity(source.len());
        Self { source, arena }
    }
}

// `self_cell!`'s `dependent:` slot takes a bare identifier and expands it
// internally as `$Dependent<'a>`. `Document` already fits, so it goes in
// directly. `Vec<InlineNode<'a>>` doesn't — hence the `InlineAst` alias.
type InlineAst<'a> = Vec<InlineNode<'a>>;

self_cell::self_cell! {
    struct ParsedDocumentCell {
        owner: OwnedInput,
        #[covariant]
        dependent: Document,
    }

    impl {Debug}
}

self_cell::self_cell! {
    struct ParsedInlineCell {
        owner: OwnedInput,
        #[covariant]
        dependent: InlineAst,
    }

    impl {Debug}
}

/// A parsed document plus the buffers it borrows from, bound together so
/// callers can hold the AST past the caller's input slice without any
/// `into_static()` copy. Drop releases the arena and source in one shot.
#[derive(Debug)]
pub struct ParsedDocument(ParsedDocumentCell);

impl ParsedDocument {
    /// Borrow the AST.
    #[must_use]
    pub fn document(&self) -> &Document<'_> {
        self.0.borrow_dependent()
    }

    /// Borrow the preprocessed source text the AST was parsed from.
    ///
    /// Note: this is the text as seen by the grammar, after include
    /// resolution and other preprocessor transforms — not the original
    /// caller input.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.0.borrow_owner().source
    }

    /// Internal constructor. Takes owner + a fallible dependent builder.
    pub(crate) fn try_new<E>(
        owner: OwnedInput,
        builder: impl for<'a> FnOnce(&'a OwnedInput) -> Result<Document<'a>, E>,
    ) -> Result<Self, E> {
        ParsedDocumentCell::try_new(owner, builder).map(Self)
    }
}

/// A parsed sequence of inline nodes plus the buffers they borrow from.
/// Counterpart to `ParsedDocument` for the inline-only entry point.
#[derive(Debug)]
pub struct ParsedInline(ParsedInlineCell);

impl ParsedInline {
    /// Borrow the inline-node slice.
    #[must_use]
    pub fn inlines(&self) -> &[InlineNode<'_>] {
        self.0.borrow_dependent()
    }

    /// Borrow the preprocessed source text the nodes were parsed from.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.0.borrow_owner().source
    }

    /// Internal constructor. Takes owner + a fallible dependent builder.
    pub(crate) fn try_new<E>(
        owner: OwnedInput,
        builder: impl for<'a> FnOnce(&'a OwnedInput) -> Result<Vec<InlineNode<'a>>, E>,
    ) -> Result<Self, E> {
        ParsedInlineCell::try_new(owner, builder).map(Self)
    }
}

/// Source text with no AST — returned by consumers (e.g. the LSP) when the
/// input is available but parsing failed.
#[derive(Debug, Clone)]
pub struct OwnedSource(Box<str>);

impl OwnedSource {
    /// Wrap owned source text.
    #[must_use]
    pub fn new(source: impl Into<Box<str>>) -> Self {
        Self(source.into())
    }

    /// Borrow the source text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.0
    }
}
