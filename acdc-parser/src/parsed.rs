//! Self-referential wrappers returned by the public `parse_*` entry points.
//!
//! `ParseResult` pins the preprocessed source, the `bumpalo::Bump` arena
//! that backs parser-allocated strings, the `Document<'_>` AST that borrows
//! from both, and any warnings the parser produced — all bound together so
//! callers can hold the AST past the caller's input slice without any
//! `into_static()` copy. Drop releases the arena, source, and warnings in
//! one shot. `ParseInlineResult` plays the same role for the inline-only
//! entry point used by TCK tests and the HTML converter's quotes-only
//! fallback.
//!
//! Consumers reach the AST via `.document()` (or `.inlines()`); bumpalo
//! never appears in a public signature.
//!
//! `OwnedSource` covers the parse-failure case (we have the text but no
//! AST) without paying for an empty arena.

use std::{cell::RefCell, rc::Rc};

use bumpalo::Bump;

use crate::{Document, InlineNode, Warning};

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

/// Successful document parse output: the AST plus the buffers it borrows
/// from, plus any non-fatal warnings the parser collected.
///
/// Modelled on chumsky's `ParseResult`: the presence of the output and the
/// presence of warnings are orthogonal, so both are returned side-by-side
/// rather than encoded into `Result`. `#[must_use]` so warnings don't get
/// silently dropped.
#[derive(Debug)]
#[must_use = "ignoring a ParseResult drops any warnings the parser produced"]
pub struct ParseResult {
    cell: ParsedDocumentCell,
    warnings: Vec<Warning>,
}

impl ParseResult {
    /// Internal constructor used by the `parse_*` entry points. Takes the
    /// owner, a shared warnings handle (the `ParserState` holds its own
    /// clone of this `Rc`; we recover the collected warnings after the
    /// builder returns), and a fallible dependent builder.
    pub(crate) fn try_new<E>(
        owner: OwnedInput,
        warnings_handle: Rc<RefCell<Vec<Warning>>>,
        builder: impl for<'a> FnOnce(&'a OwnedInput) -> Result<Document<'a>, E>,
    ) -> Result<Self, E> {
        let cell = ParsedDocumentCell::try_new(owner, builder)?;
        Ok(Self {
            cell,
            warnings: recover_warnings(warnings_handle),
        })
    }

    /// Borrow the document AST.
    #[must_use]
    pub fn document(&self) -> &Document<'_> {
        self.cell.borrow_dependent()
    }

    /// Borrow the preprocessed source the AST was parsed from.
    ///
    /// Note: this is the text as seen by the grammar, after include
    /// resolution and other preprocessor transforms — not the original
    /// caller input.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.cell.borrow_owner().source
    }

    /// Borrow the collected warnings.
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Take the warnings out of this result, leaving an empty warnings
    /// slice behind. Useful when the caller wants to route warnings
    /// independently of the AST (e.g. attach them to an LSP diagnostic
    /// stream while keeping the document for further borrowing). The
    /// `ParseResult` keeps its AST intact — only `warnings()` becomes
    /// empty.
    pub fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }
}

/// Successful inline-only parse output: the inline-node slice plus the
/// buffers it borrows from, plus any non-fatal warnings. Counterpart to
/// [`ParseResult`] for the inline-only entry point.
#[derive(Debug)]
#[must_use = "ignoring a ParseInlineResult drops any warnings the parser produced"]
pub struct ParseInlineResult {
    cell: ParsedInlineCell,
    warnings: Vec<Warning>,
}

impl ParseInlineResult {
    /// Internal constructor. Counterpart to [`ParseResult::try_new`].
    pub(crate) fn try_new<E>(
        owner: OwnedInput,
        warnings_handle: Rc<RefCell<Vec<Warning>>>,
        builder: impl for<'a> FnOnce(&'a OwnedInput) -> Result<Vec<InlineNode<'a>>, E>,
    ) -> Result<Self, E> {
        let cell = ParsedInlineCell::try_new(owner, builder)?;
        Ok(Self {
            cell,
            warnings: recover_warnings(warnings_handle),
        })
    }

    /// Infallible variant for callers that don't need warnings (e.g. the
    /// HTML converter's quotes-only fallback via `parse_text_for_quotes`).
    /// `warnings` is always empty.
    pub(crate) fn from_infallible(
        owner: OwnedInput,
        builder: impl for<'a> FnOnce(&'a OwnedInput) -> Vec<InlineNode<'a>>,
    ) -> Self {
        let cell = ParsedInlineCell::new(owner, builder);
        Self {
            cell,
            warnings: Vec::new(),
        }
    }

    /// Borrow the inline-node slice.
    #[must_use]
    pub fn inlines(&self) -> &[InlineNode<'_>] {
        self.cell.borrow_dependent()
    }

    /// Borrow the preprocessed source the nodes were parsed from.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.cell.borrow_owner().source
    }

    /// Borrow the collected warnings.
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Take the warnings out of this result, leaving an empty slice
    /// behind. See [`ParseResult::take_warnings`].
    pub fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }
}

/// Unwrap the `Rc` the `ParserState` shared with the outer scope. The
/// state is dropped before `try_new`'s builder returns, so the outer
/// clone is normally unique and `try_unwrap` succeeds. If any other clone
/// lingers (e.g. a future code path keeps one alive), we fall back to
/// draining through the `RefCell` rather than losing the warnings.
fn recover_warnings(handle: Rc<RefCell<Vec<Warning>>>) -> Vec<Warning> {
    Rc::try_unwrap(handle).map_or_else(
        |shared| std::mem::take(&mut *shared.borrow_mut()),
        RefCell::into_inner,
    )
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
