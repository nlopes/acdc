//! Self-referential wrappers returned by the public `parse_*` entry points.
//!
//! `ParseResult` pins the preprocessed source, the `bumpalo::Bump` arena
//! that backs parser-allocated strings, the `Document<'_>` AST that borrows
//! from both, and any warnings the parser produced — all bound together so
//! callers can hold the AST past the caller's input slice without any
//! `into_static()` copy. Drop releases the arena, source, and warnings in
//! one shot. `ParseInlineResult` plays the same role for standalone inline content.
//!
//! Consumers reach the AST via `.document()` or `.content()`.
//! [`DocumentArena`] stores generated text for in-place document edits.
//!
//! `OwnedSource` covers the parse-failure case (we have the text but no
//! AST) without paying for an empty arena.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use bumpalo::Bump;

use crate::{
    Document, InlineNode, Location, SourceLocation, Warning, WarningKind, model::SourceRange,
    warning::WarningCollection,
};

#[derive(Debug, Default)]
pub(crate) struct SourceFiles {
    primary: Option<PathBuf>,
    included: HashMap<Vec<String>, PathBuf>,
}

impl SourceFiles {
    pub(crate) fn new(primary: Option<PathBuf>, ranges: &[SourceRange]) -> Self {
        let mut included = HashMap::new();
        for range in ranges {
            if !range.file_chain.is_empty()
                && let Some(file) = &range.file
                && !included.contains_key(range.file_chain.as_slice())
            {
                included.insert(range.file_chain.clone(), file.clone());
            }
        }
        Self { primary, included }
    }

    fn location(&self, location: &Location) -> SourceLocation {
        let file = match location
            .start
            .file
            .as_deref()
            .filter(|chain| !chain.is_empty())
        {
            Some(chain) => self.included.get(chain.as_slice()),
            None => self.primary.as_ref(),
        };
        SourceLocation::at_location(file.cloned(), location.clone())
    }
}

/// Storage for generated text added to a parsed document.
///
/// Obtain this through [`ParseResult::with_document_mut`]. Allocations remain
/// valid until the owning parse result is dropped and cannot be reclaimed
/// individually.
#[derive(Debug)]
pub struct DocumentArena(Bump);

impl DocumentArena {
    /// Copy `text` into the arena.
    ///
    /// The returned slice lives as long as the parsed document, so AST nodes
    /// may hold it. Copies are never reclaimed individually: the whole arena
    /// is released when the `ParseResult` drops, so this is for text that
    /// becomes part of the document, not for scratch buffers.
    #[must_use]
    pub fn alloc_str<'a>(&'a self, text: &str) -> &'a str {
        self.0.alloc_str(text)
    }

    /// The underlying arena, for the parser's own interning paths.
    pub(crate) fn bump(&self) -> &Bump {
        &self.0
    }
}

/// Owner-side of the self-referential parse cell: holds the preprocessed
/// source text and the arena that parser-allocated strings live in. The
/// AST dependent borrows from both fields simultaneously via their shared
/// owner lifetime.
#[derive(Debug)]
pub(crate) struct OwnedInput {
    pub(crate) source: Box<str>,
    pub(crate) arena: DocumentArena,
}

impl OwnedInput {
    pub(crate) fn new(source: Box<str>) -> Self {
        // Seed the arena with one chunk sized to the input. Most arena
        // memory ends up holding interned strings + AST nodes whose total
        // footprint correlates with source length, so this avoids the
        // first ~10 chunk-grow round-trips through the global allocator
        // on documents larger than a few KB.
        let arena = DocumentArena(Bump::with_capacity(source.len()));
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

/// A conditional decision made while reading the source, before block parsing.
///
/// The result uses attributes at the directive and all enclosing conditions.
/// It describes this parse and does not change when a document is copied or edited.
#[derive(Clone, Debug)]
pub struct ConditionalEvaluation {
    pub(crate) file: Option<Arc<PathBuf>>,
    pub(crate) start_line: usize,
    pub(crate) active: bool,
    pub(crate) inline: bool,
    pub(crate) end_line: Option<usize>,
}

impl ConditionalEvaluation {
    /// Original source file, absent for primary string or reader input.
    #[must_use]
    pub fn source_file(&self) -> Option<&Path> {
        self.file.as_deref().map(PathBuf::as_path)
    }

    /// One-based opening line in the original source.
    #[must_use]
    pub fn start_line(&self) -> usize {
        self.start_line
    }

    /// Whether the condition and all enclosing conditions selected its content.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Whether the directive contains its content on the opening line.
    #[must_use]
    pub fn is_inline(&self) -> bool {
        self.inline
    }

    /// One-based closing line in the same file, absent for inline or unclosed directives.
    #[must_use]
    pub fn end_line(&self) -> Option<usize> {
        self.end_line
    }
}

/// Owns a parsed document, its backing text, and non-fatal warnings.
///
/// A successful parse can contain recovered content. Check [`Self::source_recovery`]
/// when the application requires complete input, and [`Self::warnings`] for all diagnostics.
#[derive(Debug)]
#[must_use = "ignoring a ParseResult drops any warnings the parser produced"]
pub struct ParseResult {
    cell: ParsedDocumentCell,
    warnings: Vec<Warning>,
    source_recovery: Option<Box<Warning>>,
    source_files: SourceFiles,
    conditionals: Vec<ConditionalEvaluation>,
}

impl ParseResult {
    /// Build the document with its source map and collect warnings after construction.
    pub(crate) fn try_new<E>(
        owner: OwnedInput,
        warnings_handle: Rc<RefCell<WarningCollection>>,
        source_files: SourceFiles,
        conditionals: Vec<ConditionalEvaluation>,
        builder: impl for<'a> FnOnce(&'a OwnedInput) -> Result<Document<'a>, E>,
    ) -> Result<Self, E> {
        let cell = ParsedDocumentCell::try_new(owner, builder)?;
        let warnings = recover_warnings(warnings_handle);
        let source_recovery = warnings
            .iter()
            .find(|warning| {
                matches!(
                    warning.kind,
                    WarningKind::ContentRecovery { .. }
                        | WarningKind::UnsupportedCounter { .. }
                        | WarningKind::UnterminatedDelimitedBlock { .. }
                        | WarningKind::UnterminatedTable { .. }
                        | WarningKind::TableUnknownFormat { .. }
                        | WarningKind::TableIncompleteRow
                        | WarningKind::TableCellOverflow { .. }
                        | WarningKind::TableColumnCount { .. }
                )
            })
            .map(|warning| Box::new(Warning::new(warning.kind.clone(), warning.location.clone())));
        Ok(Self {
            cell,
            warnings,
            source_recovery,
            source_files,
            conditionals,
        })
    }

    /// Read the document through typed handles and iterators.
    ///
    /// The document cannot outlive this parse result:
    ///
    /// ```compile_fail,E0597
    /// use acdc_parser::{Options, parse};
    ///
    /// let document = {
    ///     let parsed = parse("A paragraph.", &Options::default()).unwrap();
    ///     parsed.document()
    /// };
    /// println!("{}", document.blocks().count());
    /// ```
    #[must_use]
    pub fn document(&self) -> crate::ast::DocumentRef<'_> {
        crate::ast::DocumentRef::from(self.cell.borrow_dependent())
    }

    #[cfg(test)]
    pub(crate) fn document_storage(&self) -> &Document<'_> {
        self.cell.borrow_dependent()
    }

    /// Rewrite the document AST in place.
    ///
    /// Generated text can borrow from the supplied [`DocumentArena`]. The
    /// closure prevents nodes from borrowing shorter-lived caller buffers.
    ///
    /// Callers must preserve source locations and keep references, the table of
    /// contents, footnotes, and numbering consistent with their edits. This
    /// method does not rebuild derived document data or update [`Self::preprocessed_source`]
    /// and [`Self::warnings`]. Caption and section edits may require
    /// [`Document::renumber_captions`] and [`Document::renumber_sections`].
    pub fn with_document_mut<'outer, R>(
        &'outer mut self,
        f: impl for<'a> FnOnce(&'outer mut Document<'a>, &'a DocumentArena) -> R,
    ) -> R {
        self.cell
            .with_dependent_mut(|owner, document| f(document, &owner.arena))
    }

    /// Borrow the preprocessed source the AST was parsed from.
    ///
    /// Note: this is the text as seen by the grammar, after include
    /// resolution and other preprocessor transforms — not the original
    /// caller input.
    #[must_use]
    pub fn preprocessed_source(&self) -> &str {
        &self.cell.borrow_owner().source
    }

    /// Resolve a location from this result's AST to its original source file and position.
    ///
    /// Includes use the file resolved during preprocessing, even when selection
    /// or indentation changed the parsed text. A span crossing files is anchored
    /// in the file containing its start. Unknown include chains have no file.
    #[must_use]
    pub fn source_location(&self, location: &Location) -> SourceLocation {
        self.source_files.location(location)
    }

    /// Resolve a location from this parse to its original file and position.
    ///
    /// This has the same source-mapping rules as [`Self::source_location`] and
    /// returns an owned location, which may copy its file path.
    #[must_use]
    pub fn resolve_location(&self, location: crate::ast::LocationRef<'_>) -> SourceLocation {
        self.source_location(location.as_location())
    }

    /// Conditional decisions in preprocessing order, including selected include sources.
    ///
    /// Each decision retains original file and line numbers, including directives
    /// whose content was omitted. These records describe the parsed source and
    /// remain separate from authored AST content.
    pub fn conditionals(&self) -> impl ExactSizeIterator<Item = &ConditionalEvaluation> {
        self.conditionals.iter()
    }

    /// Borrow the collected warnings.
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// The first warning for recovered source content or structure, when present.
    ///
    /// This includes omitted content, ignored substitutions, and unmatched block
    /// boundaries, but excludes presentation warnings. It remains available after
    /// [`Self::take_warnings`].
    #[must_use]
    pub fn source_recovery(&self) -> Option<&Warning> {
        self.source_recovery.as_deref()
    }

    /// Take the warnings, leaving [`Self::warnings`] empty.
    ///
    /// The document and [`Self::source_recovery`] remain available.
    pub fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }
}

/// Inline content and its source text, with any nonfatal warnings.
///
/// Read content through [`Self::content`]. The backing slice is private:
///
/// ```compile_fail,E0624
/// use acdc_parser::{Options, parse_inline};
/// let parsed = parse_inline("Text", &Options::default()).unwrap();
/// let _ = parsed.inlines();
/// ```
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
        warnings_handle: Rc<RefCell<WarningCollection>>,
        builder: impl for<'a> FnOnce(&'a OwnedInput) -> Result<Vec<InlineNode<'a>>, E>,
    ) -> Result<Self, E> {
        let cell = ParsedInlineCell::try_new(owner, builder)?;
        Ok(Self {
            cell,
            warnings: recover_warnings(warnings_handle),
        })
    }

    /// Build inline content without fallible parsing or diagnostics.
    /// Used by [`crate::parse_inline_formatting`].
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
    pub(crate) fn inlines(&self) -> &[InlineNode<'_>] {
        self.cell.borrow_dependent()
    }

    /// Borrow the preprocessed source the nodes were parsed from.
    #[must_use]
    pub fn preprocessed_source(&self) -> &str {
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
fn recover_warnings(handle: Rc<RefCell<WarningCollection>>) -> Vec<Warning> {
    Rc::try_unwrap(handle)
        .map_or_else(
            |shared| std::mem::take(&mut *shared.borrow_mut()),
            RefCell::into_inner,
        )
        .into_vec()
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
