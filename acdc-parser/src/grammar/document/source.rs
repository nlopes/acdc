//! Cooperate with the reader between committed construction steps.

use std::{borrow::Cow, cell::RefCell, rc::Rc};

use crate::{
    AttributeName, Block, Document, DocumentAttributeAssignment, DocumentAttributes, Error,
    document_attribute::{AttributeDeclaration, RawAttributeValue},
    grammar::{
        LineMap, ParserState,
        source::{SourceIndex, SourceView},
    },
    preprocessor::{PreprocessorResult, SourceReader},
};

use super::{
    construction::ConstructionMode,
    coordinator::{BlockCursor, Execution, document_with_execution},
    delimited::DelimitedKind,
    document_parser,
    input::{BlockInput, ReadRequest},
    lists::{ListKind, ListPhase, OrdinaryListKind},
};

pub(crate) type Assignments<'a> = Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>;
#[derive(Default)]
struct SourceUpdates<'a> {
    assignments: RefCell<Assignments<'static>>,
    nested_parent: RefCell<Option<Rc<DocumentAttributes<'a>>>>,
}

type PendingAssignments<'a> = Rc<SourceUpdates<'a>>;

/// Document-local execution after recognition has committed a block.
pub(crate) trait SourceHandler {
    fn accepted<'a>(
        &mut self,
        _block: &Block<'a>,
        _attributes: &DocumentAttributes<'a>,
    ) -> Result<Assignments<'a>, String> {
        Ok(Vec::new())
    }

    fn construct<'a>(
        &mut self,
        block: &mut Block<'a>,
        state: &mut ParserState<'a>,
    ) -> Result<(bool, Assignments<'a>), Error> {
        self.accepted(block, &state.document_attributes)
            .map(|assignments| (true, assignments))
            .map_err(|message| {
                Error::Extension(
                    Box::new(state.create_error_source_location(block.location().clone())),
                    message,
                )
            })
    }
}

#[derive(Clone, Copy)]
pub(super) enum SourceLine<'a> {
    Blank,
    Attribute,
    Metadata,
    Comment,
    Heading,
    Delimited {
        delimiter: &'a str,
        kind: Option<DelimitedKind>,
        table: bool,
        comment: bool,
    },
    List {
        kind: ListKind<'a>,
        marker: &'a str,
        empty: bool,
    },
    Macro,
    Markdown,
    Quoted,
    Continuation,
    Text,
}

pub(super) fn ordinary_kind(ordered: bool) -> ListKind<'static> {
    let kind = OrdinaryListKind {
        parent_other: None,
        parent_same: None,
        allow_continuation: true,
        has_own_metadata: true,
    };
    if ordered {
        ListKind::Ordered(kind)
    } else {
        ListKind::Unordered(kind)
    }
}

struct SourceStart<'a> {
    offset: usize,
    line: &'a str,
    form: SourceLine<'a>,
    verbatim: bool,
    compound: bool,
}

struct CellRead<'a> {
    separator: char,
    delimiter: &'a str,
    start: usize,
    end: Option<usize>,
}

pub(super) struct SourceInput<'a> {
    reader: Option<SourceReader<'a>>,
    pub(super) index: Rc<SourceIndex<'a>>,
    pub(super) result: Option<PreprocessorResult<'a>>,
    updates: PendingAssignments<'a>,
    failed: bool,
    stream_open: bool,
    boundaries: Vec<&'a str>,
    cells: Vec<CellRead<'a>>,
    stream_table: bool,
}

impl<'a> SourceInput<'a> {
    fn new(
        reader: SourceReader<'a>,
        updates: PendingAssignments<'a>,
        index: Rc<SourceIndex<'a>>,
    ) -> Self {
        Self {
            reader: Some(reader),
            index,
            result: None,
            updates,
            failed: false,
            stream_open: false,
            boundaries: Vec::new(),
            cells: Vec::new(),
            stream_table: false,
        }
    }

    fn available_end(&self) -> usize {
        self.cells
            .last()
            .and_then(|cell| cell.end)
            .map_or(self.index.len(), |end| end.min(self.index.len()))
    }

    pub(super) fn line(
        &mut self,
        offset: usize,
        state: &mut ParserState<'a>,
    ) -> Result<Option<&'a str>, Error> {
        if self
            .cells
            .last()
            .and_then(|cell| cell.end)
            .is_some_and(|end| offset >= end)
        {
            return Ok(None);
        }
        let Some(line) = self.raw_line(offset, state)? else {
            return Ok(None);
        };
        if let Some(cell) = self.cells.last_mut() {
            let boundary = if document_parser::source_block_close(line, state, cell.delimiter, true)
                .is_ok()
            {
                Some(0)
            } else {
                crate::blocks::table::next_cell_boundary(line, cell.separator, offset != cell.start)
                    .map(|(end, _, _)| end)
            };
            if let Some(end) = boundary {
                cell.end = Some(offset + end);
                return Ok((end > 0).then(|| &line[..end]));
            }
        }
        Ok(Some(line))
    }

    pub(super) fn raw_line(
        &mut self,
        offset: usize,
        state: &mut ParserState<'a>,
    ) -> Result<Option<&'a str>, Error> {
        let result = self.read_line(offset, state);
        self.failed |= result.is_err();
        result
    }

    fn read_line(
        &mut self,
        offset: usize,
        state: &mut ParserState<'a>,
    ) -> Result<Option<&'a str>, Error> {
        loop {
            if offset < self.index.len() {
                let tail = state
                    .source_tail(offset)
                    .filter(|tail| !tail.is_empty())
                    .or_else(|| self.index.tail(offset))
                    .ok_or_else(|| source_error(state, offset, "missing emitted source line"))?;
                let end = tail.find('\n').map_or(tail.len(), |end| end + 1);
                return Ok(tail.get(..end));
            }
            let Some(reader) = self.reader.as_mut() else {
                return Ok(None);
            };
            for (name, assignment) in self.updates.assignments.borrow_mut().drain(..) {
                let value = match assignment {
                    DocumentAttributeAssignment::Set(value) => RawAttributeValue::Resolved(value),
                    DocumentAttributeAssignment::Unset => RawAttributeValue::Unset,
                };
                reader
                    .options_mut()
                    .document_attributes
                    .assign_document_value(name, value, false, false, None)?;
            }
            if let Some(chunk) = reader.next()? {
                if chunk.start != self.index.len() {
                    return Err(source_error(
                        state,
                        offset,
                        "reader emitted noncontiguous source",
                    ));
                }
                self.index.append(chunk.text);
                state.source_ranges.extend(chunk.source_ranges);
                state.leveloffset_ranges.extend(chunk.leveloffset_ranges);
                state.leveloffset_scopes.extend(chunk.leveloffset_scopes);
            } else {
                let reader = self
                    .reader
                    .take()
                    .ok_or_else(|| source_error(state, offset, "missing source reader at EOF"))?;
                let result = reader.finish()?;
                self.index.seal(result.text.len());
                self.result = Some(result);
            }
        }
    }

    fn kind(line: &'a str, state: &mut ParserState<'a>) -> SourceLine<'a> {
        // Classification is read-only and shares the document grammar's raw
        // recognizers. Only the accepted construction pass builds inline nodes.
        if line == "\n" {
            return SourceLine::Blank;
        }
        // Alphabetic lines can introduce a macro or a description item. All
        // other block openers start with punctuation, digits, or whitespace.
        // These reject filters avoid starting PEG for ordinary prose lines.
        if is_prose_line(line) {
            return SourceLine::Text;
        }
        document_parser::source_line(line, state).unwrap_or(SourceLine::Text)
    }

    fn scan(
        &mut self,
        start: usize,
        request: ReadRequest<'a>,
        state: &mut ParserState<'a>,
    ) -> Result<usize, Error> {
        self.stream_open = false;
        self.stream_table = false;
        let Some(unit) = self.block_start(start, request, state)? else {
            return Ok(self.available_end());
        };
        let offset = unit.offset + unit.line.len();
        if self.boundaries.last().is_some_and(|delimiter| {
            document_parser::source_block_close(unit.line, state, delimiter, false).is_ok()
        }) {
            return Ok(offset);
        }
        if let SourceLine::Delimited {
            delimiter, table, ..
        } = unit.form
            && self.index.tail(offset).is_some_and(|tail| {
                tail.split_inclusive('\n').any(|line| {
                    document_parser::source_block_close(line, state, delimiter, table).is_ok()
                })
            })
        {
            return self.delimited_end(offset, delimiter, table, state);
        }
        if state.options.extensions.is_some()
            && matches!(unit.form, SourceLine::Delimited { table: true, delimiter, .. } if delimiter.starts_with(['|', '!']))
        {
            self.stream_table = true;
            return Ok(offset);
        }
        if state.options.extensions.is_some() && unit.compound {
            self.stream_open = true;
            return Ok(offset);
        }
        if let SourceLine::Delimited {
            delimiter, table, ..
        } = unit.form
        {
            return self.delimited_end(offset, delimiter, table, state);
        }
        if !unit.verbatim {
            match unit.form {
                SourceLine::Continuation
                    if matches!(
                        request,
                        ReadRequest::ListNext(_, _, ListPhase::AfterPrincipal)
                    ) =>
                {
                    return Ok(offset);
                }
                SourceLine::Attribute
                | SourceLine::Comment
                | SourceLine::Heading
                | SourceLine::Macro => return Ok(offset),
                SourceLine::List {
                    kind,
                    marker,
                    empty,
                } => {
                    let kind = match request {
                        ReadRequest::ListHead(kind) => kind,
                        ReadRequest::Block | ReadRequest::ListNext(..) | ReadRequest::ListEnd => {
                            kind
                        }
                    };
                    return self.principal_end(start..offset, (kind, marker, empty), state);
                }
                SourceLine::Markdown => return self.markdown_end(offset, state),
                SourceLine::Quoted => {
                    if let Some(end) = self.quoted_end(start, &unit, state)? {
                        return Ok(end);
                    }
                }
                SourceLine::Blank
                | SourceLine::Metadata
                | SourceLine::Continuation
                | SourceLine::Text
                | SourceLine::Delimited { .. } => {}
            }
        }
        self.paragraph_end(offset, unit.line, unit.verbatim, request, state)
    }

    fn block_start(
        &mut self,
        start: usize,
        request: ReadRequest<'a>,
        state: &mut ParserState<'a>,
    ) -> Result<Option<SourceStart<'a>>, Error> {
        let mut offset = start;
        let mut style = None;
        let mut pending_metadata = false;
        let control = matches!(
            request,
            ReadRequest::ListNext(_, _, ListPhase::Attachments | ListPhase::Sibling)
                | ReadRequest::ListEnd
        );
        loop {
            let Some(line) = self.line(offset, state)? else {
                return Ok(None);
            };
            let form = Self::kind(line, state);
            if (pending_metadata || control)
                && let SourceLine::Delimited {
                    delimiter,
                    table,
                    comment: true,
                    kind: _,
                } = form
            {
                offset = self.delimited_end(offset + line.len(), delimiter, table, state)?;
                continue;
            }
            if matches!(form, SourceLine::Blank | SourceLine::Metadata)
                || pending_metadata && matches!(form, SourceLine::Attribute | SourceLine::Comment)
                || control
                    && matches!(
                        form,
                        SourceLine::Continuation | SourceLine::Comment | SourceLine::Attribute
                    )
            {
                pending_metadata |= matches!(form, SourceLine::Metadata);
                if let Some(list) = crate::grammar::block_attribute_line(
                    line.trim_end_matches('\n'),
                    &state.document_attributes,
                    state.options.compatibility_mode,
                ) && let Some(selected) = crate::grammar::block_attribute_style(
                    list,
                    &state.document_attributes,
                    state.options.compatibility_mode,
                ) {
                    style = Some(selected);
                }
                offset += line.len();
            } else {
                let verbatim = crate::grammar::style_overrides_block_form(
                    state.options.compatibility_mode,
                    style.as_deref(),
                );
                return Ok(Some(SourceStart {
                    offset,
                    line,
                    form,
                    verbatim,
                    compound: matches!(form, SourceLine::Delimited {kind: Some(kind), ..} if kind.has_compound_body(style.as_deref())),
                }));
            }
        }
    }

    fn delimited_end(
        &mut self,
        mut offset: usize,
        delimiter: &str,
        table: bool,
        state: &mut ParserState<'a>,
    ) -> Result<usize, Error> {
        while let Some(next) = self.line(offset, state)? {
            offset += next.len();
            if document_parser::source_block_close(next, state, delimiter, table).is_ok() {
                break;
            }
        }
        Ok(offset.min(self.index.len()))
    }

    fn metadata_end(
        &mut self,
        mut offset: usize,
        state: &mut ParserState<'a>,
    ) -> Result<usize, Error> {
        while let Some(ahead) = self.line(offset, state)? {
            offset += ahead.len();
            match Self::kind(ahead, state) {
                SourceLine::Delimited {
                    delimiter,
                    table,
                    comment: true,
                    kind: _,
                } => {
                    offset = self.delimited_end(offset, delimiter, table, state)?;
                }
                SourceLine::Metadata | SourceLine::Comment | SourceLine::Blank => {}
                SourceLine::Attribute
                | SourceLine::Heading
                | SourceLine::Delimited { .. }
                | SourceLine::List { .. }
                | SourceLine::Macro
                | SourceLine::Markdown
                | SourceLine::Quoted
                | SourceLine::Continuation
                | SourceLine::Text => break,
            }
        }
        Ok(offset.min(self.index.len()))
    }

    fn principal_end(
        &mut self,
        range: std::ops::Range<usize>,
        principal: (ListKind<'a>, &'a str, bool),
        state: &mut ParserState<'a>,
    ) -> Result<usize, Error> {
        let (kind, marker, empty) = principal;
        let mut offset = range.end;
        let mut missing_principal = empty && matches!(kind, ListKind::Description);
        let mut metadata_end = 0;
        while let Some(next) = self.line(offset, state)? {
            let next_start = offset;
            let next_form = Self::kind(next, state);
            offset += next.len();
            if missing_principal && matches!(next_form, SourceLine::Blank) {
                continue;
            }
            missing_principal = false;
            let mut probe = next;
            if matches!(next_form, SourceLine::Metadata) {
                if metadata_end <= offset {
                    metadata_end = self.metadata_end(offset, state)?;
                }
                let view = self
                    .index
                    .window(range.start..metadata_end, state.arena)
                    .ok_or_else(|| {
                        source_error(state, next_start, "missing list principal lookahead")
                    })?;
                probe = view.get(next_start..metadata_end).ok_or_else(|| {
                    source_error(state, next_start, "missing list metadata lookahead")
                })?;
            }
            if document_parser::source_principal_line(probe, state, next_start, kind, marker)
                .is_err()
            {
                offset = offset.max(metadata_end);
                break;
            }
        }
        Ok(offset.min(self.index.len()))
    }

    fn markdown_end(
        &mut self,
        mut offset: usize,
        state: &mut ParserState<'a>,
    ) -> Result<usize, Error> {
        while let Some(next) = self.line(offset, state)? {
            offset += next.len();
            if !matches!(Self::kind(next, state), SourceLine::Markdown) {
                break;
            }
        }
        Ok(offset.min(self.index.len()))
    }

    fn quoted_end(
        &mut self,
        start: usize,
        unit: &SourceStart<'a>,
        state: &mut ParserState<'a>,
    ) -> Result<Option<usize>, Error> {
        let mut offset = unit.offset + unit.line.len();
        let mut closed = unit.line.get(1..).is_some_and(|line| line.contains('"'));
        while !closed {
            let Some(next) = self.line(offset, state)? else {
                break;
            };
            offset += next.len();
            closed = next.contains('"');
        }
        if closed && let Some(next) = self.line(offset, state)? {
            offset += next.len();
            if let Some(view) = self.index.window(start..offset, state.arena)
                && let Some(source) = view.get(unit.offset..offset)
                && document_parser::source_quoted_paragraph(source, state).is_ok()
            {
                return Ok(Some(offset));
            }
        }
        // An invalid shorthand quote remains available as an ordinary paragraph.
        Ok(None)
    }

    fn paragraph_end(
        &mut self,
        mut offset: usize,
        line: &'a str,
        verbatim: bool,
        request: ReadRequest<'a>,
        state: &mut ParserState<'a>,
    ) -> Result<usize, Error> {
        let parent = match request {
            ReadRequest::ListNext(ListKind::Unordered(_), marker, _) => {
                Some(super::lists::ListMarker::Unordered(marker))
            }
            ReadRequest::ListNext(ListKind::Ordered(_), marker, _) => {
                Some(super::lists::ListMarker::Ordered(marker))
            }
            ReadRequest::ListNext(ListKind::Description, marker, _) => {
                Some(super::lists::ListMarker::Description(marker))
            }
            ReadRequest::Block
            | ReadRequest::ListHead(_)
            | ReadRequest::ListNext(ListKind::Callout, ..)
            | ReadRequest::ListEnd => None,
        };
        let mut first = true;
        while let Some(next) = self.line(offset, state)? {
            let next_start = offset;
            offset += next.len();
            if next == "\n" {
                break;
            }
            if first
                && !verbatim
                && crate::grammar::setext::is_title(line.trim_end_matches('\n'))
                && document_parser::source_setext_underline(
                    next,
                    state,
                    line.trim().chars().count(),
                )
                .is_ok()
            {
                break;
            }
            first = false;
            if (verbatim
                && matches!(
                    Self::kind(next, state),
                    SourceLine::Blank | SourceLine::Continuation
                ))
                || (!verbatim
                    && (next == "\n"
                        || document_parser::source_paragraph_boundary(
                            next, state, next_start, parent,
                        )
                        .is_ok()))
            {
                break;
            }
        }
        Ok(offset.min(self.index.len()))
    }

    pub(super) fn prepare_header(&mut self, state: &mut ParserState<'a>) -> Result<&'a str, Error> {
        self.prepare_header_from(0, state)
    }

    pub(super) fn prepare_header_from(
        &mut self,
        start: usize,
        state: &mut ParserState<'a>,
    ) -> Result<&'a str, Error> {
        let mut offset = start;
        let mut title = false;
        let mut header_started = false;
        while let Some(line) = self.line(offset, state)? {
            let kind = Self::kind(line, state);
            offset += line.len();
            if let SourceLine::Delimited {
                delimiter,
                table,
                comment: true,
                kind: _,
            } = kind
            {
                while let Some(next) = self.line(offset, state)? {
                    offset += next.len();
                    if document_parser::source_block_close(next, state, delimiter, table).is_ok() {
                        break;
                    }
                }
                continue;
            }
            if !title
                && matches!(kind, SourceLine::Text)
                && crate::grammar::setext::is_title(line.trim_end_matches('\n'))
                && let Some(next) = self.line(offset, state)?
                && document_parser::source_document_underline(
                    next,
                    state,
                    line.trim().chars().count(),
                )
                .is_ok()
            {
                title = true;
                offset += next.len();
            }
            header_started |= title || matches!(kind, SourceLine::Attribute | SourceLine::Metadata);
            if matches!(kind, SourceLine::Blank) && header_started {
                break;
            }
            if document_parser::source_document_title(line, state).is_ok() {
                title = true;
                header_started = true;
            }
            if !title
                && !matches!(
                    kind,
                    SourceLine::Attribute
                        | SourceLine::Metadata
                        | SourceLine::Comment
                        | SourceLine::Blank
                )
            {
                break;
            }
        }
        let view = self
            .index
            .window(start..offset.min(self.available_end()), state.arena)
            .ok_or_else(|| source_error(state, 0, "missing document prefix window"))?;
        let base = view.origin.as_ref().map_or(0, |origin| origin.base);
        let source = view
            .input
            .get(start - base..offset.min(self.available_end()) - base)
            .ok_or_else(|| source_error(state, start, "missing nested header window"))?;
        state.set_source_view(view);
        Ok(source)
    }
}

impl<'a> SourceInput<'a> {
    #[inline]
    fn buffered_prose(&self, cursor: &mut BlockCursor, state: &ParserState<'a>) -> Option<&'a str> {
        let start = cursor.base_offset + cursor.offset;
        if let Some(origin) = state.input_origin.as_ref().filter(|origin| {
            Rc::ptr_eq(&origin.index, &self.index)
                && origin.base <= start
                && origin.base + state.input.len() <= self.index.len()
        }) && let Some(tail) = state.input.get(start - origin.base..)
            && complete_prose_prefix(tail)
        {
            cursor.offset = start - origin.base;
            cursor.base_offset = origin.base;
            return Some(state.input);
        }
        None
    }

    fn prepare_window(
        &mut self,
        cursor: &mut BlockCursor,
        state: &mut ParserState<'a>,
        request: ReadRequest<'a>,
    ) -> Result<&'a str, Error> {
        let start = cursor.base_offset + cursor.offset;
        let end = self.scan(start, request, state)?;
        if let Some(origin) = state.input_origin.as_ref().filter(|origin| {
            !self.stream_open
                && !self.stream_table
                && self.cells.is_empty()
                && self.boundaries.is_empty()
                && Rc::ptr_eq(&origin.index, &self.index)
                && origin.base <= start
                && end <= origin.base + state.input.len()
                && origin.base + state.input.len() <= self.index.len()
        }) {
            cursor.offset = start - origin.base;
            cursor.base_offset = origin.base;
            return Ok(state.input);
        }
        let mut view = self
            .index
            .window(start..end, state.arena)
            .ok_or_else(|| source_error(state, start, "missing accepted source window"))?;
        let base = view.origin.as_ref().map_or(0, |origin| origin.base);
        if self.stream_open
            || self.stream_table
            || !self.cells.is_empty()
            || !self.boundaries.is_empty()
        {
            let mut limit = end;
            if let Some(delimiter) = self.boundaries.last() {
                let mut offset = start;
                for line in view
                    .input
                    .get(start - base..end - base)
                    .unwrap_or_default()
                    .split_inclusive('\n')
                {
                    if offset != start
                        && document_parser::source_block_close(line, state, delimiter, false)
                            .is_ok()
                    {
                        limit = offset.saturating_sub(usize::from(
                            state.source_byte(offset.saturating_sub(1)) == Some(b'\n'),
                        ));
                        break;
                    }
                    offset += line.len();
                }
            }
            view.input = view
                .input
                .get(..limit - base)
                .ok_or_else(|| source_error(state, start, "missing compound opening window"))?;
        }
        cursor.offset = start - base;
        cursor.base_offset = base;
        let source = view.input;
        state.set_source_view(view);
        Ok(source)
    }
}

impl<'a> SourceInput<'a> {
    pub(super) fn streaming_table(&self) -> bool {
        self.stream_table
    }

    pub(super) fn cell_end(&self) -> Option<usize> {
        self.cells.last().and_then(|cell| cell.end)
    }
    pub(super) fn table_header_lookahead(&self) -> Vec<&'a str> {
        self.reader
            .as_ref()
            .map_or_else(Vec::new, SourceReader::table_header_lookahead)
    }
    pub(super) fn begin_cell(&mut self, separator: char, delimiter: &'a str, start: usize) {
        self.cells.push(CellRead {
            separator,
            delimiter,
            start,
            end: None,
        });
    }
    pub(super) fn end_cell(&mut self) {
        self.cells.pop();
    }
    pub(super) fn reader_attributes(
        &mut self,
        attributes: &DocumentAttributes<'a>,
        nested_parent: Option<&Rc<DocumentAttributes<'a>>>,
    ) {
        self.updates.nested_parent.replace(nested_parent.cloned());
        self.updates.assignments.borrow_mut().clear();
        if let Some(reader) = self.reader.as_mut() {
            reader.options_mut().document_attributes = attributes.clone().into_static();
        }
    }

    pub(super) fn streaming_compound(&self) -> bool {
        self.stream_open
    }
    pub(super) fn boundary(&self) -> Option<&'a str> {
        self.boundaries.last().copied()
    }
    pub(super) fn push_boundary(&mut self, delimiter: &'a str) {
        self.boundaries.push(delimiter);
    }
    pub(super) fn pop_boundary(&mut self) {
        self.boundaries.pop();
    }
}

impl<'a> BlockInput<'a> for SourceInput<'a> {
    fn source_input(&mut self) -> Option<&mut SourceInput<'a>> {
        Some(self)
    }
    #[inline]
    fn prepare(
        &mut self,
        cursor: &mut BlockCursor,
        state: &mut ParserState<'a>,
        request: ReadRequest<'a>,
    ) -> Result<&'a str, Error> {
        if self.boundaries.is_empty()
            && self.cells.is_empty()
            && matches!(request, ReadRequest::Block)
            && let Some(source) = self.buffered_prose(cursor, state)
        {
            return Ok(source);
        }
        let result = self.prepare_window(cursor, state, request);
        self.failed |= result.is_err();
        result
    }

    fn failed(&self) -> bool {
        self.failed
    }
}

fn is_prose_line(line: &str) -> bool {
    line.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && !line.contains("::")
        && !line.contains(";;")
}

/// A blank after a few ordinary lines proves the first block can commit
/// within the existing buffer. Bound the probe so adjacent headings without
/// blank lines cannot repeatedly scan the remainder of a large source chunk.
fn complete_prose_prefix(source: &str) -> bool {
    for line in source.trim_start_matches('\n').lines().take(4) {
        if line.is_empty() {
            return true;
        }
        if !is_prose_line(line) {
            return false;
        }
    }
    false
}

fn source_error(state: &ParserState<'_>, offset: usize, message: &str) -> Error {
    Error::Parse(
        Box::new(state.create_error_source_location(state.create_location(offset, offset))),
        message.into(),
    )
}

fn recognition_error(mut error: super::RecognitionError, state: &ParserState<'_>) -> Error {
    let source = crate::peg_error_to_source_location(&error, state);
    let offset = state.input_offset(error.location.offset);
    let position = state.source_position(offset);
    error.location.offset = offset;
    error.location.line = position.line as usize;
    error.location.column = position.column as usize;
    Error::Parse(Box::new(source), error.to_string())
}

pub(crate) fn document_from_source<'a>(
    mut reader: SourceReader<'a>,
    state: &mut ParserState<'a>,
    handler: &mut dyn SourceHandler,
    conditionals: &mut Vec<crate::ConditionalEvaluation>,
    finish_source: impl FnOnce(Cow<'a, str>) -> &'a str,
) -> Result<Document<'a>, Error> {
    if let Some(result) = reader.take_pass_through() {
        let source = finish_source(result.text);
        if !std::ptr::eq(state.input, source) {
            state.set_source_view(SourceView {
                input: source,
                line_map: Rc::new(LineMap::new(source)),
                origin: None,
            });
        }
        let mut callback =
            |block: &mut Block<'a>, state: &mut ParserState<'a>, output: &mut Vec<Block<'a>>| {
                execute_source_handler(handler, block, state, None, output)
            };
        let document =
            document_with_execution(source, state, ConstructionMode::Ordered, &mut callback)
                .map_err(|error| recognition_error(error, state))??;
        *conditionals = result.conditionals;
        return Ok(document);
    }
    let updates = PendingAssignments::default();
    let mut callback =
        |block: &mut Block<'a>, state: &mut ParserState<'a>, output: &mut Vec<Block<'a>>| {
            execute_source_handler(handler, block, state, Some(&updates), output)
        };
    let index = state
        .input_origin
        .as_ref()
        .map_or_else(Rc::default, |origin| Rc::clone(&origin.index));
    let mut input = SourceInput::new(reader, Rc::clone(&updates), index);
    state.set_source_view(
        input
            .index
            .window(0..0, state.arena)
            .ok_or_else(|| source_error(state, 0, "missing initial source window"))?,
    );
    state.source_scope_offset = 0;
    let source = input.prepare_header(state)?;
    let prefix = document_parser::document_prefix(source, state)
        .map_err(|error| recognition_error(error, state))??;
    let mut cursor = BlockCursor::new(prefix.body_offset, ConstructionMode::Ordered);
    let mut blocks = prefix.comments;
    let end = cursor
        .collect(
            &mut input,
            state,
            &mut blocks,
            &mut Execution::new(&mut callback),
        )
        .map_err(|error| recognition_error(error, state))??;
    let result = input
        .result
        .take()
        .ok_or_else(|| source_error(state, end, "document ended before source exhaustion"))?;
    let source = finish_source(result.text);
    state.input = source;
    state.input_origin = None;
    state.source_scope_offset = 0;
    state.line_map = Rc::new(crate::grammar::LineMap::new(source));
    state.source_ranges = result.source_ranges;
    state.leveloffset_ranges = result.leveloffset_ranges;
    state.included_files = result.included_files;
    let document = super::finish::finish(
        state,
        prefix.start,
        end,
        prefix.header,
        prefix.attributes,
        blocks,
    )?;
    *conditionals = result.conditionals;
    Ok(document)
}

#[inline]
fn execute_source_handler<'a>(
    handler: &mut dyn SourceHandler,
    block: &mut Block<'a>,
    state: &mut ParserState<'a>,
    updates: Option<&PendingAssignments<'a>>,
    output: &mut Vec<Block<'a>>,
) -> Result<bool, Error> {
    let (keep, assignments) = handler.construct(block, state)?;
    if !assignments.is_empty() {
        apply_assignments(assignments, block, state, updates, output);
    }
    Ok(keep)
}

fn apply_assignments<'a>(
    assignments: Assignments<'a>,
    block: &Block<'a>,
    state: &mut ParserState<'a>,
    updates: Option<&PendingAssignments<'a>>,
    output: &mut Vec<Block<'a>>,
) {
    for (name, assignment) in assignments {
        let name = state.intern_cow(name);
        let value = match assignment {
            DocumentAttributeAssignment::Set(value) => RawAttributeValue::Resolved(value),
            DocumentAttributeAssignment::Unset => RawAttributeValue::Unset,
        };
        if let Some(event) = state.apply_document_attribute(
            &AttributeDeclaration {
                name,
                value,
                ignored_value: None,
            },
            false,
            block.location().clone(),
        ) {
            if let Some(updates) = updates
                && state
                    .nested_parent_attributes
                    .as_ref()
                    .is_none_or(|parent| {
                        updates
                            .nested_parent
                            .borrow()
                            .as_ref()
                            .is_some_and(|selected| Rc::ptr_eq(parent, selected))
                    })
            {
                updates.assignments.borrow_mut().push((
                    name.to_owned().into(),
                    event.assignment().clone().into_static(),
                ));
            }
            output.push(Block::DocumentAttribute(event));
        }
    }
}
