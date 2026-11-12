use std::{borrow::Cow, iter::Peekable, str::Lines};

use bumpalo::Bump;

use crate::{Error, Options};

use super::{
    CommentScanner, ConditionalFrame, DirectiveContext, Preprocessor, PreprocessorResult,
    PreprocessorState, SkimmedFrontMatter, SourceLineOrigin, SourceOrigin, attribute,
    consume_front_matter, has_attribute_continuation, has_include_directive_shape,
    include::{IncludeResult, IncludeScope},
    is_escaped_directive, skim_front_matter,
};

/// Retains source-reader state between advances. A later directive is not
/// processed until the caller advances to its line.
pub(super) struct InputCursor<'input> {
    preprocessor: Preprocessor,
    source_origin: Option<SourceOrigin>,
    arena: &'input Bump,
    lines: Peekable<Lines<'input>>,
    line_number: usize,
    out: PreprocessorState<'input>,
    scanner: CommentScanner,
    conditionals: Vec<ConditionalFrame<'input>>,
    front_matter: Option<String>,
    include: Option<IncludeFrame<'input>>,
    bulk_consumed: bool,
}

struct IncludeFrame<'input> {
    result: IncludeResult,
    cursor: Box<InputCursor<'input>>,
    input_line: usize,
    scope: Option<IncludeScope>,
    pending_blank: Option<Emission<'input>>,
    prefix_emitted: bool,
    emitted_end: usize,
}

/// Stable text emitted by one reader advance, in this cursor's coordinates.
/// The final separator is provisional until the enclosing source reaches EOF.
pub(super) struct Emission<'input> {
    pub(super) start: usize,
    pub(super) text: &'input str,
    pub(super) source_ranges: Vec<crate::model::SourceRange>,
    pub(super) leveloffset_ranges: Vec<crate::model::LeveloffsetRange>,
    pub(super) leveloffset_scopes: Vec<super::LeveloffsetScope>,
}

pub(super) enum Advance<'input> {
    Progress(Vec<Emission<'input>>),
    End,
}

impl Emission<'_> {
    fn prepend_include(&mut self, include: &IncludeResult, base: usize) {
        self.start += base;
        for range in &mut self.source_ranges {
            range.start_offset += base;
            range.end_offset += base;
            range.file_chain.insert(0, include.target.clone());
        }
        for range in &mut self.leveloffset_ranges {
            range.start_offset += base;
            range.end_offset += base;
        }
        for scope in &mut self.leveloffset_scopes {
            scope.range.start += base;
            scope.range.end += base;
        }
        if let Some(value) = include.effective_leveloffset.filter(|value| *value != 0) {
            self.leveloffset_ranges.insert(
                0,
                crate::model::LeveloffsetRange::new(
                    self.start,
                    self.start + self.text.len(),
                    value,
                ),
            );
        }
    }

    fn take_final_blank(&mut self) -> Option<Self> {
        if self.text != "\n" && !self.text.ends_with("\n\n") {
            return None;
        }
        let split = self.start + self.text.len() - 1;
        let source_ranges = self
            .source_ranges
            .iter()
            .filter_map(|range| {
                if range.end_offset <= split {
                    return None;
                }
                let mut tail = range.clone();
                let local_start = range.start_offset.saturating_sub(self.start);
                let prefix = self.text.get(local_start..split - self.start)?;
                tail.start_offset = split;
                tail.start_line += prefix.bytes().filter(|byte| *byte == b'\n').count();
                tail.source_start_offset += prefix.len();
                Some(tail)
            })
            .collect();
        let leveloffset_ranges = self
            .leveloffset_ranges
            .iter()
            .filter(|range| range.end_offset > split)
            .map(|range| crate::model::LeveloffsetRange::new(split, range.end_offset, range.value))
            .collect();
        let blank = Self {
            start: split,
            text: "\n",
            source_ranges,
            leveloffset_ranges,
            leveloffset_scopes: self
                .leveloffset_scopes
                .iter()
                .filter(|scope| scope.range.end > split)
                .cloned()
                .map(|mut scope| {
                    scope.range.start = split.max(scope.range.start);
                    scope
                })
                .collect(),
        };
        self.text = self.text.get(..self.text.len() - 1)?;
        for range in &mut self.source_ranges {
            range.end_offset = range.end_offset.min(split);
        }
        self.source_ranges
            .retain(|range| range.start_offset < range.end_offset);
        for range in &mut self.leveloffset_ranges {
            range.end_offset = range.end_offset.min(split);
        }
        self.leveloffset_ranges
            .retain(|range| range.start_offset < range.end_offset);
        for scope in &mut self.leveloffset_scopes {
            scope.range.end = scope.range.end.min(split);
        }
        self.leveloffset_scopes
            .retain(|scope| scope.range.start < scope.range.end);
        Some(blank)
    }
}

impl<'input> InputCursor<'input> {
    pub(super) fn can_finish_without_reading(&mut self) -> bool {
        self.lines.peek().is_none()
            && self
                .include
                .as_mut()
                .is_none_or(|include| include.cursor.can_finish_without_reading())
    }

    /// The ordered consumer sees included output as it is read. The ordinary
    /// `advance`/`read_all` path continues to merge each include only once.
    pub(super) fn advance_emitted(
        &mut self,
        options: &mut Options<'_>,
    ) -> Result<Advance<'input>, Error> {
        if let Some(include) = self.include.take() {
            return self.advance_include_emitted(include, options);
        }
        let start = self.out.byte_offset;
        let source_count = self.out.source_ranges.len();
        let level_count = self.out.leveloffset_ranges.len();
        let scope_count = self.out.leveloffset_scopes.len();
        let origin = self.out.origin_of_line(self.line_number);
        // A source with no preprocessing effects can be handed over as one
        // borrowed chunk. Its blocks still commit individually in the driver.
        self.skip_unchanged_input(options, true);
        if self.out.byte_offset == start {
            let Some(line) = self.lines.next() else {
                return Ok(Advance::End);
            };
            self.process_line(line, options)?;
            self.line_number += 1;
        }
        let Some(text) = self.emitted_text(start) else {
            return Ok(Advance::Progress(Vec::new()));
        };
        let end = start + text.len();
        let mut source_ranges: Vec<_> = self
            .out
            .source_ranges
            .iter()
            .skip(source_count)
            .filter(|range| range.end_offset > start)
            .cloned()
            .collect();
        if source_ranges.is_empty() {
            source_ranges.push(crate::model::SourceRange {
                start_offset: start,
                end_offset: end,
                file: self.out.source_file.as_deref().cloned(),
                file_chain: Vec::new(),
                start_line: origin.line,
                source_start_offset: origin.offset,
                column_shift: origin.column_shift,
            });
        }
        Ok(Advance::Progress(vec![Emission {
            start,
            text,
            source_ranges,
            leveloffset_ranges: self
                .out
                .leveloffset_ranges
                .iter()
                .skip(level_count)
                .cloned()
                .collect(),
            leveloffset_scopes: self
                .out
                .leveloffset_scopes
                .iter()
                .skip(scope_count)
                .cloned()
                .collect(),
        }]))
    }

    fn advance_include_emitted(
        &mut self,
        mut include: IncludeFrame<'input>,
        options: &mut Options<'_>,
    ) -> Result<Advance<'input>, Error> {
        let prefix_len = include.scope.as_ref().map_or(0, |scope| scope.prefix.len());
        if !include.prefix_emitted {
            include.prefix_emitted = true;
            if let Some(scope) = &include.scope {
                let start = self.out.byte_offset;
                let emission = Emission {
                    start,
                    text: self.arena.alloc_str(&scope.prefix),
                    source_ranges: Vec::new(),
                    leveloffset_ranges: Vec::new(),
                    leveloffset_scopes: vec![super::LeveloffsetScope {
                        range: start..start + prefix_len,
                        assignment: scope.current.clone(),
                    }],
                };
                include.emitted_end = start + prefix_len;
                self.include = Some(include);
                return Ok(Advance::Progress(vec![emission]));
            }
        }
        match include.cursor.advance_emitted(options)? {
            Advance::Progress(emissions) => {
                let mut output = Vec::new();
                for mut emission in emissions {
                    emission.prepend_include(&include.result, self.out.byte_offset + prefix_len);
                    if let Some(blank) = include.pending_blank.take() {
                        output.push(blank);
                    }
                    include.pending_blank = emission.take_final_blank();
                    if !emission.text.is_empty() {
                        output.push(emission);
                    }
                }
                if let Some(last) = output.last() {
                    include.emitted_end = last.start + last.text.len();
                }
                self.include = Some(include);
                Ok(Advance::Progress(output))
            }
            Advance::End => {
                // The enclosing include removes one final newline. A
                // pending empty line is therefore never exposed as text.
                let result = include.cursor.read_all(options)?;
                let suffix_start = self.out.byte_offset + prefix_len + result.text.len();
                // The child's provisional final newline can already serve
                // as the first newline of the generated scope suffix.
                let suffix_skip = include.emitted_end.saturating_sub(suffix_start);
                let start = self.out.byte_offset;
                let scope_count = self.out.leveloffset_scopes.len();
                let scoped = include.scope.is_some();
                self.finish_include(
                    include.result,
                    result,
                    include.input_line,
                    include.scope,
                    options,
                )?;
                let mut emissions = Vec::new();
                if scoped
                    && let Some(text) = self
                        .emitted_text(start)
                        .and_then(|text| text.get(suffix_start - start + suffix_skip..))
                {
                    emissions.push(Emission {
                        start: suffix_start + suffix_skip,
                        text,
                        source_ranges: Vec::new(),
                        leveloffset_ranges: Vec::new(),
                        leveloffset_scopes: self
                            .out
                            .leveloffset_scopes
                            .iter()
                            .skip(scope_count)
                            .filter(|scope| scope.range.end > suffix_start)
                            .cloned()
                            .collect(),
                    });
                }
                Ok(Advance::Progress(emissions))
            }
        }
    }

    fn emitted_text(&self, start: usize) -> Option<&'input str> {
        let mut remaining = self.out.byte_offset.checked_sub(start)?;
        if remaining == 0 {
            return None;
        }
        if start == 0
            && self.out.borrowed_run.is_none()
            && let [Cow::Borrowed(text)] = self.out.output.as_slice()
            && std::ptr::eq(text.as_ptr(), self.out.input.as_ptr())
            && text.len() == self.out.input.len()
        {
            return Some(self.arena.alloc_str(&format!("{text}\n")));
        }
        let borrowed = self
            .out
            .borrowed_run
            .as_ref()
            .and_then(|range| self.out.input.get(range.clone()));
        let mut pieces = Vec::new();
        for text in borrowed
            .into_iter()
            .chain(self.out.output.iter().rev().map(Cow::as_ref))
        {
            let length = remaining.min(text.len() + 1);
            let suffix = text.get(text.len() + 1 - length..)?;
            pieces.push(suffix);
            remaining -= length;
            if remaining == 0 {
                break;
            }
        }
        if remaining != 0 {
            return None;
        }
        if pieces.len() == 1 {
            let text = pieces.pop()?;
            let offset = (text.as_ptr() as usize).checked_sub(self.out.input.as_ptr() as usize);
            if let Some(offset) = offset
                && let Some(with_separator) = self.out.input.get(offset..offset + text.len() + 1)
                && with_separator.ends_with('\n')
            {
                return Some(with_separator);
            }
            return Some(self.arena.alloc_str(&format!("{text}\n")));
        }
        let mut joined = String::new();
        for piece in pieces.into_iter().rev() {
            joined.push_str(piece);
            joined.push('\n');
        }
        Some(self.arena.alloc_str(&joined))
    }

    pub(super) fn new(
        preprocessor: &Preprocessor,
        input: &'input str,
        source_origin: Option<&SourceOrigin>,
        options: &mut Options<'_>,
        line_origins: Vec<SourceLineOrigin>,
        front_matter: Option<SkimmedFrontMatter>,
        arena: &'input Bump,
    ) -> Self {
        let mut lines = input.lines().peekable();
        let mut line_number = 1;
        let front_matter = consume_front_matter(
            &mut lines,
            &mut line_number,
            preprocessor.include_context.depth == 0,
            front_matter,
        );
        if let Some(value) = &front_matter {
            options
                .document_attributes
                .set_text("front-matter".into(), value.clone().into());
        }
        let mut out = PreprocessorState::new(input, source_origin, line_origins);
        out.record_leveloffset(options);
        Self {
            preprocessor: preprocessor.clone(),
            source_origin: source_origin.cloned(),
            arena,
            lines,
            line_number,
            out,
            scanner: CommentScanner::new(
                crate::grammar::setext::is_enabled(options.compatibility_mode),
                preprocessor.include_context.block_context.clone(),
                options.compatibility_mode,
            ),
            conditionals: Vec::new(),
            front_matter,
            include: None,
            bulk_consumed: false,
        }
    }

    pub(super) fn advance(&mut self, options: &mut Options<'_>) -> Result<bool, Error> {
        if let Some(mut include) = self.include.take() {
            if include.cursor.advance(options)? {
                self.include = Some(include);
            } else {
                let result = include.cursor.read_all(options)?;
                self.finish_include(
                    include.result,
                    result,
                    include.input_line,
                    include.scope,
                    options,
                )?;
            }
            return Ok(true);
        }
        let Some(line) = self.lines.next() else {
            return Ok(false);
        };
        self.process_line(line, options)?;
        self.line_number += 1;
        Ok(true)
    }

    fn process_line(&mut self, line: &'input str, options: &mut Options<'_>) -> Result<(), Error> {
        if self.conditionals.last().is_none_or(|frame| frame.active)
            && self.scanner.preserves_comment_line(line)
        {
            self.out.push_source_line(line, self.line_number);
            self.scanner.record(line, &options.document_attributes);
            return Ok(());
        }
        let origin = self.out.origin_of_line(self.line_number);
        let ctx = DirectiveContext {
            input_line: self.line_number,
            source_line: origin.line,
            current_offset: origin.offset,
            source_origin: self.source_origin.as_ref(),
        };
        if self.preprocessor.process_conditional_line(
            line,
            &ctx,
            &options.document_attributes,
            &mut self.conditionals,
            &mut self.out,
            &mut self.scanner,
        )? || self.conditionals.last().is_some_and(|frame| !frame.active)
        {
            return Ok(());
        }
        if self.scanner.allows_attribute_entries() && has_attribute_continuation(line) {
            let start_line = self.line_number;
            let content =
                Preprocessor::process_continuation(line, &mut self.lines, &mut self.line_number);
            attribute::parse_line(options, &content)?;
            self.out.record_leveloffset(options);
            self.out.push_chunk(content, start_line);
            self.scanner.record(line, &options.document_attributes);
            return Ok(());
        } else if self.scanner.allows_attribute_entries() && line.starts_with(':') {
            attribute::parse_line(options, line.trim())?;
        }
        self.out.record_leveloffset(options);
        if self.scanner.at_table_delimiter(line) || self.scanner.at_verbatim_delimiter(line) {
            self.out.push_source_line(line, self.line_number);
        } else if line.starts_with("//") {
            if self.scanner.drops(line) {
                return Ok(());
            }
            self.out.push_source_line(line, self.line_number);
        } else if line.ends_with(']') && !line.starts_with('[') && line.contains("::") {
            return self.process_directive_line(line, options);
        } else {
            self.out.push_source_line(line, self.line_number);
        }
        self.scanner.record(line, &options.document_attributes);
        Ok(())
    }

    fn process_directive_line(
        &mut self,
        line: &'input str,
        options: &mut Options<'_>,
    ) -> Result<(), Error> {
        if is_escaped_directive(line) {
            self.out.note_source_line(self.line_number);
            self.out.push_line(Cow::Borrowed(&line[1..]));
            self.scanner
                .record(&line[1..], &options.document_attributes);
        } else if has_include_directive_shape(line) {
            self.process_include_line(line, options)?;
        } else {
            self.out.push_source_line(line, self.line_number);
            self.scanner.record(line, &options.document_attributes);
        }
        Ok(())
    }

    fn process_include_line(
        &mut self,
        line: &'input str,
        options: &mut Options<'_>,
    ) -> Result<(), Error> {
        let origin = self.out.origin_of_line(self.line_number);
        let ctx = DirectiveContext {
            input_line: self.line_number,
            source_line: origin.line,
            current_offset: origin.offset,
            source_origin: self.source_origin.as_ref(),
        };
        let blocked = if options.include_loader.provider().is_none()
            && options.safe_mode != crate::SafeMode::Secure
        {
            Some("include not read because include loading is disabled".to_owned())
        } else {
            self.preprocessor
                .include_context
                .blocked_limit(options.safe_mode)
                .map(|limit| {
                    if limit == 0 {
                        "include not read because maximum include depth is zero".to_owned()
                    } else {
                        format!("maximum include depth of {limit} exceeded")
                    }
                })
        };
        if let Some(message) = blocked {
            self.preprocessor.recover_content_at(
                message,
                Preprocessor::create_source_location(ctx.source_line, ctx.current_file()),
            );
            self.out.push_source_line(line, self.line_number);
            self.scanner.record(line, &options.document_attributes);
            return Ok(());
        }
        self.out.flush_run();
        let Some(mut included) = self.preprocessor.process_include(
            line,
            ctx.source_line,
            ctx.current_offset,
            ctx.source_origin,
            options,
            super::IncludeContext {
                block_context: self.scanner.block_context(),
                ..self.preprocessor.include_context.clone()
            },
        )?
        else {
            self.out.push_source_line(line, self.line_number);
            self.scanner.record(line, &options.document_attributes);
            return Ok(());
        };
        if let Some(input) = included.input.take() {
            if let Some(attributes) = input.attributes {
                options.document_attributes = attributes;
            }
            let normalized = Preprocessor::normalize(&included.content);
            let text = self.arena.alloc_str(&normalized);
            let front_matter = options
                .document_attributes
                .contains_key("skip-front-matter")
                .then(|| skim_front_matter(text.lines()))
                .flatten();
            let preprocessor = Preprocessor::nested(&self.preprocessor.warnings, input.context);
            let cursor = Self::new(
                &preprocessor,
                text,
                Some(&input.source_origin),
                options,
                input.line_origins,
                front_matter,
                self.arena,
            );
            self.include = Some(IncludeFrame {
                result: included,
                cursor: Box::new(cursor),
                input_line: self.line_number,
                scope: input.scope,
                pending_blank: None,
                prefix_emitted: false,
                emitted_end: self.out.byte_offset,
            });
        } else {
            self.accept_include(included, self.line_number, options);
        }
        Ok(())
    }

    fn accept_include(&mut self, mut included: IncludeResult, line: usize, options: &Options<'_>) {
        if let Some(context) = included.comment_context.take() {
            self.scanner.record_preprocessed(&included.content, context);
        } else {
            self.scanner
                .record_expansion(&included.content, &options.document_attributes);
        }
        Preprocessor::handle_include_result(included, &mut self.out, line);
    }

    fn finish_include(
        &mut self,
        mut included: IncludeResult,
        mut result: PreprocessorResult<'_>,
        line: usize,
        scope: Option<IncludeScope>,
        options: &mut Options<'_>,
    ) -> Result<(), Error> {
        if let Some(scope) = scope {
            scope.finish(&mut result, &mut options.document_attributes)?;
        }
        included.content = result.text.into_owned();
        included.leveloffset_ranges = result.leveloffset_ranges;
        included.leveloffset_scopes = result.leveloffset_scopes;
        included.source_ranges = result.source_ranges;
        included.included_files.extend(result.included_files);
        included.conditionals = result.conditionals;
        included.comment_context = Some(result.comment_context);
        self.accept_include(included, line, options);
        Ok(())
    }

    fn skip_unchanged_input(&mut self, options: &Options<'_>, include_root: bool) {
        if (!include_root && self.preprocessor.include_context.depth == 0)
            || self.bulk_consumed
            || self.line_number != 1
            || self.front_matter.is_some()
            || self
                .preprocessor
                .nested_attributes_must_propagate(self.out.input)
        {
            return;
        }
        let Some(scanner) = self.preprocessor.try_pass_through(
            self.out.input,
            crate::grammar::setext::is_enabled(options.compatibility_mode),
            options,
        ) else {
            return;
        };
        self.bulk_consumed = true;
        for line in self.lines.by_ref() {
            self.out.note_source_line(self.line_number);
            self.out.byte_offset += line.len() + 1;
            self.line_number += 1;
        }
        self.out.output.push(Cow::Borrowed(self.out.input));
        self.scanner = scanner;
    }

    pub(super) fn read_all(
        mut self,
        options: &mut Options<'_>,
    ) -> Result<PreprocessorResult<'static>, Error> {
        self.skip_unchanged_input(options, false);
        loop {
            if let Some(include) = self.include.take() {
                let result = include.cursor.read_all(options)?;
                self.finish_include(
                    include.result,
                    result,
                    include.input_line,
                    include.scope,
                    options,
                )?;
            } else if !self.advance(options)? {
                break;
            }
        }
        let file = self.source_origin.as_ref().and_then(SourceOrigin::as_path);
        for frame in self.conditionals {
            self.preprocessor.recover_content_at(
                "conditional directive has no matching endif",
                Preprocessor::create_source_location(frame.opening_line, file),
            );
        }
        Ok(self
            .out
            .into_result(self.front_matter, self.scanner.block_context()))
    }
}
