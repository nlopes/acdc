//! The preprocessor module is responsible for processing the input document and expanding
//! include directives.
use std::{
    borrow::Cow,
    cell::RefCell,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
};

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

use crate::{
    AttributeValue, Location, Options, Warning, WarningKind,
    error::{Error, SourceLocation},
    model::{LeveloffsetRange, Position, SourceRange},
};

mod attribute;
mod comment;
mod conditional;
mod include;
mod tag;

use comment::CommentScanner;
use include::{Include, IncludeResult, IncludedLineOrigin, LocationContext};

/// Result from preprocessing that includes both the processed text and metadata needed
/// for accurate parsing (like leveloffset ranges).
///
/// `text` borrows from the caller's input when possible (fast path: no include /
/// conditional / multi-line-attribute triggers), enabling zero-copy parsing all
/// the way through the grammar. When any trigger fires, `text` is `Cow::Owned`.
#[derive(Debug, Default)]
pub(crate) struct PreprocessorResult<'a> {
    /// The preprocessed document text.
    pub(crate) text: Cow<'a, str>,
    /// Byte ranges where specific leveloffset values apply.
    /// Used by the parser to adjust section levels.
    pub(crate) leveloffset_ranges: Vec<LeveloffsetRange>,
    /// Byte ranges mapping preprocessed output back to source files.
    /// Used by the parser to produce accurate file/line info in warnings.
    pub(crate) source_ranges: Vec<SourceRange>,
}

impl PreprocessorResult<'_> {
    /// Materialize any borrowed text into an owned `PreprocessorResult<'static>`.
    /// Used by `process_file` / `process_reader` where the source buffer is a
    /// local and cannot outlive the function.
    fn into_owned(self) -> PreprocessorResult<'static> {
        PreprocessorResult {
            text: Cow::Owned(self.text.into_owned()),
            leveloffset_ranges: self.leveloffset_ranges,
            source_ranges: self.source_ranges,
        }
    }
}

/// Per-directive context bundling position and file information that
/// would otherwise need to be threaded individually through every
/// directive helper (and push the argument count past clippy's limit).
#[derive(Debug)]
struct DirectiveContext<'a> {
    line_number: &'a mut usize,
    current_offset: usize,
    source_origin: Option<&'a SourceOrigin>,
}

impl DirectiveContext<'_> {
    fn current_file(&self) -> Option<&Path> {
        self.source_origin.map(SourceOrigin::as_path)
    }
}

/// Origin used to resolve includes while preprocessing a source.
///
/// URI origins retain their exact spelling because Asciidoctor resolves nested
/// URI targets by literal directory-and-target concatenation rather than RFC URL
/// joining or filesystem resolution.
#[derive(Debug, Clone)]
pub(super) enum SourceOrigin {
    File(PathBuf),
    Uri(String),
}

impl SourceOrigin {
    fn as_path(&self) -> &Path {
        match self {
            Self::File(path) => path,
            Self::Uri(uri) => Path::new(uri),
        }
    }
}

/// An open block-form conditional and whether its content is active after
/// accounting for every enclosing conditional.
#[derive(Debug)]
struct ConditionalFrame<'input> {
    conditional: conditional::Conditional<'input>,
    active: bool,
}

/// Mutable state accumulated during preprocessing.
struct PreprocessorState<'input> {
    input: &'input str,
    output: Vec<Cow<'input, str>>,
    /// A maximal run of unchanged output lines borrowed directly from `input`.
    /// Interior newlines are part of the range; the newline after the final line
    /// is supplied when output chunks are joined.
    borrowed_run: Option<Range<usize>>,
    byte_offset: usize,
    leveloffset_ranges: Vec<LeveloffsetRange>,
    source_ranges: Vec<SourceRange>,
    /// The file the in-progress output is being read from, used as the `file`
    /// of the main-file [`SourceRange`]s recorded below. `None` for stdin/string
    /// input — ranges are still recorded (so line/offset remapping works) with a
    /// `None` file.
    source_file: Option<std::path::PathBuf>,
    /// Byte offset of each 1-indexed source line in the (normalized) primary input,
    /// so a run/chunk anchored at a source line can record its origin-file byte
    /// offset. `src_line_starts[n - 1]` is the start of source line `n`.
    src_line_starts: Vec<usize>,
    /// The open main-file source range: byte offset in the output where the
    /// current contiguous run began, and the original source line/offset it maps to.
    run: Option<MainFileRun>,
    /// The source line the next contiguous main-file output line is expected to
    /// have. A mismatch means the source skipped lines (a dropped comment, a
    /// stripped conditional, …) and the current run must be closed.
    run_expected_src_line: usize,
}

/// Bookkeeping for a contiguous run of main-file output whose source lines are
/// consecutive, so a single [`SourceRange`] maps it back to the original file.
#[derive(Debug, Clone, Copy)]
struct MainFileRun {
    out_start: usize,
    src_start_line: usize,
    src_start_offset: usize,
}

/// How a recorded preprocessed span maps back to its origin file: the origin
/// line/offset of the span's first byte plus the per-line column shift from an
/// `indent=` re-indent (`0` = byte-for-byte 1:1 copy).
#[derive(Debug, Clone, Copy)]
struct OriginMapping {
    start_line: usize,
    source_start_offset: usize,
    column_shift: isize,
}

impl OriginMapping {
    /// A 1:1 (un-transformed) mapping — content copied verbatim, no re-indent.
    fn one_to_one(start_line: usize, source_start_offset: usize) -> Self {
        Self {
            start_line,
            source_start_offset,
            column_shift: 0,
        }
    }
}

impl<'input> PreprocessorState<'input> {
    fn new(input: &'input str, source_origin: Option<&SourceOrigin>) -> Self {
        let mut src_line_starts = vec![0];
        src_line_starts.extend(
            input
                .bytes()
                .enumerate()
                .filter_map(|(idx, byte)| (byte == b'\n').then_some(idx + 1)),
        );
        Self {
            input,
            output: Vec::new(),
            borrowed_run: None,
            byte_offset: 0,
            leveloffset_ranges: Vec::new(),
            source_ranges: Vec::new(),
            source_file: source_origin.map(|origin| origin.as_path().to_path_buf()),
            src_line_starts,
            run: None,
            run_expected_src_line: 1,
        }
    }

    /// Flush the pending unchanged source run into a single borrowed output
    /// chunk. This is the key slow-path allocation invariant: retained source
    /// lines are grouped by contiguous input range instead of owned one-by-one.
    fn flush_borrowed_run(&mut self) {
        if let Some(range) = self.borrowed_run.take() {
            self.output.push(Cow::Borrowed(&self.input[range]));
        }
    }

    /// Emit a line that cannot be coalesced with unchanged source text.
    fn push_line(&mut self, line: Cow<'input, str>) {
        self.flush_borrowed_run();
        self.byte_offset += line.len() + 1;
        self.output.push(line);
    }

    /// Emit an unchanged line from the primary input. Consecutive source lines
    /// extend a single borrowed range, including their interior newlines.
    fn push_source_line(&mut self, line: &'input str, src_line: usize) {
        self.note_source_line(src_line);

        let start = self.src_offset_of_line(src_line);
        let end = start + line.len();
        debug_assert_eq!(self.input.get(start..end), Some(line));

        if let Some(range) = &mut self.borrowed_run
            && range.end.checked_add(1) == Some(start)
        {
            range.end = end;
        } else {
            self.flush_borrowed_run();
            self.borrowed_run = Some(start..end);
        }
        self.byte_offset += line.len() + 1;
    }

    /// Byte offset of 1-indexed source `line` in the primary input.
    fn src_offset_of_line(&self, line: usize) -> usize {
        self.src_line_starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(0)
    }

    /// Account a simple one-source-line → one-output-line emission, extending the
    /// current run or starting a fresh one when the source line is not
    /// consecutive with the previous emitted line. Call **before** `push_line`.
    fn note_source_line(&mut self, src_line: usize) {
        if self.run.is_some() && src_line != self.run_expected_src_line {
            self.flush_run();
        }
        if self.run.is_none() {
            self.run = Some(MainFileRun {
                out_start: self.byte_offset,
                src_start_line: src_line,
                src_start_offset: self.src_offset_of_line(src_line),
            });
        }
        self.run_expected_src_line = src_line + 1;
    }

    /// Record a [`SourceRange`] mapping the preprocessed span `[start_offset,
    /// end_offset)` back to `file` (resolved, for diagnostics) and `file_chain` (the
    /// include targets as written, for the ASG) at `start_line` / `source_start_offset`.
    fn push_source_range(
        &mut self,
        start_offset: usize,
        end_offset: usize,
        file: Option<PathBuf>,
        file_chain: Vec<String>,
        origin: OriginMapping,
    ) {
        self.source_ranges.push(SourceRange {
            start_offset,
            end_offset,
            file,
            file_chain,
            start_line: origin.start_line,
            source_start_offset: origin.source_start_offset,
            column_shift: origin.column_shift,
        });
    }

    /// Close the open run, recording its `[out_start, byte_offset)` span as a
    /// main-file [`SourceRange`]. No-op when no bytes were emitted.
    fn flush_run(&mut self) {
        if let Some(run) = self.run.take()
            && self.byte_offset > run.out_start
        {
            self.push_source_range(
                run.out_start,
                self.byte_offset,
                self.source_file.clone(),
                // Own content: the include chain is empty here; an enclosing
                // `include::` prepends its target when this file's ranges are merged.
                Vec::new(),
                // Own (un-spliced) main-file content is a byte-for-byte 1:1 copy.
                OriginMapping::one_to_one(run.src_start_line, run.src_start_offset),
            );
        }
    }

    /// Emit synthesized content (a collapsed attribute continuation or active
    /// single-line conditional) as its own standalone [`SourceRange`] anchored at
    /// `src_start_line`, so it never shares a run with surrounding lines whose
    /// output-newline count would otherwise be miscounted.
    fn push_chunk(&mut self, content: String, src_start_line: usize) {
        self.flush_run();
        let start = self.byte_offset;
        let source_start_offset = self.src_offset_of_line(src_start_line);
        self.push_line(Cow::Owned(content));
        // Synthesized content is not re-indented.
        self.push_source_range(
            start,
            self.byte_offset,
            self.source_file.clone(),
            Vec::new(),
            OriginMapping::one_to_one(src_start_line, source_start_offset),
        );
    }
}

/// BOM (Byte Order Mark) patterns for encoding detection
const BOM_PATTERNS: &[(&[u8], &Encoding, usize, &str)] = &[
    (&[0xEF, 0xBB, 0xBF], UTF_8, 3, "UTF-8"),
    (&[0xFF, 0xFE], UTF_16LE, 2, "UTF-16 LE"),
    (&[0xFE, 0xFF], UTF_16BE, 2, "UTF-16 BE"),
];

/// Reads a file and decodes it based on BOM (Byte Order Mark) or explicit encoding.
///
/// Supports:
/// - UTF-8 with BOM (EF BB BF)
/// - UTF-16 LE with BOM (FF FE)
/// - UTF-16 BE with BOM (FE FF)
/// - UTF-8 without BOM (fallback)
/// - Explicit encoding via `encoding` parameter
///
/// # Errors
/// Returns an error if:
/// - The file cannot be read
/// - The explicit encoding label is unknown
/// - The file is not valid UTF-8 and has no BOM
pub(crate) fn read_and_decode_file(
    file_path: &Path,
    encoding: Option<&str>,
) -> Result<String, Error> {
    let bytes = std::fs::read(file_path)?;
    decode_bytes(&bytes, encoding, &file_path.display().to_string())
}

/// Decode source bytes using an explicit encoding, BOM detection, or UTF-8.
pub(super) fn decode_bytes(
    bytes: &[u8],
    encoding: Option<&str>,
    source: &str,
) -> Result<String, Error> {
    // If there was an encoding specified, decode the entire file as that
    if let Some(enc_label) = encoding {
        if let Some(encoding) = Encoding::for_label(enc_label.as_bytes()) {
            let (cow, _, had_errors) = encoding.decode(bytes);
            if had_errors {
                tracing::error!(
                    %source,
                    encoding = %enc_label,
                    "decoding encountered errors"
                );
            }
            return Ok(cow.into_owned());
        }
        return Err(Error::UnknownEncoding(enc_label.to_string()));
    }

    // Check for BOM patterns and decode accordingly
    for (bom, encoding, skip, name) in BOM_PATTERNS {
        if bytes.starts_with(bom)
            && let Some(content) = bytes.get(*skip..)
        {
            let (cow, _, had_errors) = encoding.decode(content);
            if had_errors {
                tracing::error!(
                    %source,
                    encoding = name,
                    "decoding encountered errors"
                );
            }
            return Ok(cow.into_owned());
        }
    }

    // If no BOM, try decoding as UTF-8 directly
    let (cow, _, had_errors) = UTF_8.decode(bytes);
    if !had_errors {
        return Ok(cow.into_owned());
    }

    // If you get here, the file is not valid UTF-8 (and no BOM)
    Err(Error::UnrecognizedEncodingInFile(source.to_string()))
}

/// Preprocessor shared across `Include` / `Conditional` / `Tag` helpers.
///
/// Carries a warning sink (`Rc<RefCell<Vec<Warning>>>`) that the caller
/// — typically `parse_input` / `parse_inline` in `lib.rs` — also hands
/// to the later `ParserState`. That makes preprocessor warnings
/// (missing includes, unclosed tags, bad attribute lines, if/endif
/// mismatches, ...) reach `ParseResult::warnings()` alongside grammar
/// warnings.
///
/// All public entry points take the handle explicitly; the struct is
/// a carrier so nested `&self` helpers (`process_include`,
/// `process_conditional_line`, `process_directive_line`) can reach the sink and
/// immutable caller authority without threading them through every parameter list.
#[derive(Debug)]
pub(crate) struct Preprocessor {
    warnings: Rc<RefCell<Vec<Warning>>>,
    /// Whether the caller supplied `allow-uri-read` before document attributes
    /// were processed. Document content must not be able to grant this authority.
    caller_allows_uri_read: bool,
}

impl Preprocessor {
    fn new(options: &Options<'_>, warnings: Rc<RefCell<Vec<Warning>>>) -> Self {
        Self {
            warnings,
            caller_allows_uri_read: matches!(
                options.document_attributes.get("allow-uri-read"),
                Some(AttributeValue::String(_) | AttributeValue::Bool(true))
            ),
        }
    }

    /// Push a warning with an attached source location, also emitting it
    /// through `tracing::warn!` as a belt-and-suspenders fallback so
    /// subscribers keep seeing the same messages.
    pub(crate) fn add_warning_at(
        &self,
        message: impl Into<Cow<'static, str>>,
        location: SourceLocation,
    ) {
        let warning = Warning::new(WarningKind::Other(message.into()), Some(location));
        tracing::warn!(?warning);
        self.warnings.borrow_mut().push(warning);
    }
}

impl Preprocessor {
    /// Helper to create a `SourceLocation` from preprocessor context (line-level precision).
    ///
    /// Since the preprocessor operates line-by-line and doesn't track column positions,
    /// we use column=1 as a placeholder. The line number and offset still provide
    /// useful location information for error messages.
    fn create_source_location(line_number: usize, file_parent: Option<&Path>) -> SourceLocation {
        SourceLocation {
            file: file_parent.map(Path::to_path_buf),
            // Preprocessor doesn't track column — use 0 as placeholder.
            location: Location::point(Position::from_line_col(line_number, 0)),
        }
    }

    /// Normalize line endings and trailing whitespace.
    ///
    /// Fast path: when the input already uses LF line endings, has no trailing
    /// whitespace on any line, and no embedded CR characters, return
    /// `Cow::Borrowed` (optionally trimming a single trailing newline to match
    /// `str::lines`'s drop-trailing-empty behavior). Slow path: rebuild the
    /// buffer line-by-line as before.
    fn normalize(input: &str) -> Cow<'_, str> {
        // Match the original behavior of `input.lines()`: a single trailing
        // newline (`\n` or `\r\n`) is dropped.
        let trimmed = input
            .strip_suffix("\r\n")
            .or_else(|| input.strip_suffix('\n'))
            .unwrap_or(input);
        // Any `\r` (including inside `\r\n` pairs) or trailing whitespace on a
        // line forces a rebuild.
        let needs_rebuild = trimmed.as_bytes().contains(&b'\r')
            || trimmed
                .split('\n')
                .any(|line| matches!(line.as_bytes().last(), Some(b' ' | b'\t')));
        if !needs_rebuild {
            return Cow::Borrowed(trimmed);
        }
        let mut result = String::with_capacity(input.len());
        for (i, line) in input.lines().map(str::trim_end).enumerate() {
            if i > 0 {
                result.push('\n');
            }
            result.push_str(line);
        }
        Cow::Owned(result)
    }

    /// Decides whether the normalized `text` can be used as-is, letting the
    /// caller skip the full line-by-line rebuild in [`Self::process_inner`].
    ///
    /// Returns `true` when running `process_inner` would produce byte-identical
    /// output, so the caller returns the normalized text directly (zero-copy,
    /// preserving any borrow from the original input). Returns `false` as soon
    /// as it spots something `process_inner` would rewrite: an
    /// `include`/`ifdef`/`ifndef`/`ifeval` directive (or its escaped form), a
    /// multi-line attribute continuation, or a line comment that would be
    /// dropped.
    ///
    /// Read-only: it must not mutate options. Single-line attributes
    /// (`:attr: value`) are left to downstream parsing — `process_inner` only
    /// mutates attributes to evaluate conditional directives, which already
    /// force the rebuild.
    ///
    /// Directive detection is unconditional: an `include::` inside a `----`
    /// block is still processed by `process_inner` (matching asciidoctor,
    /// exercised by `include_with_indent.adoc`), so those lines must not be
    /// skipped. The verbatim tracking in the scanner below is only for the
    /// comment decision — comments inside verbatim blocks are kept.
    fn try_pass_through(text: &str, setext: bool) -> bool {
        // Dropping an adjacent line comment is the one rewrite that can't be
        // spotted with a simple per-line check: whether a `//` line is dropped
        // depends on the previous line and on being outside a verbatim block.
        // Run the same `CommentScanner` that `process_inner` uses, and return
        // false only on an actual drop — so documents whose comments are all
        // kept (standalone, inside verbatim, tag directives) still pass through.
        let mut scanner = CommentScanner::new(setext);
        for line in text.lines() {
            // `process_inner` unescapes escaped directives.
            if line.starts_with("\\include")
                || line.starts_with("\\ifdef")
                || line.starts_with("\\ifndef")
                || line.starts_with("\\ifeval")
            {
                return false;
            }
            // `process_inner` collapses multi-line attribute continuations.
            if line.starts_with(':') && (line.ends_with(" + \\") || line.ends_with(" \\")) {
                return false;
            }
            // Directive lines: include::, ifdef::, ifndef::, ifeval::
            if line.ends_with(']')
                && !line.starts_with('[')
                && line.contains("::")
                && (line.starts_with("include")
                    || line.starts_with("ifdef")
                    || line.starts_with("ifndef")
                    || line.starts_with("ifeval"))
            {
                return false;
            }
            if scanner.at_verbatim_delimiter(line) {
                scanner.record(line);
                continue;
            }
            if scanner.drops(line) {
                // `process_inner` would drop this adjacent comment, so the text
                // is not byte-identical — force the rebuild.
                return false;
            }
            scanner.record(line);
        }
        true
    }

    #[tracing::instrument(skip(reader, warnings))]
    pub(crate) fn process_reader<R: std::io::Read>(
        mut reader: R,
        options: &Options,
        warnings: Rc<RefCell<Vec<Warning>>>,
    ) -> Result<PreprocessorResult<'static>, Error> {
        let mut input = String::new();
        reader.read_to_string(&mut input).map_err(|e| {
            tracing::error!(error=?e, "failed to read from reader");
            e
        })?;
        // The local `input` cannot outlive this function, so materialize any
        // borrowed text into an owned result.
        Ok(Self::new(options, warnings)
            .process_inner(&input, None, options)?
            .into_owned())
    }

    #[tracing::instrument(skip(warnings))]
    pub(crate) fn process<'a>(
        input: &'a str,
        options: &Options,
        warnings: Rc<RefCell<Vec<Warning>>>,
    ) -> Result<PreprocessorResult<'a>, Error> {
        Self::new(options, warnings).process_inner(input, None, options)
    }

    /// Like `process` but lets the caller pass the file path explicitly, used
    /// by `parse_file` where the input has already been read and leaked.
    #[tracing::instrument(skip(file_path, warnings))]
    pub(crate) fn process_with_file<'a>(
        input: &'a str,
        file_path: &Path,
        options: &Options,
        warnings: Rc<RefCell<Vec<Warning>>>,
    ) -> Result<PreprocessorResult<'a>, Error> {
        let source_origin = SourceOrigin::File(file_path.to_path_buf());
        Self::new(options, warnings).process_inner(input, Some(&source_origin), options)
    }

    #[cfg(test)]
    #[tracing::instrument(skip(file_path, warnings))]
    pub(crate) fn process_file<P: AsRef<Path>>(
        file_path: P,
        options: &Options,
        warnings: Rc<RefCell<Vec<Warning>>>,
    ) -> Result<PreprocessorResult<'static>, Error> {
        if file_path.as_ref().parent().is_some() {
            // Use read_and_decode_file to support UTF-8, UTF-16 LE, and UTF-16 BE with BOM
            let input = read_and_decode_file(file_path.as_ref(), None)?;
            let source_origin = SourceOrigin::File(file_path.as_ref().to_path_buf());
            Ok(Self::new(options, warnings)
                .process_inner(&input, Some(&source_origin), options)?
                .into_owned())
        } else {
            Err(Error::InvalidIncludePath(
                Box::new(Self::create_source_location(1, Some(file_path.as_ref()))),
                file_path.as_ref().to_path_buf(),
            ))
        }
    }

    /// Process an include directive.
    ///
    /// Returns the included content along with any leveloffset that applies.
    #[tracing::instrument(skip(self))]
    fn process_include(
        &self,
        line: &str,
        line_number: usize,
        current_offset: usize,
        source_origin: Option<&SourceOrigin>,
        options: &Options,
    ) -> Result<Option<IncludeResult>, Error> {
        if let Some(source_origin) = source_origin {
            let include = Include::parse(
                source_origin,
                line,
                LocationContext::new(line_number, current_offset, Some(source_origin.as_path())),
                options,
                self.caller_allows_uri_read,
                &self.warnings,
            )?;
            return Ok(Some(include.lines()?));
        }
        tracing::error!(%line, "source origin is missing - include directive cannot be processed");
        Ok(None)
    }

    #[tracing::instrument(skip(lines, attribute_content))]
    fn process_continuation<'a, I: Iterator<Item = &'a str>>(
        attribute_content: &mut String,
        lines: &mut std::iter::Peekable<I>,
        line_number: &mut usize,
    ) {
        while let Some(next_line) = lines.peek() {
            let next_line = next_line.trim();
            // If the next line isn't the end of a continuation, or a
            // continuation, we need to break out.
            if next_line.starts_with(':') || next_line.is_empty() {
                break;
            }
            // If we get here, and we get a hard wrap, keep everything as is.
            // If we get here, and we get a soft wrap, then remove the newline.
            // Anything else means we're at the end of the wrapped attribute, so
            // feed it and break.
            if next_line.ends_with(" + \\") {
                attribute_content.push_str(next_line);
                attribute_content.push('\n');
                lines.next();
                *line_number += 1;
            } else if next_line.ends_with(" \\") {
                attribute_content.push_str(next_line.trim_end_matches('\\'));
                lines.next();
                *line_number += 1;
            } else {
                attribute_content.push_str(next_line);
                lines.next();
                *line_number += 1;
                break;
            }
        }
    }

    /// Check if a line is a verbatim or raw block delimiter.
    ///
    /// Verbatim/raw blocks preserve content literally, including comments.
    /// Recognized delimiters:
    /// - `----` (listing/source blocks) - 4+ hyphens
    /// - `....` (literal blocks) - 4+ periods
    /// - `++++` (passthrough blocks) - 4+ plus signs
    /// - ` ``` ` (markdown code fences) - 3+ backticks
    #[tracing::instrument]
    pub(super) fn is_verbatim_delimiter(line: &str) -> Option<&str> {
        let trimmed = line.trim();

        // Check for markdown code fences (3+ backticks)
        if trimmed.starts_with("```") {
            return Some("```");
        }

        // Check for other delimiters (4+ chars)
        //
        // We need to fetch the same delimiter size to make sure we close the block
        // correctly, and the minimum size is 4.
        let mut chars = trimmed.chars();
        let first_char = chars.next()?;
        if first_char != '-' && first_char != '.' && first_char != '+' {
            return None;
        }
        let mut idx = 1;
        for next_char in chars {
            if next_char == first_char {
                idx += 1;
            } else {
                break;
            }
        }
        if idx >= 4 {
            return trimmed.get(..idx);
        }
        None
    }

    /// Handle the result of processing an include directive.
    /// Records leveloffset and source ranges, and extends output with included lines.
    ///
    /// This also merges nested ranges from included files, adjusting their byte offsets
    /// to be relative to the current output position. This enables proper accumulation
    /// through arbitrarily deep include nesting.
    fn handle_include_result(include_result: IncludeResult, state: &mut PreprocessorState<'_>) {
        state.flush_borrowed_run();
        let start_offset = state.byte_offset;

        // Calculate the byte length of the included content
        let content_len: usize = include_result
            .lines
            .iter()
            .map(|l| l.len() + 1) // +1 for newline
            .sum();

        // If there's an effective leveloffset, record the range
        if let Some(leveloffset) = include_result.effective_leveloffset {
            if leveloffset != 0 {
                state.leveloffset_ranges.push(LeveloffsetRange::new(
                    start_offset,
                    start_offset + content_len,
                    leveloffset,
                ));
                tracing::trace!(
                    leveloffset,
                    start_offset,
                    end_offset = start_offset + content_len,
                    "Recording leveloffset range for include"
                );
            }
        }

        // Merge nested leveloffset ranges from the included file.
        // Shift their byte offsets to be relative to the current output position.
        // This enables proper accumulation through nested includes (A→B→C).
        for nested_range in include_result.nested_leveloffset_ranges {
            let adjusted_range = LeveloffsetRange::new(
                nested_range.start_offset + start_offset,
                nested_range.end_offset + start_offset,
                nested_range.value,
            );
            tracing::trace!(
                original_start = nested_range.start_offset,
                original_end = nested_range.end_offset,
                adjusted_start = adjusted_range.start_offset,
                adjusted_end = adjusted_range.end_offset,
                leveloffset = adjusted_range.value,
                "Merging nested leveloffset range"
            );
            state.leveloffset_ranges.push(adjusted_range);
        }

        // Record the included content's source ranges. `target` is the include target
        // as written in the directive — the outermost element of every range's ASG
        // chain. A whole-file include's lines are consecutive (`1..N`), collapsing into
        // one range anchored at line 1 / byte 0. A partial (`lines=`/`tags=`) include
        // splits into one range per maximal run of consecutive origin lines, each
        // anchored at the run's true origin line and byte offset.
        if let Some(file) = include_result.file {
            let target = include_result.target;

            // Group emitted lines into maximal runs of consecutive origin lines,
            // tracking each run's `[out_start, out_end)` span in the preprocessed
            // buffer alongside its origin anchor.
            let mut runs: Vec<(usize, usize, IncludedLineOrigin)> = Vec::new();
            let mut cursor = start_offset;
            let mut expected_line = 0;
            for (line, origin) in include_result
                .lines
                .iter()
                .zip(&include_result.source_lines)
            {
                let out_end = cursor + line.len() + 1;
                if origin.line == expected_line
                    && let Some(run) = runs.last_mut()
                {
                    run.1 = out_end;
                } else {
                    runs.push((cursor, out_end, *origin));
                }
                cursor = out_end;
                expected_line = origin.line + 1;
            }
            for (out_start, out_end, origin) in runs {
                state.push_source_range(
                    out_start,
                    out_end,
                    Some(file.clone()),
                    vec![target.clone()],
                    OriginMapping {
                        start_line: origin.line,
                        source_start_offset: origin.offset,
                        // `indent=N` re-indents every line of this include uniformly;
                        // the remap subtracts this to recover origin columns.
                        column_shift: include_result.column_shift,
                    },
                );
            }
            tracing::trace!(
                ?file,
                start_offset,
                end_offset = start_offset + content_len,
                "Recording source range for include"
            );

            // Merge nested source ranges, shifting preprocessed byte offsets to the
            // current output position; the origin file and source offset/line are
            // already relative to the nested file, so they carry over unchanged. The
            // include chain gains this include's `target` at its front (this file is
            // the parent of everything the nested file reached).
            for nested_range in include_result.nested_source_ranges {
                let mut file_chain = Vec::with_capacity(nested_range.file_chain.len() + 1);
                file_chain.push(target.clone());
                file_chain.extend(nested_range.file_chain);
                state.push_source_range(
                    nested_range.start_offset + start_offset,
                    nested_range.end_offset + start_offset,
                    nested_range.file,
                    file_chain,
                    OriginMapping {
                        start_line: nested_range.start_line,
                        source_start_offset: nested_range.source_start_offset,
                        // A nested range keeps the shift it was recorded with (an inner
                        // `indent=` it carries); this level's own `indent=` is not
                        // composed onto it.
                        column_shift: nested_range.column_shift,
                    },
                );
            }
        }

        state.byte_offset += content_len;
        state
            .output
            .extend(include_result.lines.into_iter().map(Cow::Owned));
    }

    /// Process a block or single-line conditional directive.
    ///
    /// Returns `true` when `line` was a conditional directive and therefore
    /// must not be emitted as ordinary document content.
    fn process_conditional_line<'input>(
        &self,
        line: &'input str,
        ctx: &DirectiveContext<'_>,
        attributes: &crate::DocumentAttributes,
        stack: &mut Vec<ConditionalFrame<'input>>,
        out: &mut PreprocessorState<'_>,
    ) -> Result<bool, Error> {
        if !line.ends_with(']') || line.starts_with('[') || !line.contains("::") {
            return Ok(false);
        }

        if line.starts_with("ifdef") || line.starts_with("ifndef") || line.starts_with("ifeval") {
            let mut content = String::new();
            let conditional = conditional::parse_line(
                line,
                *ctx.line_number,
                ctx.current_offset,
                ctx.current_file(),
            )?;
            let parent_active = stack.last().is_none_or(|frame| frame.active);
            let is_inline = conditional.has_inline_content();
            let active = parent_active
                && conditional.is_true(
                    attributes,
                    &mut content,
                    *ctx.line_number,
                    ctx.current_offset,
                    ctx.current_file(),
                )?;

            if is_inline {
                if active {
                    out.push_chunk(content, *ctx.line_number);
                }
            } else {
                stack.push(ConditionalFrame {
                    conditional,
                    active,
                });
            }
            return Ok(true);
        }

        if line.starts_with("endif")
            && let Some(frame) = stack.last()
        {
            let endif = conditional::parse_endif(
                line,
                *ctx.line_number,
                ctx.current_offset,
                ctx.current_file(),
            )?;
            if !endif.closes(&frame.conditional) {
                self.add_warning_at(
                    "attribute mismatch between if and endif directives",
                    Self::create_source_location(*ctx.line_number, ctx.current_file()),
                );
                return Err(Error::InvalidConditionalDirective(Box::new(
                    Self::create_source_location(*ctx.line_number, ctx.current_file()),
                )));
            }
            stack.pop();
            return Ok(true);
        }

        Ok(false)
    }

    fn process_directive_line<'input>(
        &self,
        line: &'input str,
        ctx: &mut DirectiveContext<'_>,
        options: &Options,
        out: &mut PreprocessorState<'input>,
    ) -> Result<(), Error> {
        if line.starts_with("\\include")
            || line.starts_with("\\ifdef")
            || line.starts_with("\\ifndef")
            || line.starts_with("\\ifeval")
        {
            out.note_source_line(*ctx.line_number);
            out.push_line(Cow::Borrowed(&line[1..]));
        } else if line.starts_with("include") {
            // Included content carries its own source ranges; close the main-file
            // run first so it does not overlap them.
            out.flush_run();
            if let Some(include_result) = self.process_include(
                line,
                *ctx.line_number,
                ctx.current_offset,
                ctx.source_origin,
                options,
            )? {
                Self::handle_include_result(include_result, out);
            }
        } else {
            out.push_source_line(line, *ctx.line_number);
        }
        Ok(())
    }

    #[tracing::instrument]
    fn process_inner<'a>(
        &self,
        input: &'a str,
        source_origin: Option<&SourceOrigin>,
        options: &Options,
    ) -> Result<PreprocessorResult<'a>, Error> {
        let normalized = Preprocessor::normalize(input);
        let setext = comment::setext_enabled(options);

        // Fast path: no triggers means the slow rebuild would produce byte-identical
        // output. Return the normalized text directly, preserving any borrow from
        // the caller's input so that downstream parsing can be zero-copy.
        if Self::try_pass_through(&normalized, setext) {
            return Ok(PreprocessorResult {
                text: normalized,
                leveloffset_ranges: Vec::new(),
                source_ranges: Vec::new(),
            });
        }

        self.process_slow_path(&normalized, source_origin, options, setext)
    }

    /// Rebuild a document that contains at least one preprocessor trigger.
    ///
    /// Keep this out of line so the much larger include/conditional machinery
    /// cannot perturb instruction layout for the common pass-through path.
    #[cold]
    #[inline(never)]
    fn process_slow_path<'a>(
        &self,
        normalized: &Cow<'a, str>,
        source_origin: Option<&SourceOrigin>,
        options: &Options,
        setext: bool,
    ) -> Result<PreprocessorResult<'a>, Error> {
        // Slow path: at least one trigger fires (include, conditional,
        // multi-line attribute continuation, or escaped directive). Rebuild
        // line-by-line. The output is materialized before `normalized` drops,
        // whether normalization borrowed the caller's input or created a buffer.
        let normalized_ref = normalized.as_ref();

        let mut options = options.clone();
        let mut lines = normalized_ref.lines().peekable();
        let mut line_number = 1;
        let mut current_offset = 0;
        let mut out = PreprocessorState::new(normalized_ref, source_origin);
        // Tracks verbatim-block and previous-line context so adjacent line
        // comments can be dropped (matching asciidoctor's reader). See
        // `comment::CommentScanner`.
        let mut scanner = CommentScanner::new(setext);
        let mut conditional_stack = Vec::new();

        while let Some(line) = lines.next() {
            let conditional_consumed = {
                let ctx = DirectiveContext {
                    line_number: &mut line_number,
                    current_offset,
                    source_origin,
                };
                self.process_conditional_line(
                    line,
                    &ctx,
                    &options.document_attributes,
                    &mut conditional_stack,
                    &mut out,
                )?
            };
            if conditional_consumed {
                current_offset += line.len() + 1;
                line_number += 1;
                continue;
            }

            if conditional_stack.last().is_some_and(|frame| !frame.active) {
                current_offset += line.len() + 1;
                line_number += 1;
                continue;
            }

            if line.starts_with(':') && (line.ends_with(" + \\") || line.ends_with(" \\")) {
                let mut attribute_content = String::with_capacity(line.len() * 2);
                if line.ends_with(" + \\") {
                    attribute_content.push_str(line);
                    attribute_content.push('\n');
                } else if line.ends_with(" \\") {
                    attribute_content.push_str(line.trim_end_matches('\\'));
                }
                // The attribute spans `[continuation_start_line, line_number]`;
                // emit it as its own range anchored at its first line.
                let continuation_start_line = line_number;
                Self::process_continuation(&mut attribute_content, &mut lines, &mut line_number);
                attribute::parse_line(&mut options.document_attributes, attribute_content.as_str());
                out.push_chunk(attribute_content, continuation_start_line);
                scanner.record(line);
                // `process_continuation` advanced `line_number` over the absorbed
                // continuation lines but not the attribute line itself; the `continue`
                // below skips the loop-tail increment, so account for it here to keep
                // `line_number` aligned with the source for everything that follows.
                line_number += 1;
                current_offset += line.len() + 1;
                continue;
            } else if line.starts_with(':') {
                attribute::parse_line(&mut options.document_attributes, line.trim());
            }
            if scanner.at_verbatim_delimiter(line) {
                out.push_source_line(line, line_number);
            } else if line.starts_with("//") {
                if scanner.drops(line) {
                    // Drop the adjacent comment; don't `record` it, so a run of
                    // comments after content is dropped together. The dropped line
                    // breaks source-line continuity, closing the current run when
                    // the next content line is emitted.
                    current_offset += line.len() + 1;
                    line_number += 1;
                    continue;
                }
                out.push_source_line(line, line_number);
            } else if line.ends_with(']') && !line.starts_with('[') && line.contains("::") {
                let mut ctx = DirectiveContext {
                    line_number: &mut line_number,
                    current_offset,
                    source_origin,
                };
                self.process_directive_line(line, &mut ctx, &options, &mut out)?;
            } else {
                out.push_source_line(line, line_number);
            }
            scanner.record(line);
            current_offset += line.len() + 1;
            line_number += 1;
        }

        out.flush_run();
        out.flush_borrowed_run();

        Ok(PreprocessorResult {
            text: Cow::Owned(out.output.join("\n")),
            leveloffset_ranges: out.leveloffset_ranges,
            source_ranges: out.source_ranges,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::LineMap;

    #[test]
    fn unchanged_source_lines_share_one_borrowed_output_chunk() {
        let input = "first\n\nsecond\nthird";
        let mut state = PreprocessorState::new(input, None);

        for (index, line) in input.lines().enumerate() {
            state.push_source_line(line, index + 1);
        }
        state.flush_borrowed_run();

        assert_eq!(state.output.len(), 1);
        assert!(matches!(
            state.output.as_slice(),
            [Cow::Borrowed(output)] if *output == input
        ));
    }

    #[test]
    fn test_process() -> Result<(), Error> {
        let options = Options::default();
        let input = ":attribute: value

ifdef::attribute[]
content
endif::[]
";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, ":attribute: value\n\ncontent");
        Ok(())
    }

    #[test]
    fn multi_attribute_conditional_in_header() -> Result<(), Error> {
        let input = "= Title
ifdef::backend-pdf,backend-docbook5[]
:title-page:
endif::backend-pdf,backend-docbook5[]

== Visible
Body";
        let result = Preprocessor::process(input, &Options::default(), Rc::default())?;
        assert_eq!(result.text, "= Title\n\n== Visible\nBody");
        Ok(())
    }

    #[test]
    fn active_multi_attribute_conditional_in_header() -> Result<(), Error> {
        let options = Options::builder()
            .with_attribute("backend-pdf", true)
            .build();
        let input = "= Title
ifdef::backend-pdf,backend-docbook5[]
:title-page:
endif::backend-pdf,backend-docbook5[]

== Visible
Body";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "= Title\n:title-page:\n\n== Visible\nBody");
        Ok(())
    }

    #[test]
    fn parser_accepts_multi_attribute_conditional_in_header() -> Result<(), Error> {
        let input = "= Title
ifdef::backend-pdf,backend-docbook5[]
:title-page:
endif::backend-pdf,backend-docbook5[]

== Visible
Body";
        let parsed = crate::parse(input, &Options::default())?;
        assert!(parsed.document().header.is_some());
        assert!(matches!(
            parsed.document().blocks.as_slice(),
            [crate::Block::Section(_)]
        ));
        Ok(())
    }

    #[test]
    fn inactive_conditional_body_can_contain_blank_lines() -> Result<(), Error> {
        let input = "= Title

ifdef::backend-pdf,backend-docbook5[]

== Hidden

Hidden body

endif::backend-pdf,backend-docbook5[]

== Visible
Visible body";
        let result = Preprocessor::process(input, &Options::default(), Rc::default())?;
        assert_eq!(result.text, "= Title\n\n\n== Visible\nVisible body");
        assert!(!result.text.contains("Hidden"));
        assert!(!result.text.contains("endif::"));
        Ok(())
    }

    #[test]
    fn nested_conditionals_follow_enclosing_activity() -> Result<(), Error> {
        let options = Options::builder().with_attribute("outer", true).build();
        let input = "ifdef::outer[]
outer content
ifdef::inner[]
hidden inner content
endif::inner[]
after inner
endif::outer[]
tail";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "outer content\nafter inner\ntail");
        Ok(())
    }

    #[test]
    fn inactive_enclosing_conditional_hides_active_nested_condition() -> Result<(), Error> {
        let options = Options::builder().with_attribute("inner", true).build();
        let input = "ifdef::outer[]
hidden outer content
ifdef::inner[]
hidden inner content
endif::inner[]
endif::outer[]
tail";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "tail");
        Ok(())
    }

    #[test]
    fn multi_attribute_conditions_evaluate_every_attribute() -> Result<(), Error> {
        let options = Options::builder()
            .with_attribute("third", true)
            .with_attribute("first", true)
            .with_attribute("second", true)
            .build();
        let input = "ifdef::missing-one,missing-two,third[]
or content
endif::missing-one,missing-two,third[]
ifdef::first+second+third[]
and content
endif::first+second+third[]";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "or content\nand content");
        Ok(())
    }

    #[test]
    fn attribute_defined_in_active_conditional_affects_later_condition() -> Result<(), Error> {
        let options = Options::builder().with_attribute("outer", true).build();
        let input = "ifdef::outer[]
:inner:
endif::outer[]
ifdef::inner[]
visible
endif::inner[]";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, ":inner:\nvisible");
        Ok(())
    }

    #[test]
    fn test_line_comment_adjacent_to_content_is_dropped() -> Result<(), Error> {
        let options = Options::default();
        let input = "para line one
// adjacent comment
para line two";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "para line one\npara line two");
        Ok(())
    }

    #[test]
    fn test_trailing_line_comments_after_content_are_dropped() -> Result<(), Error> {
        let options = Options::default();
        let input = "Usage-controlled: *NO*. +
// Trappable: *NO*. +
// Interruptible: *NO*. +";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "Usage-controlled: *NO*. +");
        Ok(())
    }

    #[test]
    fn test_standalone_line_comment_is_preserved() -> Result<(), Error> {
        let options = Options::default();
        let input = "para

// standalone comment

more";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "para\n\n// standalone comment\n\nmore");
        Ok(())
    }

    /// Preprocesses `input` as though read from `file`, then reports the source
    /// line `create_error_source_location` resolves for the first byte of
    /// `needle` in the preprocessed output — i.e. the line a warning anchored
    /// there would show the user. Returns `None` if preprocessing fails or the
    /// needle isn't present, which callers assert against explicitly.
    fn reported_source_line(input: &str, file: &str, needle: &str) -> Option<usize> {
        let result = Preprocessor::process_with_file(
            input,
            Path::new(file),
            &Options::default(),
            Rc::default(),
        )
        .ok()?;
        let text: &'static str = Box::leak(result.text.into_owned().into_boxed_str());
        resolve_warning_location(text, result.source_ranges, file, needle).map(|(_, line)| line)
    }

    /// Resolves the `(file, line)` `create_error_source_location` would report for
    /// the first byte of `needle` in the preprocessed `text`, given the
    /// preprocessor's `source_ranges` and the document's own `current_file`.
    fn resolve_warning_location(
        text: &'static str,
        source_ranges: Vec<SourceRange>,
        current_file: &str,
        needle: &str,
    ) -> Option<(Option<std::path::PathBuf>, usize)> {
        use crate::grammar::ParserState;

        let offset = text.find(needle)?;
        let mut state = ParserState::new_for_test(text);
        state.current_file = Some(std::path::PathBuf::from(current_file).into());
        state.source_ranges = source_ranges;

        let loc = state.create_location(offset, offset + needle.len());
        let resolved = state.create_error_source_location(loc);
        Some((resolved.file, resolved.location.start.line as usize))
    }

    #[test]
    fn dropped_comment_remaps_following_lines_to_source() {
        // The TODO item-L repro: a dropped adjacent comment must not shift the
        // reported line of everything after it.
        let input = "line one\n// c\nline two\n\nsecond para\n";
        assert_eq!(reported_source_line(input, "doc.adoc", "line one"), Some(1));
        assert_eq!(reported_source_line(input, "doc.adoc", "line two"), Some(3));
        assert_eq!(
            reported_source_line(input, "doc.adoc", "second para"),
            Some(5)
        );
    }

    #[test]
    fn stripped_conditional_remaps_following_lines_to_source() {
        // `cond` is unset, so the body is removed; `two` is source line 5.
        let input = "one\nifdef::cond[]\nhidden\nendif::[]\ntwo\n";
        assert_eq!(reported_source_line(input, "doc.adoc", "one"), Some(1));
        assert_eq!(reported_source_line(input, "doc.adoc", "two"), Some(5));
    }

    #[test]
    fn active_conditional_remaps_body_and_following_lines_to_source() {
        let input = ":cond:\nifdef::cond[]\ninside\nendif::[]\nafter\n";
        assert_eq!(reported_source_line(input, "doc.adoc", "inside"), Some(3));
        assert_eq!(reported_source_line(input, "doc.adoc", "after"), Some(5));
    }

    #[test]
    fn collapsed_attribute_continuation_remaps_following_lines_to_source() {
        // `:attr:` collapses two source lines into one; `baz` is source line 4.
        let input = ":attr: foo \\\nbar\n\nbaz\n";
        assert_eq!(reported_source_line(input, "doc.adoc", "baz"), Some(4));
    }

    #[test]
    fn no_path_input_records_ranges_with_none_file() -> Result<(), Error> {
        // stdin/string input has no path, but ranges are still recorded (with a
        // `None` file) so same-file line/offset remapping works for it too.
        let input = "one\n// c\ntwo\n";
        let result = Preprocessor::process(input, &Options::default(), Rc::default())?;
        assert!(!result.source_ranges.is_empty());
        assert!(result.source_ranges.iter().all(|r| r.file.is_none()));

        // `two` is source line 3 despite the dropped comment on line 2.
        let text: &'static str = Box::leak(result.text.clone().into_owned().into_boxed_str());
        let mapped = resolve_warning_location(text, result.source_ranges, "", "two");
        assert_eq!(mapped.map(|(_, line)| line), Some(3));
        Ok(())
    }

    #[test]
    fn include_reports_line_within_the_included_file() -> Result<(), Error> {
        // The main file drops an adjacent comment (line 4) before pulling in a
        // chapter via `include::`. A warning anchored in the chapter must name the
        // *chapter* file at its own line, and main-file content after the dropped
        // comment must still report its true source line.
        let path = Path::new("fixtures/preprocessor/include_line_mapping_main.adoc");
        let result = Preprocessor::process_file(path, &Options::default(), Rc::default())?;
        let text: &'static str = Box::leak(result.text.into_owned().into_boxed_str());

        // `target reference here` is line 5 of the included chapter.
        let chapter = resolve_warning_location(
            text,
            result.source_ranges.clone(),
            "fixtures/preprocessor/include_line_mapping_main.adoc",
            "target reference here",
        );
        assert_eq!(
            chapter
                .as_ref()
                .and_then(|(file, _)| file.as_ref())
                .and_then(|p| p.file_name())
                .and_then(std::ffi::OsStr::to_str),
            Some("include_line_mapping_chapter.adoc"),
        );
        assert_eq!(chapter.map(|(_, line)| line), Some(5));

        // Main-file content after the dropped comment is line 5 of the main file.
        let main = resolve_warning_location(
            text,
            result.source_ranges,
            "fixtures/preprocessor/include_line_mapping_main.adoc",
            "after the comment",
        );
        assert_eq!(
            main.as_ref()
                .and_then(|(file, _)| file.as_ref())
                .and_then(|p| p.file_name())
                .and_then(std::ffi::OsStr::to_str),
            Some("include_line_mapping_main.adoc"),
        );
        assert_eq!(main.map(|(_, line)| line), Some(5));
        Ok(())
    }

    /// Process `path`, then for the first byte of `needle` in the preprocessed
    /// output return the origin `(file name, source line, source byte offset)`
    /// the recorded ranges resolve — i.e. what an AST node anchored there reports
    /// after the remap. `None` if preprocessing fails or the needle is absent.
    fn resolve_partial_origin(path: &str, needle: &str) -> Option<(Option<String>, usize, usize)> {
        let result =
            Preprocessor::process_file(Path::new(path), &Options::default(), Rc::default()).ok()?;
        let text = result.text.into_owned();
        let offset = text.find(needle)?;
        let range = SourceRange::find_containing(&result.source_ranges, offset)?;
        let file = range
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_string);
        let line_map = LineMap::new(&text);
        Some((
            file,
            line_map.source_line(range, &text, offset) as usize,
            range.source_offset(offset),
        ))
    }

    #[test]
    fn partial_lines_include_maps_to_included_file_true_lines() {
        // `include::include_partial_part.adoc[lines=3..4]` pulls in lines 3-4 of the
        // part. Both must report the part file at their true origin line and byte
        // offset — not line 1 / offset 0.
        let main = "fixtures/preprocessor/include_partial_lines.adoc";

        // Part line 3 ("Line three.") starts at byte 20 of the part file.
        assert_eq!(
            resolve_partial_origin(main, "Line three."),
            Some((Some("include_partial_part.adoc".to_string()), 3, 20)),
        );
        // Part line 4 ("Line four.") starts at byte 32.
        assert_eq!(
            resolve_partial_origin(main, "Line four."),
            Some((Some("include_partial_part.adoc".to_string()), 4, 32)),
        );
    }

    #[test]
    fn partial_multi_range_include_splits_into_located_runs() {
        // `lines=1..1;4..5` is a non-contiguous selection: line 1, then lines 4-5.
        // Each surviving line keeps its true origin line/offset across the gap.
        let main = "fixtures/preprocessor/include_partial_multi.adoc";

        assert_eq!(
            resolve_partial_origin(main, "Line one."),
            Some((Some("include_partial_part.adoc".to_string()), 1, 0)),
        );
        assert_eq!(
            resolve_partial_origin(main, "Line four."),
            Some((Some("include_partial_part.adoc".to_string()), 4, 32)),
        );
        assert_eq!(
            resolve_partial_origin(main, "Line five."),
            Some((Some("include_partial_part.adoc".to_string()), 5, 43)),
        );
    }

    #[test]
    fn tag_include_maps_to_tag_region_source_lines() {
        // `include::tagged_content.adoc[tag=intro]` selects the intro region, whose
        // first line is line 4 of `tagged_content.adoc` (after the untagged opener,
        // a blank line, and the `// tag::intro[]` directive).
        let main = "fixtures/preprocessor/include_with_tag.adoc";
        let resolved = resolve_partial_origin(main, "This is the introduction.");
        assert_eq!(
            resolved.as_ref().and_then(|(file, _, _)| file.as_deref()),
            Some("tagged_content.adoc"),
        );
        assert_eq!(resolved.map(|(_, line, _)| line), Some(4));
    }

    #[test]
    fn test_line_comment_in_list_continuation_is_preserved() -> Result<(), Error> {
        let options = Options::default();
        let input = "* item
+
// a comment
+
more";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "* item\n+\n// a comment\n+\nmore");
        Ok(())
    }

    #[test]
    fn test_line_comment_after_title_is_preserved() -> Result<(), Error> {
        let options = Options::default();
        let input = "= The Title
// a comment
Roberto Avanzi";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "= The Title\n// a comment\nRoberto Avanzi");
        Ok(())
    }

    #[test]
    fn test_line_comment_in_verbatim_block_is_preserved() -> Result<(), Error> {
        let options = Options::default();
        let input = "----
code line
// not a comment here
----";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "----\ncode line\n// not a comment here\n----");
        Ok(())
    }

    #[test]
    fn test_line_comment_after_equals_run_dropped_without_setext() -> Result<(), Error> {
        // Without Setext, `=====` is ordinary content, so an adjacent comment
        // after it is dropped like any other mid-paragraph comment.
        let options = Options::default();
        let input = "Title
=====
// a comment
body";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "Title\n=====\nbody");
        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    fn test_line_comment_after_setext_underline_is_preserved() -> Result<(), Error> {
        // With Setext enabled, `=====` is a heading underline (a block
        // boundary), so a comment after it is preserved rather than absorbed.
        let options = Options::builder().with_setext().build();
        let input = "Title
=====
// a comment
body";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, "Title\n=====\n// a comment\nbody");
        Ok(())
    }

    #[test]
    fn test_good_endif_directive() -> Result<(), Error> {
        let options = Options::default();
        let input = ":asdf:

ifdef::asdf[]
content
endif::asdf[]";
        let result = Preprocessor::process(input, &options, Rc::default())?;
        assert_eq!(result.text, ":asdf:\n\ncontent");
        Ok(())
    }

    #[test]
    fn test_bad_endif_directive() {
        let options = Options::default();
        let input = "ifdef::asdf[]
content
endif::another[]";
        let output = Preprocessor::process(input, &options, Rc::default());
        assert!(matches!(
            output,
            Err(Error::InvalidConditionalDirective(..))
        ));
    }

    #[test]
    fn multi_attribute_endif_must_match_complete_condition() {
        let input = "ifdef::backend-pdf,backend-docbook5[]
content
endif::backend-pdf[]";
        let output = Preprocessor::process(input, &Options::default(), Rc::default());
        assert!(matches!(
            output,
            Err(Error::InvalidConditionalDirective(..))
        ));
    }

    #[test]
    fn test_utf8_bom_detection() -> Result<(), Error> {
        let path = Path::new("fixtures/preprocessor/utf8_bom.adoc");
        let content = read_and_decode_file(path, None)?;

        // Should contain the test content without BOM
        assert!(content.contains("= Test Document"));
        assert!(content.contains("This is a test with special chars: é, ñ, ü."));
        // BOM should be stripped
        assert!(!content.starts_with('\u{FEFF}'));
        Ok(())
    }

    #[test]
    fn test_utf16le_bom_detection() -> Result<(), Error> {
        let path = Path::new("fixtures/preprocessor/utf16le_bom.adoc");
        let content = read_and_decode_file(path, None)?;

        // Should correctly decode UTF-16 LE content
        assert!(content.contains("= Test Document"));
        assert!(content.contains("This is a test with special chars: é, ñ, ü."));
        Ok(())
    }

    #[test]
    fn test_utf16be_bom_detection() -> Result<(), Error> {
        let path = Path::new("fixtures/preprocessor/utf16be_bom.adoc");
        let content = read_and_decode_file(path, None)?;

        // Should correctly decode UTF-16 BE content
        assert!(content.contains("= Test Document"));
        assert!(content.contains("This is a test with special chars: é, ñ, ü."));
        Ok(())
    }

    #[test]
    fn test_utf8_no_bom() -> Result<(), Error> {
        let path = Path::new("fixtures/preprocessor/utf8_no_bom.adoc");
        let content = read_and_decode_file(path, None)?;

        // Should decode regular UTF-8 file
        assert!(content.contains("= Test Document"));
        assert!(content.contains("This is a test with special chars: é, ñ, ü."));
        Ok(())
    }

    #[test]
    fn test_explicit_encoding_override() -> Result<(), Error> {
        // Test that explicit encoding parameter works
        let path = Path::new("fixtures/preprocessor/utf8_no_bom.adoc");
        let content = read_and_decode_file(path, Some("utf-8"))?;

        assert!(content.contains("= Test Document"));
        Ok(())
    }

    #[test]
    fn test_unknown_encoding_error() {
        let path = Path::new("fixtures/preprocessor/utf8_no_bom.adoc");
        let result = read_and_decode_file(path, Some("unknown-encoding-12345"));

        assert!(matches!(result, Err(Error::UnknownEncoding(_))));
    }

    #[test]
    fn test_include_utf16_file() -> Result<(), Error> {
        // Test that include directive works with UTF-16 LE files
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/main_with_include.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain content from both main file and included UTF-16 file
        assert!(result.text.contains("= Main Document"));
        assert!(result.text.contains("This is included content."));
        assert!(result.text.contains("With special characters: é, ñ, ü."));
        assert!(result.text.contains("After include."));
        Ok(())
    }

    #[test]
    fn test_include_with_single_tag() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_with_tag.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain the intro tag content
        assert!(result.text.contains("This is the introduction."));
        assert!(result.text.contains("It has multiple lines."));
        // Should NOT contain other content
        assert!(!result.text.contains("untagged content"));
        assert!(!result.text.contains("main content"));
        assert!(!result.text.contains("Debug information"));
        // Should NOT contain tag directives
        assert!(!result.text.contains("tag::intro"));
        assert!(!result.text.contains("end::intro"));
        Ok(())
    }

    #[test]
    fn test_include_with_multiple_tags() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_multiple_tags.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain both intro and main content
        assert!(result.text.contains("This is the introduction."));
        assert!(result.text.contains("This is the main content."));
        // Should NOT contain debug or untagged content
        assert!(!result.text.contains("Debug information"));
        Ok(())
    }

    #[test]
    fn test_include_with_wildcard_excluding_tag() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_wildcard_exclude.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain intro and main content
        assert!(result.text.contains("This is the introduction."));
        assert!(result.text.contains("This is the main content."));
        // Should NOT contain debug content
        assert!(!result.text.contains("Debug information"));
        Ok(())
    }

    #[test]
    fn test_include_with_double_wildcard() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_double_wildcard.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain all content except tag directive lines
        assert!(result.text.contains("untagged content"));
        assert!(result.text.contains("This is the introduction."));
        assert!(result.text.contains("This is the main content."));
        assert!(result.text.contains("Debug information"));
        // Should NOT contain tag directives
        assert!(!result.text.contains("tag::intro"));
        assert!(!result.text.contains("end::intro"));
        Ok(())
    }

    #[test]
    fn test_include_with_nested_tag() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_nested_tag.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain only the nested content
        assert!(result.text.contains("This is nested within main."));
        // Should NOT contain main content outside nested
        assert!(!result.text.contains("This is the main content."));
        assert!(!result.text.contains("Back to main content."));
        Ok(())
    }

    #[test]
    fn test_include_select_untagged_only() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_untagged_only.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain only untagged content
        assert!(result.text.contains("untagged content at the beginning"));
        assert!(result.text.contains("More untagged content"));
        assert!(result.text.contains("Final untagged content"));
        // Should NOT contain any tagged content
        assert!(!result.text.contains("This is the introduction"));
        assert!(!result.text.contains("This is the main content"));
        assert!(!result.text.contains("Debug information"));
        Ok(())
    }

    #[test]
    fn test_include_tag_with_lines() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_tag_with_lines.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // When combining tag= and lines=, the lines= attribute refers to
        // line numbers in the ORIGINAL file, not the filtered result.
        // tag=intro selects lines 4-5 (content between tag directives)
        // lines=4 selects only line 4 from the original file
        // The intersection is just line 4: "This is the introduction."
        assert!(result.text.contains("This is the introduction."));
        // Line 5 is not in lines=4, so it should NOT be included
        assert!(!result.text.contains("It has multiple lines."));
        Ok(())
    }

    #[test]
    fn test_include_with_indent() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_with_indent.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // min indent is 0, so indent=4 adds 4 spaces preserving relative indentation
        assert!(result.text.contains("    def hello"));
        assert!(result.text.contains("      puts \"Hello\""));
        assert!(result.text.contains("    end"));
        Ok(())
    }

    #[test]
    fn test_include_with_indent_zero() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_with_indent_zero.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // min indent is 0, so indent=0 leaves content unchanged
        assert!(result.text.contains("def hello"));
        assert!(result.text.contains("  puts \"Hello\""));
        assert!(result.text.contains("end"));
        Ok(())
    }

    #[test]
    fn test_include_with_indent_and_tag() -> Result<(), Error> {
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/include_with_indent_and_tag.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain intro tag content, indented by 2 spaces
        assert!(result.text.contains("  This is the introduction."));
        assert!(result.text.contains("  It has multiple lines."));
        // Should NOT contain other content
        assert!(!result.text.contains("main content"));
        assert!(!result.text.contains("Debug information"));
        Ok(())
    }

    #[test]
    fn test_nested_include_relative_paths() -> Result<(), Error> {
        // Tests that nested includes resolve paths relative to their parent file.
        // Structure:
        //   nested_include_main.adoc
        //     -> includes subdir/middle.adoc
        //         -> includes inner.adoc (relative to subdir/)
        let warnings = Rc::<RefCell<Vec<Warning>>>::default();
        let path = Path::new("fixtures/preprocessor/nested_include_main.adoc");
        let options = Options::default();

        let result = Preprocessor::process_file(path, &options, warnings)?;

        // Should contain content from main file
        assert!(result.text.contains("= Nested Include Test"));
        // Should contain content from subdir/middle.adoc
        assert!(result.text.contains("This is middle content."));
        // Should contain content from subdir/inner.adoc (resolved relative to subdir/)
        assert!(
            result.text.contains("This is inner content from subdir."),
            "Nested include failed to resolve relative path. Got: {}",
            result.text
        );
        Ok(())
    }
}
