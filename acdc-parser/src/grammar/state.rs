use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    ops::Range,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

use bitflags::bitflags;
use bumpalo::Bump;

use crate::{
    AttributeName, AttributeValue, CalloutRef, CaptionKind, DocumentAttributes, Footnote, Location,
    Options, SourceLocation, TocEntry, Warning, WarningKind, XrefCaptionLabel,
    grammar::LineMap,
    model::{
        LeveloffsetRange, SourceRange, substitute,
        substitution::{HEADER, SubstitutionPlan},
    },
};

#[derive(Clone, Copy, Debug)]
struct XrefCaptionLabelSnapshot<'a> {
    example: XrefCaptionLabel<'a>,
    listing: XrefCaptionLabel<'a>,
    table: XrefCaptionLabel<'a>,
}

#[derive(Debug)]
pub(crate) struct ParserState<'a> {
    pub(crate) document_attributes: Rc<DocumentAttributes<'a>>,
    /// Attributes inherited by the current nested document. Asciidoctor-style
    /// table cells may read these values, but attribute entries in the cell
    /// cannot replace or unset them.
    pub(crate) nested_parent_attributes: Option<Rc<DocumentAttributes<'a>>>,
    /// Tracks document hard-break changes that apply only to following blocks.
    pub(crate) hardbreaks: bool,
    /// Processed-text offsets where an empty attribute reference was removed.
    pub(crate) empty_attribute_offsets: Vec<usize>,
    /// Attribute-produced spans used to preserve substitution order in nested parsing.
    pub(crate) attribute_value_ranges: Vec<Range<usize>>,
    pub(crate) line_map: Rc<LineMap>,
    /// Parse options, shared via `Rc` so the per-inline-parse
    /// `for_inline_parsing` sub-state is cheap to construct — the old
    /// `Options` deep-clone (including its ~80-entry `DocumentAttributes`
    /// default map) was a meaningful hot spot.
    pub(crate) options: Rc<Options<'a>>,
    /// The input being parsed. Borrowed from the caller's buffer so that
    /// model nodes can hold `Cow::Borrowed` references into it without
    /// allocating per-token strings.
    pub(crate) input: &'a str,
    /// Bump arena for transient strings that downstream inline parsing must
    /// borrow from with lifetime `'a`. Used for substitution buffers, escape
    /// unwrapping, and other grammar sites that historically built local
    /// `String`s and then tried to hand them to the inline parser. Strings
    /// are pushed via `arena.alloc_str(...)`, yielding `&'a str`.
    pub(crate) arena: &'a Bump,
    /// Shared via `Rc<RefCell>` so the sub-state built by `for_inline_parsing`
    /// doesn't have to deep-clone the tracker on entry and on writeback. The
    /// tracker is push-only and participates in no rollback semantics, so
    /// sharing a single cell across the top-level state and every nested
    /// inline sub-parse is equivalent to the old clone/writeback pattern for
    /// all success paths and for every failure path the caller already fails
    /// through. This matters because macro-heavy fixtures (footnotes +
    /// bold/italic/mono/xref content) trigger hundreds of recursive
    /// `process_inlines` calls, and the prior pattern produced O(N×M) work
    /// where N is footnote count and M is the number of nested parses.
    pub(crate) footnote_tracker: Rc<RefCell<FootnoteTracker<'a>>>,
    xref_caption_label_snapshots: Rc<RefCell<Vec<XrefCaptionLabelSnapshot<'a>>>>,
    /// TOC entries collected during parsing, in document order. A section pushes its
    /// [`TocEntry`] here as soon as its title is parsed; `numbered` is `false` for
    /// special styles (`[bibliography]`, `[glossary]`, ...) that are not auto-numbered
    /// under `sectnums`.
    pub(crate) toc_entries: Vec<TocEntry<'a>>,
    pub(crate) last_block_was_verbatim: bool,
    /// Callout references found in the last verbatim block (for validation with callout
    /// lists)
    pub(crate) last_verbatim_callouts: Vec<CalloutRef>,
    /// The current file being parsed (`None` for inline/string parsing). Used as the
    /// fallback file for diagnostics (`create_error_source_location`) when an offset
    /// isn't covered by a recorded source range.
    ///
    /// Held as `Arc` for the **hot** clone: `for_inline_parsing` inherits it once per
    /// inline sub-parse (and macro-heavy docs spawn hundreds), where the refcount bump
    /// beats a `PathBuf` copy. The diagnostic sites instead deep-copy it into the public
    /// [`SourceLocation::file`](crate::SourceLocation) (`Option<PathBuf>`) API so it's
    /// left as a copy rather than widening the published API to `Option<Arc<PathBuf>>`.
    pub(crate) current_file: Option<Arc<PathBuf>>,
    /// Byte ranges where specific leveloffset values apply. Set by the preprocessor when
    /// processing includes with `leveloffset=` attributes. Used by the parser to adjust
    /// section levels.
    pub(crate) leveloffset_ranges: Vec<LeveloffsetRange>,
    /// Byte ranges mapping preprocessed output back to source files. Set by the
    /// preprocessor when processing `include::` directives. Used to produce accurate
    /// file/line info in warnings.
    pub(crate) source_ranges: Vec<SourceRange>,
    /// Warnings collected during PEG parsing. Shared across the top-level state and any
    /// `for_inline_parsing` sub-states via `Rc`, so warnings raised during nested inline
    /// parses (author lines, revision lines, substituted inline content) reach the
    /// top-level `ParseResult` alongside everything else. The outer `parse_input` /
    /// `parse_inline` keeps a clone of this `Rc` and recovers the final `Vec<Warning>`
    /// after the self-cell builder closure drops the `ParserState`. Deduplicated on
    /// insertion because PEG backtracking can re-fire the same warning multiple times.
    pub(crate) warnings: Rc<RefCell<Vec<Warning>>>,
    /// When true, inline parsing uses a reduced rule set that only matches formatting
    /// markup (bold, italic, monospace, highlight, superscript, subscript, curved quotes)
    /// and plain text. Used by `parse_text_for_quotes` to apply "quotes" substitution
    /// without matching macros, xrefs, etc.
    pub(crate) quotes_only: bool,
    /// When parsing content extracted from a constrained formatting rule, holds
    /// the delimiter byte of the outer formatting (e.g., `b'_'` for italic).
    /// Used to correctly fail boundary checks when the outer delimiter is a
    /// word character (only `_` among the formatting delimiters).
    pub(crate) outer_constrained_delimiter: Option<u8>,
    /// Whether this state parses document content or a nested inline segment.
    pub(crate) scope: ParserScope,
    /// Context set before entering the PEG inline parser. These fields are
    /// constant within a single `inlines()` call, allowing rules to be
    /// argument-free and thus cacheable by the PEG packrat memoizer.
    pub(crate) inline_ctx: InlineContext,
    /// Memoised `@`-lookahead covering `[.., scanned_up_to)` with the first
    /// `@` in that range, if any. Without the `scanned_up_to` field, a cached
    /// "no `@`" result at offset X is indistinguishable from "`@` is at X"
    /// and can trigger the expensive email rule the cache was meant to avoid.
    pub(crate) next_at_sign_cache: Cell<Option<AtLookahead>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AtLookahead {
    pub(crate) scanned_up_to: usize,
    pub(crate) first_at: Option<usize>,
}

/// Inline parsing context — constant within a single `inlines()` call.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct InlineContext {
    /// Byte offset for location calculation in inline rules.
    pub(crate) offset: usize,
    /// Enabled substitutions and their source order.
    pub(crate) substitutions: SubstitutionPlan,
    /// Independent grammar capabilities for this parse.
    pub(crate) rules: InlineRules,
}

bitflags! {
    /// Independent grammar capabilities for one inline parse.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct InlineRules: u8 {
        /// Match bare URLs and email addresses.
        const AUTOLINKS = 1 << 0;
        /// Match index-term macros.
        const INDEX_TERMS = 1 << 1;
        /// Apply the document or block hard-break option.
        const HARD_BREAKS = 1 << 2;
        /// Permit a trailing ` +` to end at end of input.
        const EOI_HARD_BREAK = 1 << 3;
    }
}

impl Default for InlineRules {
    fn default() -> Self {
        Self::AUTOLINKS | Self::INDEX_TERMS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParserScope {
    Document,
    Inline,
}

#[derive(Debug, Clone)]
pub(crate) struct FootnoteTracker<'a> {
    /// All registered footnotes in the order they were encountered.
    pub(crate) footnotes: Vec<Footnote<'a>>,
    /// The last assigned footnote number (starts at 1)
    last_footnote_position: u32,
    /// Map of named footnote IDs to their assigned numbers
    ///
    /// This helps ensure that named footnotes are only assigned a number once and reused.
    /// If it's an anonymous footnote (no ID), it always gets a new number.
    named_footnote_numbers: HashMap<&'a str, u32>,
    /// Footnote numbers whose stored entry has already had its location/content
    /// finalized to document-absolute coordinates (see [`Self::finalize`]). The first
    /// occurrence of a number wins, so a later bare reference of a named footnote does
    /// not overwrite the defining occurrence.
    location_finalized: HashSet<u32>,
}

impl<'a> FootnoteTracker<'a> {
    pub(crate) fn new() -> Self {
        Self {
            footnotes: Vec::new(),
            last_footnote_position: 1,
            named_footnote_numbers: HashMap::new(),
            location_finalized: HashSet::new(),
        }
    }

    /// Replace the stored entry's location and content with `footnote`'s, which the
    /// caller has just mapped to document-absolute coordinates. Footnotes are recorded
    /// during inline parsing in preprocessed-*local* coordinates (offset 0), so the
    /// stored copy is otherwise mislocated; `map_inline_locations` calls this once the
    /// in-tree copy has been mapped. Entries are kept in number order, so `number - 1`
    /// indexes the stored entry. First occurrence wins: a later bare reference of a
    /// named footnote leaves the defining occurrence's content and location intact.
    pub(crate) fn finalize(&mut self, footnote: &Footnote<'a>) {
        if self.location_finalized.insert(footnote.number)
            && let Some(index) = footnote.number.checked_sub(1)
            && let Some(entry) = self.footnotes.get_mut(index as usize)
        {
            entry.location = footnote.location.clone();
            entry.content.clone_from(&footnote.content);
        }
    }

    /// Register a footnote and assign it a number. Named footnotes are
    /// deduplicated: subsequent occurrences with the same id reuse the first
    /// number and are not re-added to the list. Anonymous footnotes always
    /// get a fresh number.
    #[tracing::instrument(skip_all, fields(?footnote))]
    pub(crate) fn push(&mut self, footnote: &mut Footnote<'a>) {
        if let Some(id) = footnote.id
            && let Some(&existing) = self.named_footnote_numbers.get(id)
        {
            footnote.number = existing;
            return;
        }
        footnote.number = self.last_footnote_position;
        if let Some(id) = footnote.id {
            self.named_footnote_numbers
                .insert(id, self.last_footnote_position);
        }
        self.footnotes.push(footnote.clone());
        self.last_footnote_position += 1;
    }
}

impl<'a> ParserState<'a> {
    /// Allocate `s` into the arena, returning a `&'a str` that lives for the
    /// parse. Used by grammar rules that build a transient owned `String`
    /// (attribute substitution, escape unwrapping, …) and need to hand the
    /// result to the inline parser with the outer `'a` lifetime so that the
    /// resulting `InlineNode`s can be returned.
    pub(crate) fn intern_str(&self, s: &str) -> &'a str {
        self.arena.alloc_str(s)
    }

    /// Promote a `Cow<'b, str>` to `&'a str`. `Owned` variants are interned
    /// into the arena; `Borrowed` values pass through unchanged (no copy)
    /// when their lifetime already outlives `'a`, which is the common case
    /// for substitution results that borrowed from the grammar input.
    pub(crate) fn intern_cow<'b>(&self, cow: Cow<'b, str>) -> &'a str
    where
        'b: 'a,
    {
        match cow {
            Cow::Borrowed(s) => s,
            Cow::Owned(s) => self.arena.alloc_str(&s),
        }
    }

    /// Format `args` directly into the arena, returning `&'a str`. Avoids the
    /// heap-`String` + `alloc_str`-copy pair that `intern_str(&format!(...))`
    /// forces: a single write into `bumpalo::collections::String`.
    pub(crate) fn intern_fmt(&self, args: std::fmt::Arguments<'_>) -> &'a str {
        use std::fmt::Write as _;
        let mut s = bumpalo::collections::String::new_in(self.arena);
        // `BumpString::write_fmt` is infallible; the `Err` arm is unreachable.
        let _ = s.write_fmt(args);
        s.into_bump_str()
    }

    /// Concatenate `parts` into the arena separated by `sep`, returning
    /// `&'a str`. Avoids the heap-`String` `.join(...)` followed by an
    /// `alloc_str` copy.
    pub(crate) fn intern_join<I, S>(&self, parts: I, sep: &str) -> &'a str
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut s = bumpalo::collections::String::new_in(self.arena);
        let mut first = true;
        for p in parts {
            if !first {
                s.push_str(sep);
            }
            first = false;
            s.push_str(p.as_ref());
        }
        s.into_bump_str()
    }

    pub(crate) fn capture_xref_caption_labels(&self) -> NonZeroUsize {
        let label = |name| match self.document_attributes.get(name) {
            Some(AttributeValue::String(_)) => {
                let value = self
                    .document_attributes
                    .get_string(name)
                    .map(|value| self.intern_cow(value))
                    .unwrap_or_default();
                if value.is_empty() {
                    XrefCaptionLabel::NumberOnly
                } else {
                    XrefCaptionLabel::AtReference(value)
                }
            }
            Some(AttributeValue::Bool(true)) => XrefCaptionLabel::NumberOnly,
            Some(AttributeValue::Bool(false) | AttributeValue::None) | None => {
                XrefCaptionLabel::AtTarget
            }
        };
        let mut snapshots = self.xref_caption_label_snapshots.borrow_mut();
        let snapshot = NonZeroUsize::MIN.saturating_add(snapshots.len());
        snapshots.push(XrefCaptionLabelSnapshot {
            example: label("example-caption"),
            listing: label("listing-caption"),
            table: label("table-caption"),
        });
        snapshot
    }

    pub(crate) fn xref_caption_label(
        &self,
        snapshot: NonZeroUsize,
        kind: CaptionKind,
    ) -> XrefCaptionLabel<'a> {
        let labels = self
            .xref_caption_label_snapshots
            .borrow()
            .get(snapshot.get() - 1)
            .copied();
        labels.map_or(XrefCaptionLabel::AtTarget, |labels| match kind {
            CaptionKind::Example => labels.example,
            CaptionKind::Listing => labels.listing,
            CaptionKind::Table => labels.table,
            CaptionKind::Figure => XrefCaptionLabel::AtTarget,
        })
    }

    /// Apply an attribute entry declared by document content: expand `{attr}`
    /// references at definition time (matching `asciidoctor`), intern the result
    /// for the parse, and store it unless the built-in policy, parser options, or
    /// a nested parent lock the name against document changes.
    ///
    /// Returns the substituted value so a caller that also builds an AST node for
    /// the entry keeps the value the document wrote.
    pub(crate) fn apply_document_attribute(
        &mut self,
        key: AttributeName<'a>,
        value: AttributeValue<'a>,
        set: bool,
        in_header: bool,
    ) -> AttributeValue<'a> {
        let value = self.resolve_document_attribute_value(value, &self.document_attributes);
        if self
            .options
            .is_document_attribute_locked(key.as_ref(), in_header)
            || self
                .nested_parent_attributes
                .as_ref()
                .is_some_and(|attributes| attributes.contains_explicit(key.as_ref()))
        {
            return value;
        }
        if matches!(key.as_ref(), "hardbreaks" | "hardbreaks-option") {
            self.hardbreaks = set;
        }
        Rc::make_mut(&mut self.document_attributes).set(key, value.clone());
        value
    }

    /// Expand an attribute value's references against the given attributes.
    pub(crate) fn resolve_document_attribute_value(
        &self,
        value: AttributeValue<'a>,
        attributes: &DocumentAttributes<'a>,
    ) -> AttributeValue<'a> {
        match value {
            AttributeValue::String(s) => {
                let substituted = substitute(&s, HEADER, attributes);
                AttributeValue::String(Cow::Borrowed(self.intern_str(&substituted)))
            }
            AttributeValue::Bool(_) | AttributeValue::None => value,
        }
    }

    /// Initialize hard-break state from attributes supplied by the parser API.
    pub(crate) fn initialize_hardbreaks(&mut self) {
        self.hardbreaks = self
            .document_attributes
            .get("hardbreaks-option")
            .or_else(|| self.document_attributes.get("hardbreaks"))
            .is_some_and(|value| {
                !matches!(value, AttributeValue::Bool(false) | AttributeValue::None)
            });
    }

    /// Testing helper: constructs a `ParserState` backed by a leaked `Bump`.
    /// One leak per call, bounded by the number of test cases executed —
    /// acceptable because `cargo test` tears down the process afterwards.
    /// Do **not** use from production code paths (use `new` with an explicit
    /// arena owned by the caller instead).
    #[cfg(test)]
    pub(crate) fn new_for_test(input: &'a str) -> Self {
        let arena: &'a Bump = Box::leak(Box::new(Bump::new()));
        Self::new(input, arena)
    }

    pub(crate) fn new(input: &'a str, arena: &'a Bump) -> Self {
        Self {
            options: Rc::new(Options::default()),
            document_attributes: Rc::new(DocumentAttributes::default()),
            nested_parent_attributes: None,
            hardbreaks: false,
            empty_attribute_offsets: Vec::new(),
            attribute_value_ranges: Vec::new(),
            line_map: Rc::new(LineMap::new(input)),
            input,
            arena,
            footnote_tracker: Rc::new(RefCell::new(FootnoteTracker::new())),
            xref_caption_label_snapshots: Rc::new(RefCell::new(Vec::new())),
            toc_entries: Vec::new(),
            last_block_was_verbatim: false,
            last_verbatim_callouts: Vec::new(),
            current_file: None,
            leveloffset_ranges: Vec::new(),
            source_ranges: Vec::new(),
            warnings: Rc::new(RefCell::new(Vec::new())),
            quotes_only: false,
            outer_constrained_delimiter: None,
            scope: ParserScope::Document,
            inline_ctx: InlineContext::default(),
            next_at_sign_cache: Cell::new(None),
        }
    }

    /// Minimal constructor for quotes-only parsing where document attributes aren't needed.
    pub(crate) fn new_quotes_only(input: &'a str, arena: &'a Bump) -> Self {
        use std::sync::LazyLock;
        // Cache empty options to avoid creating default DocumentAttributes (~80 HashMap
        // entries) on every call. Quotes-only parsing doesn't use document attributes.
        static EMPTY_OPTIONS: LazyLock<Options<'static>> = LazyLock::new(|| Options {
            document_attributes: DocumentAttributes::empty(),
            ..Options::default()
        });
        Self {
            options: Rc::new(EMPTY_OPTIONS.clone()),
            document_attributes: Rc::new(DocumentAttributes::empty()),
            nested_parent_attributes: None,
            hardbreaks: false,
            empty_attribute_offsets: Vec::new(),
            attribute_value_ranges: Vec::new(),
            line_map: Rc::new(LineMap::new(input)),
            input,
            arena,
            footnote_tracker: Rc::new(RefCell::new(FootnoteTracker::new())),
            xref_caption_label_snapshots: Rc::new(RefCell::new(Vec::new())),
            toc_entries: Vec::new(),
            last_block_was_verbatim: false,
            last_verbatim_callouts: Vec::new(),
            current_file: None,
            leveloffset_ranges: Vec::new(),
            source_ranges: Vec::new(),
            warnings: Rc::new(RefCell::new(Vec::new())),
            quotes_only: true,
            outer_constrained_delimiter: None,
            scope: ParserScope::Document,
            inline_ctx: InlineContext::default(),
            next_at_sign_cache: Cell::new(None),
        }
    }

    /// Lightweight constructor for inline parsing that reuses parent state fields
    /// instead of creating expensive defaults that get immediately overwritten.
    pub(crate) fn for_inline_parsing(
        input: &'a str,
        parent: &ParserState<'a>,
        inline_ctx: InlineContext,
    ) -> Self {
        Self {
            options: parent.options.clone(),
            document_attributes: Rc::clone(&parent.document_attributes),
            nested_parent_attributes: parent.nested_parent_attributes.clone(),
            hardbreaks: parent.hardbreaks,
            empty_attribute_offsets: Vec::new(),
            attribute_value_ranges: Vec::new(),
            line_map: Rc::new(LineMap::new(input)),
            input,
            arena: parent.arena,
            footnote_tracker: Rc::clone(&parent.footnote_tracker),
            xref_caption_label_snapshots: Rc::clone(&parent.xref_caption_label_snapshots),
            toc_entries: Vec::new(),
            last_block_was_verbatim: false,
            last_verbatim_callouts: Vec::new(),
            // Inherit the file so inline sub-parse nodes are stamped with the correct
            // origin file by `create_location` (cheap `Arc` clone).
            current_file: parent.current_file.clone(),
            leveloffset_ranges: Vec::new(),
            source_ranges: Vec::new(),
            // Share the parent's warnings vec so anything raised during a
            // nested inline sub-parse reaches the top-level `ParseResult`
            // without a separate drain step.
            warnings: Rc::clone(&parent.warnings),
            quotes_only: parent.quotes_only,
            outer_constrained_delimiter: parent.outer_constrained_delimiter,
            scope: ParserScope::Inline,
            inline_ctx,
            next_at_sign_cache: Cell::new(None),
        }
    }

    /// Warn about unexpected trailing content after a block macro's attribute list.
    pub(crate) fn warn_trailing_macro_content(
        &mut self,
        macro_name: &str,
        trailing: &str,
        end: usize,
        offset: usize,
    ) {
        if !trailing.trim().is_empty() {
            let start_abs = end + offset;
            let location = self.create_location(start_abs, start_abs + trailing.len());
            self.add_generic_warning_at(
                format!("unexpected content after {macro_name} macro: '{trailing}'"),
                location,
            );
        }
    }

    /// Collect an ad-hoc string warning with no source location. Prefer
    /// [`Self::add_generic_warning_at`] whenever the call site has a
    /// `Location`; untethered warnings are still useful (e.g. document-
    /// wide conditions) but callers that can place the warning should.
    pub(crate) fn add_generic_warning(&self, message: String) {
        self.add_warning(Warning::new(WarningKind::Other(Cow::Owned(message)), None));
    }

    /// Collect an ad-hoc string warning anchored to a grammar `Location`.
    /// Converts through `create_error_source_location` so the resulting
    /// `SourceLocation` carries the correct file, including the mapping
    /// through `source_ranges` for included content.
    pub(crate) fn add_generic_warning_at(&self, message: String, location: Location) {
        let source_location = self.create_error_source_location(location);
        self.add_warning(Warning::new(
            WarningKind::Other(Cow::Owned(message)),
            Some(source_location),
        ));
    }

    /// Collect a typed warning. Deduplicates against previously collected
    /// warnings (PEG backtracking can fire the same warning repeatedly).
    pub(crate) fn add_warning(&self, warning: Warning) {
        let mut warnings = self.warnings.borrow_mut();
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }

    /// Collect a warning raised by the inline preprocessor, first resolving its
    /// preprocessed location to the originating file + source line (the same
    /// mapping the error path uses). The inline preprocessor builds locations with
    /// `file: None` and post-splice line numbers; this brings warnings from
    /// `include::`d content and after preprocessor edits in line with the resolved
    /// AST node locations and the PEG-error path.
    pub(crate) fn add_inline_preprocessor_warning(&self, mut warning: Warning) {
        if let Some(source_location) = warning.location.take() {
            warning.location = Some(self.create_error_source_location(source_location.location));
        }
        self.add_warning(warning);
    }

    /// Emit all collected warnings via tracing. Call after parsing
    /// completes. Acts as a belt-and-suspenders fallback for callers that
    /// ignore the warnings slice on `ParseResult`.
    pub(crate) fn emit_warnings(&self) {
        for warning in self.warnings.borrow().iter() {
            tracing::warn!("{warning}");
        }
    }

    /// Create a `SourceLocation` for an error/warning, resolving the correct
    /// file and adjusting line numbers for included content.
    pub(crate) fn create_error_source_location(&self, location: Location) -> SourceLocation {
        if let Some(range) =
            SourceRange::find_containing(&self.source_ranges, location.absolute_start)
        {
            let file = range.file.clone();
            let adjusted_start_line =
                self.line_map
                    .source_line(range, self.input, location.absolute_start);
            let adjusted_end_line = self.line_map.source_line(
                range,
                self.input,
                location.absolute_end.min(self.input.len()),
            );
            SourceLocation {
                file,
                location: Location {
                    absolute_start: location.absolute_start,
                    absolute_end: location.absolute_end,
                    // The authoritative file for a diagnostic lives on the enclosing
                    // `SourceLocation`; these positions don't carry it.
                    start: crate::Position::new(adjusted_start_line, location.start.column),
                    end: crate::Position::new(adjusted_end_line, location.end.column),
                },
            }
        } else {
            SourceLocation {
                // Cold path: deep-copy the `Arc<PathBuf>` into the public `Option<PathBuf>`
                // (one clone per diagnostic). See `current_file` for why the API isn't widened.
                file: self.current_file.as_deref().cloned(),
                location,
            }
        }
    }

    /// Create a Location from raw byte offsets.
    ///
    /// This method enforces UTF-8 character boundaries:
    /// - Clamps offsets to input bounds
    /// - Rounds both start and end backward to nearest char boundary
    /// - Ensures start <= end
    ///
    /// We round end backward (not forward) because `absolute_end` has inclusive
    /// semantics - it represents the last byte of the range, not one past it.
    /// When a byte lands mid-character, rounding backward includes that character.
    pub(crate) fn create_location(&self, start: usize, end: usize) -> Location {
        use crate::grammar::utf8_utils::ensure_char_boundary;

        // Clamp to input bounds first
        let clamped_start = start.min(self.input.len());
        let clamped_end = end.min(self.input.len());

        // Ensure UTF-8 boundaries (both round backward for inclusive semantics)
        let safe_start = ensure_char_boundary(self.input, clamped_start);
        let safe_end = ensure_char_boundary(self.input, clamped_end);

        // Ensure start <= end
        let safe_end = safe_end.max(safe_start);

        let start_pos = self.line_map.offset_to_position(safe_start, self.input);
        let end_pos = self.line_map.offset_to_position(safe_end, self.input);

        // Positions carry no `file` at construction (the ASG omits the primary file).
        // The post-parse remap pass stamps the originating file on `include::`d nodes;
        // it only runs when the preprocessor recorded source ranges, which never happens
        // without an include/edit, so leaving `file` `None` is correct for the common
        // single-file document.
        Location {
            absolute_start: safe_start,
            absolute_end: safe_end,
            start: start_pos,
            end: end_pos,
        }
    }

    /// Helper to create block location with standard offset calculation.
    ///
    /// Adds `offset` to both start and end, then decrements end by one character
    /// (to exclude trailing delimiter/newline). UTF-8 safety is handled by `create_location`.
    pub(crate) fn create_block_location(
        &self,
        start: usize,
        end: usize,
        offset: usize,
    ) -> Location {
        use crate::grammar::utf8_utils::safe_decrement_offset;

        let adjusted_start = start + offset;
        let adjusted_end = end + offset;

        // Decrement end by one character (safely handling UTF-8)
        let final_end = if adjusted_end == 0 {
            0
        } else {
            safe_decrement_offset(self.input, adjusted_end)
        };

        // create_location handles all UTF-8 boundary enforcement
        self.create_location(adjusted_start, final_end)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::indexing_slicing)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    fn warning_kinds(state: &ParserState<'_>) -> Vec<WarningKind> {
        state
            .warnings
            .borrow()
            .iter()
            .map(|w| w.kind.clone())
            .collect()
    }

    fn other_kind(msg: &'static str) -> WarningKind {
        WarningKind::Other(Cow::Borrowed(msg))
    }

    #[test]
    fn add_warning_deduplicates_identical_messages() {
        let state = ParserState::new_for_test("test");
        state.add_generic_warning("duplicate warning".to_string());
        state.add_generic_warning("duplicate warning".to_string());
        state.add_generic_warning("duplicate warning".to_string());
        let warnings = state.warnings.borrow();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, other_kind("duplicate warning"));
    }

    #[test]
    fn add_warning_preserves_distinct_messages() {
        let state = ParserState::new_for_test("test");
        state.add_generic_warning("first".to_string());
        state.add_generic_warning("second".to_string());
        state.add_generic_warning("third".to_string());
        assert_eq!(state.warnings.borrow().len(), 3);
    }

    #[test]
    fn add_warning_preserves_insertion_order() {
        let state = ParserState::new_for_test("test");
        state.add_generic_warning("beta".to_string());
        state.add_generic_warning("alpha".to_string());
        state.add_generic_warning("beta".to_string());
        state.add_generic_warning("gamma".to_string());
        assert_eq!(
            warning_kinds(&state),
            vec![other_kind("beta"), other_kind("alpha"), other_kind("gamma"),],
        );
    }

    #[test]
    fn add_warning_is_shared_across_inline_subparses() {
        // `for_inline_parsing` must share the parent's warnings store so
        // a warning raised inside a substituted author/revision/inline
        // sub-parse reaches the outer `ParseResult`.
        let parent = ParserState::new_for_test("parent input");
        parent.add_generic_warning("from parent".to_string());
        {
            let child = ParserState::for_inline_parsing("child input", &parent, parent.inline_ctx);
            child.add_generic_warning("from child".to_string());
        }
        let kinds = warning_kinds(&parent);
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains(&other_kind("from parent")));
        assert!(kinds.contains(&other_kind("from child")));
    }

    #[test]
    #[tracing_test::traced_test]
    fn emit_warnings_outputs_via_tracing() {
        let state = ParserState::new_for_test("test");
        state.add_generic_warning("warning one".to_string());
        state.add_generic_warning("warning two".to_string());
        state.emit_warnings();
        assert!(logs_contain("warning one"));
        assert!(logs_contain("warning two"));
    }

    #[test]
    fn create_error_source_location_resolves_included_file() {
        use crate::model::SourceRange;

        // Simulate: main file "line1\n", included file "inc_line1\ninc_line2\n"
        let input = "line1\ninc_line1\ninc_line2\n";
        let mut state = ParserState::new_for_test(input);
        state.current_file = Some(PathBuf::from("main.adoc").into());
        state.source_ranges.push(SourceRange {
            start_offset: 6, // "inc_line1\n..." starts at byte 6
            end_offset: 25,
            file: Some(PathBuf::from("/tmp/included.adoc")),
            file_chain: vec!["included.adoc".to_string()],
            start_line: 1,
            source_start_offset: 0,
            column_shift: 0,
        });

        // Location inside the included range (second line of include: "inc_line2")
        let loc = state.create_location(16, 24); // "inc_line2"
        let src_loc = state.create_error_source_location(loc);

        assert_eq!(src_loc.file, Some(PathBuf::from("/tmp/included.adoc")));
        // From start_offset=6 to absolute_start=16, there's 1 newline ("inc_line1\n")
        // So start_line = 1 + 1 = 2
        assert_eq!(src_loc.location.start.line, 2);
    }

    #[test]
    fn create_error_source_location_falls_back_to_current_file() {
        let input = "main content\n";
        let mut state = ParserState::new_for_test(input);
        state.current_file = Some(PathBuf::from("main.adoc").into());
        // No source ranges

        let loc = state.create_location(0, 11);
        let src_loc = state.create_error_source_location(loc.clone());

        assert_eq!(src_loc.file, Some(PathBuf::from("main.adoc")));
        assert_eq!(src_loc.location.start.line, loc.start.line);
        assert_eq!(src_loc.location.end.line, loc.end.line);
    }
}
