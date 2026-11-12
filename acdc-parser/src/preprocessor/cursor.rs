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
}

struct IncludeFrame<'input> {
    result: IncludeResult,
    cursor: Box<InputCursor<'input>>,
    input_line: usize,
    scope: Option<IncludeScope>,
}

impl<'input> InputCursor<'input> {
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

    fn skip_unchanged_include(&mut self, options: &Options<'_>) {
        if self.preprocessor.include_context.depth == 0
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
        self.skip_unchanged_include(options);
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
