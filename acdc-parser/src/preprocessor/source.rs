//! Reader facade used by the ordered source-processing driver.

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashSet, VecDeque},
    path::Path,
    rc::Rc,
};

use bumpalo::Bump;

use crate::{
    Error, Options,
    extensions::{SourceEdit, SourceProcessorContext},
    model::{LeveloffsetRange, SourceRange},
    warning::WarningCollection,
};

use super::{
    Preprocessor, PreprocessorResult, SourceLineOrigin, SourceOrigin,
    cursor::{Advance, Emission, InputCursor},
    has_attribute_driven_styles, skim_front_matter,
};

pub(crate) struct SourceChunk<'a> {
    pub(crate) start: usize,
    pub(crate) text: &'a str,
    pub(crate) source_ranges: Vec<SourceRange>,
    pub(crate) leveloffset_ranges: Vec<LeveloffsetRange>,
    pub(crate) leveloffset_scopes: Vec<super::LeveloffsetScope>,
}

pub(crate) struct SourceReader<'a> {
    cursor: Option<InputCursor<'a>>,
    unchanged: Option<&'a str>,
    result: Option<PreprocessorResult<'a>>,
    pending: VecDeque<Emission<'a>>,
    pub(crate) options: Rc<Options<'static>>,
}

impl<'a> SourceReader<'a> {
    #[cfg(test)]
    pub(crate) fn new(
        input: &'a str,
        file: Option<&Path>,
        options: &Options<'_>,
        warnings: Rc<RefCell<WarningCollection>>,
        arena: &'a Bump,
    ) -> Result<Self, Error> {
        let normalized = Preprocessor::normalize(input);
        let normalized = match normalized {
            std::borrow::Cow::Borrowed(text) => text,
            std::borrow::Cow::Owned(text) => arena.alloc_str(&text),
        };
        Self::from_normalized(
            normalized,
            file,
            options.clone().into_static(),
            warnings,
            arena,
        )
    }

    /// The caller owns normalization so its retained source can be borrowed
    /// directly. Normalization removes one trailing newline and must run once.
    pub(crate) fn from_normalized(
        normalized: &'a str,
        file: Option<&Path>,
        options: Options<'static>,
        warnings: Rc<RefCell<WarningCollection>>,
        arena: &'a Bump,
    ) -> Result<Self, Error> {
        let mut options = Rc::new(options);
        let origin = match file {
            Some(file) => SourceOrigin::entry_file(file, options.base_dir.as_deref())?,
            None => SourceOrigin::memory(options.base_dir.as_deref()),
        };
        let mut origins = Vec::new();
        let mut text = Cow::Borrowed(normalized);
        if let Some(registry) = &options.extensions {
            let context = SourceProcessorContext {
                file,
                line: 1,
                offset: 0,
                attributes: &options.document_attributes,
                warnings: &warnings,
            };
            for mut processor in registry.preprocessors() {
                let edits = processor
                    .process(&text, &context)
                    .map_err(|message| context.error(message))?;
                if !edits.is_empty() {
                    let (updated, mapped) = apply_edits(&text, &origins, edits)
                        .map_err(|message| context.error(message))?;
                    text = Cow::Owned(updated);
                    origins = mapped;
                }
            }
        }
        let includes = options
            .extensions
            .as_ref()
            .and_then(crate::extensions::Registry::include_processors);
        let preprocessor = Preprocessor {
            warnings,
            include_context: super::IncludeContext::root(&options, includes),
        };
        let normalized = match text {
            Cow::Borrowed(text) => text,
            Cow::Owned(text) => arena.alloc_str(&text),
        };
        let front_matter = options
            .document_attributes
            .contains_key("skip-front-matter")
            .then(|| skim_front_matter(normalized.lines()))
            .flatten();
        // A committed callback can change an attribute used by a later style.
        // That can expose directives previously hidden in a comment block, so
        // those sources must keep the scanner resumable even if today's values
        // would allow the complete input to pass through unchanged.
        if origins.is_empty()
            && front_matter.is_none()
            && !has_attribute_driven_styles(normalized)
            && let Some(scanner) = preprocessor.try_pass_through(
                normalized,
                crate::grammar::setext::is_enabled(options.compatibility_mode),
                &options,
            )
        {
            return Ok(Self {
                cursor: None,
                unchanged: Some(normalized),
                result: Some(PreprocessorResult {
                    text: Cow::Borrowed(normalized),
                    leveloffset_ranges: Preprocessor::initial_leveloffset_ranges(
                        normalized.len(),
                        &options,
                    ),
                    leveloffset_scopes: Vec::new(),
                    source_ranges: Vec::new(),
                    included_files: HashSet::default(),
                    conditionals: Vec::new(),
                    front_matter: None,
                    comment_context: scanner.block_context(),
                }),
                pending: VecDeque::new(),
                options,
            });
        }
        let cursor = InputCursor::new(
            preprocessor,
            normalized,
            Some(origin),
            Rc::make_mut(&mut options),
            origins,
            front_matter,
            arena,
        );
        Ok(Self {
            cursor: Some(cursor),
            unchanged: None,
            result: None,
            pending: VecDeque::new(),
            options,
        })
    }

    pub(crate) fn table_header_lookahead(&self) -> Vec<&'a str> {
        self.cursor
            .as_ref()
            .map_or_else(Vec::new, InputCursor::table_header_lookahead)
    }

    pub(crate) fn options_mut(&mut self) -> &mut Options<'static> {
        Rc::make_mut(&mut self.options)
    }

    /// The preprocessor has proved that this complete source needs no edits.
    /// No reader advancement is needed while its committed blocks execute.
    pub(crate) fn pass_through_input(&self) -> Option<&'a str> {
        self.unchanged
    }

    pub(crate) fn take_pass_through(&mut self) -> Option<PreprocessorResult<'a>> {
        self.unchanged.take()?;
        self.result.take()
    }

    pub(crate) fn next(&mut self) -> Result<Option<SourceChunk<'a>>, Error> {
        if let Some(text) = self.unchanged.take() {
            return Ok((!text.is_empty()).then_some(SourceChunk {
                start: 0,
                text,
                source_ranges: Vec::new(),
                leveloffset_ranges: Vec::new(),
                leveloffset_scopes: Vec::new(),
            }));
        }
        loop {
            if let Some(emission) = self.pending.pop_front() {
                return Ok(Some(SourceChunk {
                    start: emission.start,
                    text: emission.text,
                    source_ranges: emission.source_ranges,
                    leveloffset_ranges: emission.leveloffset_ranges,
                    leveloffset_scopes: emission.leveloffset_scopes,
                }));
            }
            let Some(cursor) = &mut self.cursor else {
                return Ok(None);
            };
            match cursor.advance_emitted(Rc::make_mut(&mut self.options))? {
                Advance::Progress(emissions) => {
                    self.pending.extend(emissions);
                    self.finish_if_exhausted()?;
                }
                Advance::End => return Ok(None),
            }
        }
    }

    fn finish_if_exhausted(&mut self) -> Result<(), Error> {
        if !self
            .cursor
            .as_mut()
            .is_some_and(InputCursor::can_finish_without_reading)
        {
            return Ok(());
        }
        let mut cursor = self
            .cursor
            .take()
            .ok_or_else(|| std::io::Error::other("missing exhausted source cursor"))?;
        while let Advance::Progress(emissions) =
            cursor.advance_emitted(Rc::make_mut(&mut self.options))?
        {
            self.pending.extend(emissions);
        }
        let result = cursor.read_all(Rc::make_mut(&mut self.options))?;
        let end = result.text.len();
        for emission in &mut self.pending {
            emission.text = emission
                .text
                .get(..end.saturating_sub(emission.start).min(emission.text.len()))
                .unwrap_or_default();
            for range in &mut emission.source_ranges {
                range.end_offset = range.end_offset.min(end);
            }
            for range in &mut emission.leveloffset_ranges {
                range.end_offset = range.end_offset.min(end);
            }
            for scope in &mut emission.leveloffset_scopes {
                scope.range.end = scope.range.end.min(end);
            }
        }
        self.pending.retain(|emission| !emission.text.is_empty());
        self.result = Some(result);
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<PreprocessorResult<'a>, Error> {
        if let Some(result) = self.result {
            return Ok(result);
        }
        self.cursor
            .ok_or_else(|| std::io::Error::other("missing source cursor"))?
            .read_all(Rc::make_mut(&mut self.options))
    }
}

/// Compose each line edit with the previous stage's source map. Only an
/// editing processor pays for a new text buffer and line map.
fn apply_edits(
    input: &str,
    origins: &[SourceLineOrigin],
    edits: Vec<SourceEdit>,
) -> Result<(String, Vec<SourceLineOrigin>), String> {
    let mut edits = edits.into_iter().peekable();
    let mut output = String::with_capacity(input.len());
    let mut mapped = Vec::new();
    let mut offset = 0;
    let mut emit = |line: &str, origin| {
        if !mapped.is_empty() {
            output.push('\n');
        }
        output.push_str(line);
        mapped.push(origin);
    };
    for (index, line) in input.split('\n').enumerate() {
        let number = index + 1;
        let origin = origins
            .get(index)
            .copied()
            .unwrap_or_else(|| SourceLineOrigin::one_to_one(number, offset));
        offset += line.len() + 1;
        match edits.next_if(|edit| edit.line() == number) {
            None => emit(line, origin),
            Some(SourceEdit::Remove(_)) => {}
            Some(SourceEdit::Replace { text, .. }) => {
                for replacement in text.split('\n') {
                    emit(
                        replacement.trim_end(),
                        SourceLineOrigin {
                            column_shift: None,
                            ..origin
                        },
                    );
                }
            }
        }
    }
    if let Some(edit) = edits.next() {
        return Err(format!(
            "invalid source edit at line {}: edits must address existing lines in ascending order",
            edit.line()
        ));
    }
    Ok((output, mapped))
}

#[cfg(test)]
mod tests;
