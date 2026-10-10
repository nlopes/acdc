//! Read an include source in chunks and select its text.
//!
//! A small selection can come from a source larger than the text limit.
//! Keep only possible selected text, within the limit. Track the original line
//! numbers and byte offsets for diagnostics and source locations.
//!
//! The caller checks access and selects the applicable line or tag selector before
//! it calls [`read`]. It applies indentation and processes nested directives after
//! this module returns.
//!
//! The reader has three stages:
//!
//! - `decode` keeps the same decoder across chunks. It stops when the selector
//!   finishes, even if the rest of the chunk contains invalid bytes.
//! - `Selection` joins decoded chunks into lines and applies the selector.
//! - `Candidates` stores possible selected lines within the size limit. It returns
//!   the selected lines in source order, with their original locations.
//!
//! A finite `lines=` selection stops at its highest requested line.
//! Each line loses its trailing whitespace before selection. Text after the last
//! requested line cannot change the result or cause an error.
//! If a range ends beyond the end of the source, that range selects nothing.
//! We keep possible selected lines until we know whether the source reaches the
//! required line. For example, `lines=10..20` needs at least 20 source lines.
//!
//! Full includes, open-ended ranges, and tag selections read to the end of the source.
//! A tag can occur again later. Full and tagged includes keep the existing
//! normalization rules. These rules can depend on later whitespace, so we keep
//! both the original text and the text without trailing whitespace.
//!
//! A read can return a limited number of bytes after the requested line.
//! These bytes count toward the HTTP transfer limit. When a finite selection is
//! complete, we close the response without reading the rest.
//! The per-source text limit does not limit scan time. The caller checks the
//! shared include budget before it processes nested directives.

use std::{
    collections::BinaryHeap,
    io::{self, Cursor, Read},
    ops::ControlFlow,
};

use encoding_rs::{DecoderResult, Encoding, UTF_8, UTF_16LE};

use super::{ContentSelection, LinesRange};
use crate::{
    Error, IncludeSource,
    preprocessor::{SourceLineOrigin, tag},
};

pub(super) const MAX_SELECTED_BYTES: usize = 10 * 1024 * 1024;
const MAX_TAG_METADATA_BYTES: usize = 10 * 1024 * 1024;

/// Selected text after normalization, with source locations and no added indentation.
pub(super) struct Selected {
    pub text: String,
    /// One source location for each selected line, including empty lines.
    /// If `text` is empty, this vector distinguishes no lines from one empty line.
    pub origins: Vec<SourceLineOrigin>,
    /// The caller adds directive locations after a successful read.
    /// If the read fails, it discards these issues with the partial text.
    pub issues: Vec<tag::Issue>,
}

pub(super) fn read(
    source: IncludeSource,
    encoding: Option<&str>,
    name: &str,
    selection: &ContentSelection,
) -> Result<Selected, Error> {
    let mut selected = Selection::new(selection, name);
    if selected.stop_after != Some(0) {
        decode(source, encoding, name, |text| selected.feed(text))?;
    }
    selected.finish()
}

/// Read only bytes that can be part of a byte order mark (BOM).
/// An unconditional three-byte read would pass the end of a short line such as "a\n".
fn bom_prefix(reader: &mut dyn Read) -> io::Result<Vec<u8>> {
    let mut prefix = Vec::with_capacity(3);
    let mut next = [0];
    loop {
        match reader.read(&mut next) {
            Ok(0) => break,
            Ok(_) => {
                let [byte] = next;
                prefix.push(byte);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
        if !matches!(prefix.as_slice(), [0xef | 0xff | 0xfe] | [0xef, 0xbb]) {
            break;
        }
    }
    Ok(prefix)
}

fn decode(
    source: IncludeSource,
    label: Option<&str>,
    name: &str,
    mut consume: impl FnMut(&str) -> Result<ControlFlow<()>, Error>,
) -> Result<(), Error> {
    #[cfg(feature = "network")]
    let read_limit = source.read_limit;
    let mut reader = source.into_reader();
    let prefix = bom_prefix(reader.as_mut())?;
    let explicit = label.and_then(|label| Encoding::for_label(label.as_bytes()));
    if explicit == Some(UTF_16LE)
        && label.is_some_and(|s| s.eq_ignore_ascii_case("utf-16"))
        && !prefix.starts_with(&[0xff, 0xfe])
        && !prefix.starts_with(&[0xfe, 0xff])
    {
        return Err(Error::UnrecognizedEncodingInFile(name.to_string()));
    }
    // Match `decode_bytes` when no encoding is specified: remove the first BOM.
    // The decoder then handles any repeated BOM.
    let (encoding, skip) = explicit.map_or_else(
        || Encoding::for_bom(&prefix).unwrap_or((UTF_8, 0)),
        |encoding| (encoding, 0),
    );
    let mut decoder = encoding.new_decoder();
    // Count the removed BOM bytes toward the HTTP transfer limit.
    #[cfg(feature = "network")]
    let mut total = skip;
    let mut reader = Cursor::new(prefix.into_iter().skip(skip).collect::<Vec<_>>()).chain(reader);
    let mut input = [0; 8192];
    let mut output = [0; 8192];
    loop {
        #[cfg(not(feature = "network"))]
        let buffer = input.as_mut_slice();
        #[cfg(feature = "network")]
        let buffer = {
            // Limit the read size to the remaining HTTP transfer limit.
            // At the limit, read one more byte to check for excess data.
            let available = read_limit.map_or(input.len(), |limit| {
                input.len().min(limit.saturating_sub(total).max(1))
            });
            input.get_mut(..available).unwrap_or_default()
        };
        let count = match reader.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        #[cfg(feature = "network")]
        {
            // Count decompressed response bytes before character decoding.
            // The HTTP transfer limit also applies to bytes outside the selection.
            total = total.saturating_add(count);
            if read_limit.is_some_and(|limit| total > limit) {
                return Err(Error::HttpRequest(
                    "remote include response exceeds the 10 MiB transfer limit".to_string(),
                ));
            }
        }
        let last = count == 0;
        let mut bytes = input.get(..count).unwrap_or_default();
        // The output buffer can fill before the decoder finishes the input chunk.
        // Keep the same decoder: one character can span two reads.
        // At the end of the source, `last` makes the decoder detect incomplete characters.
        loop {
            let (result, consumed, written) =
                decoder.decode_to_utf8_without_replacement(bytes, &mut output, last);
            let text = std::str::from_utf8(output.get(..written).unwrap_or_default())
                .map_err(|_| Error::UnrecognizedEncodingInFile(name.to_string()))?;
            // The valid text can complete the selection before invalid bytes
            // in the same chunk. Ignore these bytes after the selection ends.
            if consume(text)?.is_break() {
                return Ok(());
            }
            if matches!(result, DecoderResult::Malformed(..)) {
                return Err(Error::UnrecognizedEncodingInFile(name.to_string()));
            }
            bytes = bytes.get(consumed..).unwrap_or_default();
            if result == DecoderResult::InputEmpty {
                break;
            }
        }
        if last {
            return Ok(());
        }
    }
}

/// Possible selected lines for one form of whitespace normalization.
#[derive(Default)]
struct Candidates {
    lines: BinaryHeap<Retained>,
    // Count one newline for each line, including empty lines.
    // The final text omits the last newline, so this count can exceed
    // the size limit by one byte.
    bytes: usize,
    // Minimum source length, in lines, that makes discarded text part of
    // the selection. At this length, the selected text exceeds the size limit.
    // If the source is shorter, the discarded text was outside the selection.
    first_discarded_requirement: Option<usize>,
}

/// A possible selected line and the minimum source length that makes it valid.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct Retained {
    // Keep this field first in the derived ordering. The max-heap removes
    // lines that need the longest source first. Lines that are valid in
    // a shorter source remain available.
    required_end: usize,
    number: usize,
    offset: usize,
    text: String,
}

impl Candidates {
    fn discard(&mut self, required: usize) {
        self.first_discarded_requirement = Some(
            self.first_discarded_requirement
                .map_or(required, |old| old.min(required)),
        );
    }

    fn push(
        &mut self,
        line: &Line,
        length: usize,
        required_end: usize,
        number: usize,
        offset: usize,
    ) {
        if length > MAX_SELECTED_BYTES {
            self.discard(required_end);
            return;
        }
        let text = line.prefix.get(..length).unwrap_or_default().to_string();
        self.bytes += text.len() + 1;
        self.lines.push(Retained {
            required_end,
            number,
            offset,
            text,
        });
        // A removed line can belong to a range that ends beyond the source.
        // Record the source length that would make this line part of the selection.
        while self.bytes > MAX_SELECTED_BYTES + 1 {
            if let Some(line) = self.lines.pop() {
                self.bytes -= line.text.len() + 1;
                self.discard(line.required_end);
            }
        }
    }

    fn oversized(&self, total_lines: usize) -> bool {
        self.first_discarded_requirement
            .is_some_and(|required| required <= total_lines)
    }

    fn finish(self, total_lines: usize, name: &str) -> Result<Selected, Error> {
        if self.oversized(total_lines) {
            return Err(Error::IncludeSourceTooLarge(name.to_string()));
        }
        let mut lines = self.lines.into_vec();
        lines.retain(|line| line.required_end <= total_lines);
        // The heap keeps stored text within the limit. Sort the output by source line.
        lines.sort_unstable_by_key(|line| line.number);
        let mut text = String::with_capacity(self.bytes.saturating_sub(1));
        let mut origins = Vec::with_capacity(lines.len());
        for line in lines {
            if !origins.is_empty() {
                text.push('\n');
            }
            text.push_str(&line.text);
            origins.push(SourceLineOrigin {
                line: line.number,
                offset: line.offset,
                column_shift: 0,
            });
        }
        Ok(Selected {
            text,
            origins,
            issues: Vec::new(),
        })
    }
}

/// Line and selection state that continues across decoded chunks.
struct Selection<'a> {
    selector: &'a ContentSelection,
    name: &'a str,
    // `None` requires the end of the source. `Some(0)` means no valid line requests.
    stop_after: Option<usize>,
    tags: Option<tag::Selector<'a>>,
    // Full and tagged includes follow `Preprocessor::normalize`.
    // Later whitespace can change how earlier lines are trimmed. Keep both
    // forms within the limit until `rebuild` is known. Line selections always
    // use `trimmed`, so unread text cannot change their text or offsets.
    raw: Candidates,
    trimmed: Candidates,
    rebuild: bool,
    line: Option<Line>,
    // Full and tagged includes delay the last line to match `normalize(...).lines()`.
    // Line selections accept each line at its newline, including blank lines.
    pending: Option<Line>,
    // Count only accepted source lines. The current or pending line is `number + 1`.
    number: usize,
    raw_offset: usize,
    trimmed_offset: usize,
    issues: Vec<tag::Issue>,
    checked_issues: usize,
    issue_bytes: usize,
}

impl<'a> Selection<'a> {
    fn new(selection: &'a ContentSelection, name: &'a str) -> Self {
        Self {
            selector: selection,
            name,
            stop_after: Self::last_requested_line(selection),
            tags: if let ContentSelection::Tags(filters) = selection {
                Some(tag::Selector::new(filters))
            } else {
                None
            },
            raw: Candidates::default(),
            trimmed: Candidates::default(),
            rebuild: false,
            line: None,
            pending: None,
            number: 0,
            raw_offset: 0,
            trimmed_offset: 0,
            issues: Vec::new(),
            checked_issues: 0,
            issue_bytes: 0,
        }
    }

    /// Stop a finite selection at its highest requested line.
    /// Ignore invalid ranges. Valid open-ended ranges need the end of the source.
    fn last_requested_line(selection: &ContentSelection) -> Option<usize> {
        let ContentSelection::Lines(ranges) = selection else {
            return None;
        };
        ranges.iter().try_fold(0, |last, range| {
            let end = match *range {
                LinesRange::Single(line) => line,
                LinesRange::Range(0, _) => 0,
                LinesRange::Range(_, end) if end < 0 => return None,
                LinesRange::Range(start, end) => usize::try_from(end)
                    .ok()
                    .filter(|end| *end >= start)
                    .unwrap_or(0),
            };
            Some(last.max(end))
        })
    }

    /// Minimum source length, in lines, that makes the next line part of the selection.
    ///
    /// For `lines=10..20`, the source must have at least 20 lines.
    /// A single line or an open-ended range only needs the source to reach that line.
    ///
    /// If ranges overlap, use the lowest requirement for each line.
    /// For example, `lines=1..100;2` selects line 2 from a source with only two lines.
    /// It selects line 1 only if the source has at least 100 lines.
    fn requirement(&self) -> Option<usize> {
        let number = self.number + 1;
        match self.selector {
            ContentSelection::All => Some(number),
            ContentSelection::Tags(_) => self
                .tags
                .as_ref()
                .is_some_and(tag::Selector::selected)
                .then_some(number),
            ContentSelection::Lines(ranges) => ranges
                .iter()
                .filter_map(|range| match *range {
                    LinesRange::Single(line) => (line == number).then_some(number),
                    LinesRange::Range(start, end) if start > 0 && number >= start => {
                        if end < 0 {
                            Some(number)
                        } else {
                            usize::try_from(end).ok().filter(|end| *end >= number)
                        }
                    }
                    LinesRange::Range(_, _) => None,
                })
                .min(),
        }
    }

    fn feed(&mut self, text: &str) -> Result<ControlFlow<()>, Error> {
        for ch in text.chars() {
            if self.line.is_none() {
                if let Some(line) = self.pending.take() {
                    self.accept(&line)?;
                }
                self.line = Some(Line::new(self.requirement(), self.tags.is_some()));
            }
            if ch == '\n' {
                if let Some(mut line) = self.line.take() {
                    line.remove_cr();
                    self.rebuild |= line.needs_rebuild();
                    if matches!(self.selector, ContentSelection::Lines(_)) {
                        // The newline completes this line, even if it is blank.
                        // We do not need to read again to check for the end of the source.
                        self.accept(&line)?;
                        if self.stop_after == Some(self.number) {
                            return Ok(ControlFlow::Break(()));
                        }
                    } else {
                        self.pending = Some(line);
                    }
                }
            } else if let Some(line) = &mut self.line {
                line.push(ch);
                // Fail before the newline only if this line must be selected.
                // A finite range can still end beyond the source. A tagged line can
                // still be a marker, which is excluded from the output.
                if line.trimmed_len > MAX_SELECTED_BYTES
                    && line.required_end.is_some_and(|end| end <= self.number + 1)
                    && self.tags.is_none()
                {
                    return Err(Error::IncludeSourceTooLarge(self.name.to_string()));
                }
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    fn accept(&mut self, line: &Line) -> Result<(), Error> {
        let required = self.requirement();
        self.rebuild |= line.crlf;
        self.number += 1;
        let marker = line.marker(self.name)?;
        // Process markers before the next line: they control its selection.
        // Marker lines are excluded from the selected text.
        if let (Some(tags), Some((directive, name))) = (&mut self.tags, marker) {
            tags.marker(directive, name, self.number, |issue| {
                self.issues.push(issue);
            });
            if tags.stack_bytes() > MAX_TAG_METADATA_BYTES {
                return Err(self.metadata_error());
            }
            self.check_issues()?;
        } else if let Some(required) = required {
            if !matches!(self.selector, ContentSelection::Lines(_)) {
                self.raw
                    .push(line, line.len, required, self.number, self.raw_offset);
            }
            self.trimmed.push(
                line,
                line.trimmed_len,
                required,
                self.number,
                self.trimmed_offset,
            );
            // The trimmed form is the smallest possible result. If it exceeds
            // the size limit, later normalization cannot make the selection fit.
            if self.trimmed.oversized(self.number) {
                return Err(Error::IncludeSourceTooLarge(self.name.to_string()));
            }
        }
        // Count skipped text and markers too. Source locations refer to
        // byte offsets in the full normalized source.
        self.raw_offset = self.raw_offset.saturating_add(line.len).saturating_add(1);
        self.trimmed_offset = self
            .trimmed_offset
            .saturating_add(line.trimmed_len)
            .saturating_add(1);
        Ok(())
    }

    fn metadata_error(&self) -> Error {
        Error::IncludeSelectionTooComplex(self.name.to_string())
    }

    /// Limit tag metadata even when no text is selected.
    fn check_issues(&mut self) -> Result<(), Error> {
        // Count only new issues. Counting all previous issues at each marker
        // would make the checks take quadratic time.
        self.issue_bytes += self.issues.iter().skip(self.checked_issues).map(|issue| match issue {
            tag::Issue::UnexpectedEnd { name, .. } | tag::Issue::Unclosed { name, .. } => name.len(),
            tag::Issue::MismatchedEnd { expected, found, .. } => expected.len() + found.len(),
            tag::Issue::Missing { names } => names.iter().map(String::len).sum(),
        } + std::mem::size_of::<tag::Issue>()).sum::<usize>();
        self.checked_issues = self.issues.len();
        if self.issue_bytes + self.tags.as_ref().map_or(0, tag::Selector::stack_bytes)
            > MAX_TAG_METADATA_BYTES
        {
            return Err(self.metadata_error());
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Selected, Error> {
        if let Some(line) = self.line.take() {
            self.rebuild |= line.needs_rebuild();
            self.pending = Some(line);
        }
        if let Some(mut line) = self.pending.take() {
            // `normalize` removes a final CRLF before it checks for CR.
            // Only a CRLF before the last line should set `rebuild` in `accept`.
            line.crlf = false;
            // A line selection keeps a final line without a newline, even if
            // trimming makes it empty. Other selectors match `normalize(...).lines()`:
            // "a\n\n" becomes "a\n", which `.lines()` reads as one line, "a".
            if matches!(self.selector, ContentSelection::Lines(_))
                || if self.rebuild {
                    line.trimmed_len > 0
                } else {
                    line.len > 0
                }
            {
                self.accept(&line)?;
            }
        }
        if let Some(tags) = self.tags.take() {
            tags.finish(|issue| self.issues.push(issue));
            self.check_issues()?;
        }
        let candidates = if self.rebuild || matches!(self.selector, ContentSelection::Lines(_)) {
            self.trimmed
        } else {
            self.raw
        };
        let mut selected = candidates.finish(self.number, self.name)?;
        selected.issues = self.issues;
        Ok(selected)
    }
}

/// The full decoded line length, with limited storage for possible selected text.
///
/// Keep counting bytes after storage stops. This preserves source offsets for
/// large skipped lines. If only trailing whitespace exceeds the limit, the stored
/// text can still contain the full trimmed line.
struct Line {
    prefix: String,
    required_end: Option<usize>,
    len: usize,
    trimmed_len: usize,
    last: Option<char>,
    previous: Option<char>,
    carriage_returns: usize,
    crlf: bool,
    probes: Option<[Probe; 2]>,
}

impl Line {
    fn new(required_end: Option<usize>, tags: bool) -> Self {
        Self {
            prefix: String::new(),
            required_end,
            len: 0,
            trimmed_len: 0,
            last: None,
            previous: None,
            carriage_returns: 0,
            crlf: false,
            probes: tags.then(|| [Probe::new("tag::"), Probe::new("end::")]),
        }
    }

    fn push(&mut self, ch: char) {
        self.len = self.len.saturating_add(ch.len_utf8());
        if !ch.is_whitespace() {
            self.trimmed_len = self.len;
        }
        if self.required_end.is_some() && self.len <= MAX_SELECTED_BYTES {
            self.prefix.push(ch);
        }
        self.previous = self.last.replace(ch);
        self.carriage_returns += usize::from(ch == '\r');
        if let Some(probes) = &mut self.probes {
            for probe in probes {
                probe.push(ch);
            }
        }
    }

    /// Treat CRLF as one line ending. Keep its effect on normalization.
    fn remove_cr(&mut self) {
        if self.last == Some('\r') {
            if self.prefix.len() == self.len {
                self.prefix.pop();
            }
            self.len -= 1;
            self.carriage_returns -= 1;
            self.last = self.previous;
            self.crlf = true;
        }
    }

    fn needs_rebuild(&self) -> bool {
        self.carriage_returns > 0 || matches!(self.last, Some(' ' | '\t'))
    }

    fn marker(&self, source: &str) -> Result<Option<(&'static str, &str)>, Error> {
        if let Some(probes) = &self.probes {
            // Match the buffered tag extractor: a valid `tag::` marker takes
            // priority over `end::`, even if `end::` comes first on the line.
            for probe in probes {
                if matches!(probe.state, ProbeState::Done(true)) {
                    if probe.overflow {
                        return Err(Error::IncludeSelectionTooComplex(source.to_string()));
                    }
                    return Ok(Some((
                        if probe.keyword == "tag::" {
                            "tag"
                        } else {
                            "end"
                        },
                        &probe.name,
                    )));
                }
            }
        }
        Ok(None)
    }
}

/// Find a tag marker without storing the surrounding line.
///
/// Each probe checks the first occurrence of one keyword.
/// If that occurrence is invalid, the probe stops. This matches the buffered
/// tag extractor, which does not try later occurrences of the same keyword.
/// Separate probes for `tag::` and `end::` preserve their priority without storing
/// all the text before a marker.
struct Probe {
    keyword: &'static str,
    matched: usize,
    previous: Option<char>,
    word_before: bool,
    state: ProbeState,
    // Keep checking syntax after the name buffer is full.
    // An overlong valid marker causes an error. An invalid marker remains plain text.
    overflow: bool,
    name: String,
}

enum ProbeState {
    Searching,
    Name,
    /// The probe found `[` after a possible name. Only `]` can complete the marker.
    Bracket,
    /// Whether the first keyword occurrence forms a valid marker.
    Done(bool),
}

impl Probe {
    fn new(keyword: &'static str) -> Self {
        Self {
            keyword,
            matched: 0,
            previous: None,
            word_before: false,
            state: ProbeState::Searching,
            overflow: false,
            name: String::new(),
        }
    }

    fn push(&mut self, ch: char) {
        let previous = self.previous.replace(ch);
        match self.state {
            ProbeState::Done(_) => (),
            ProbeState::Searching => {
                let expected = self
                    .keyword
                    .as_bytes()
                    .get(self.matched)
                    .copied()
                    .map(char::from);
                if expected == Some(ch) {
                    if self.matched == 0 {
                        self.word_before =
                            previous.is_some_and(|c| c.is_alphanumeric() || c == '_');
                    }
                    self.matched += 1;
                    if self.matched == self.keyword.len() {
                        self.state = if self.word_before {
                            ProbeState::Done(false)
                        } else {
                            ProbeState::Name
                        };
                    }
                } else {
                    self.matched = usize::from(self.keyword.starts_with(ch));
                    self.word_before = previous.is_some_and(|c| c.is_alphanumeric() || c == '_');
                }
            }
            ProbeState::Bracket => {
                self.state =
                    ProbeState::Done(ch == ']' && (!self.name.is_empty() || self.overflow));
            }
            ProbeState::Name => {
                if ch == '[' {
                    self.state = ProbeState::Bracket;
                } else if ch == ']' || ch.is_whitespace() {
                    self.state = ProbeState::Done(false);
                } else if self.name.len() + ch.len_utf8() <= MAX_TAG_METADATA_BYTES {
                    self.name.push(ch);
                } else {
                    self.overflow = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
