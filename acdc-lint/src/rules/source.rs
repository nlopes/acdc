use std::path::PathBuf;

use acdc_parser::{InlineNode, Location, Position, SourceLocation};

use crate::{LintDiagnostic, LintId, LintLevel, LintOptions, LintReport};

#[derive(Clone, Copy)]
pub(crate) struct SourceLine<'a> {
    pub(crate) number: usize,
    pub(crate) text: &'a str,
}

#[derive(Clone, Copy)]
pub(crate) struct SourceLineRange {
    start: usize,
    end: usize,
}

pub(crate) struct LintEmitter<'a> {
    file: Option<PathBuf>,
    options: &'a LintOptions,
    diagnostics: Vec<LintDiagnostic>,
}

impl<'a> LintEmitter<'a> {
    pub(crate) fn new(file: Option<PathBuf>, options: &'a LintOptions) -> Self {
        Self {
            file,
            options,
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn emit(
        &mut self,
        lint: LintId,
        message: impl Into<String>,
        help: Option<String>,
        location: Option<SourceLocation>,
    ) {
        let level = self.options.level_for(lint);
        if level == LintLevel::Allow {
            return;
        }

        let mut diagnostic = LintDiagnostic::new(lint, level, message);
        if let Some(help) = help {
            diagnostic = diagnostic.with_help(help);
        }
        if let Some(location) = location {
            diagnostic = diagnostic.at(location);
        }
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn point_location(&self, line: usize, column: usize) -> SourceLocation {
        SourceLocation::at_position(
            self.file.clone(),
            Position::from_line_col(line.max(1), column.max(1)),
        )
    }

    pub(crate) fn source_location(&self, location: &Location) -> SourceLocation {
        SourceLocation::at_location(self.file.clone(), location.clone())
    }

    pub(crate) fn finish(self) -> LintReport {
        LintReport::new(self.diagnostics)
    }
}

pub(crate) fn clone_source_location(location: &SourceLocation) -> SourceLocation {
    SourceLocation::at_location(location.file.clone(), location.location.clone())
}

pub(crate) fn collect_lines(source: &str) -> Vec<SourceLine<'_>> {
    source
        .lines()
        .enumerate()
        .map(|(index, text)| SourceLine {
            number: index.saturating_add(1),
            text,
        })
        .collect()
}

pub(crate) fn source_line_at<'a>(lines: &[SourceLine<'a>], line: u32) -> Option<SourceLine<'a>> {
    let line = usize::try_from(line).ok()?;
    lines.get(line.checked_sub(1)?).copied()
}

pub(crate) fn source_lines_for_range<'a>(
    lines: &'a [SourceLine<'a>],
    range: SourceLineRange,
) -> &'a [SourceLine<'a>] {
    if range.start == 0 || range.start > lines.len() {
        return &[];
    }

    let start = range.start.saturating_sub(1);
    let end = range.end.min(lines.len());
    if start >= end {
        return &[];
    }

    lines.get(start..end).unwrap_or(&[])
}

pub(crate) fn line_range_for_location(location: &Location) -> Option<SourceLineRange> {
    if location.start.file.is_some() || location.end.file.is_some() {
        return None;
    }

    let start = usize::try_from(location.start.line).ok()?;
    let end = usize::try_from(location.end.line).ok()?;
    if start == 0 || end == 0 || end < start {
        return None;
    }

    Some(SourceLineRange { start, end })
}

pub(crate) fn line_range_for_inlines(inlines: &[InlineNode<'_>]) -> Option<SourceLineRange> {
    let mut ranges = inlines
        .iter()
        .filter_map(|node| line_range_for_location(node.location()));
    let mut combined = ranges.next()?;

    for range in ranges {
        combined.start = combined.start.min(range.start);
        combined.end = combined.end.max(range.end);
    }

    Some(combined)
}

pub(crate) fn skipped_delimited_lines(lines: &[SourceLine<'_>]) -> Vec<bool> {
    let mut skipped = vec![false; lines.len()];
    let mut active_delimiter: Option<String> = None;

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.text.trim();
        if let Some(skip) = skipped.get_mut(index)
            && (active_delimiter.is_some() || delimiter_token(trimmed).is_some())
        {
            *skip = true;
        }

        if let Some(delimiter) = active_delimiter.as_deref() {
            if trimmed == delimiter {
                active_delimiter = None;
            }
            continue;
        }

        if let Some(delimiter) = delimiter_token(trimmed) {
            active_delimiter = Some(delimiter.to_string());
        }
    }

    skipped
}

pub(crate) fn delimiter_token(trimmed: &str) -> Option<&str> {
    if trimmed.is_empty() {
        return None;
    }

    let backtick_run = leading_run(trimmed, '`');
    if backtick_run >= 3 {
        return trimmed.get(..backtick_run);
    }

    if let Some((first, rest)) = split_first_char(trimmed)
        && matches!(first, '|' | '!' | ',' | ':')
        && rest.chars().count() >= 3
        && rest.chars().all(|ch| ch == '=')
    {
        return Some(trimmed);
    }

    if trimmed == "--" {
        return Some(trimmed);
    }

    let (first, _) = split_first_char(trimmed)?;
    if matches!(first, '/' | '=' | '-' | '.' | '*' | '+' | '_' | '~')
        && trimmed.chars().count() >= 4
        && trimmed.chars().all(|ch| ch == first)
    {
        Some(trimmed)
    } else {
        None
    }
}

pub(crate) fn split_first_char(value: &str) -> Option<(char, &str)> {
    let mut chars = value.chars();
    let first = chars.next()?;
    Some((first, chars.as_str()))
}

pub(crate) fn is_skipped_line(line_number: usize, skipped_lines: &[bool]) -> bool {
    skipped_lines
        .get(line_number.saturating_sub(1))
        .copied()
        .unwrap_or(false)
}

pub(crate) fn is_block_attribute_line(trimmed: &str) -> bool {
    trimmed.starts_with('[') && trimmed.ends_with(']')
}

pub(crate) fn is_list_continuation(trimmed: &str) -> bool {
    trimmed == "+"
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListFamily {
    Description,
    Ordered,
    Unordered,
}

pub(crate) fn root_list_family(line: &str) -> Option<ListFamily> {
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let trimmed = line.trim_start();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let first = parts.next()?;
    let has_text = parts.next().is_some();
    if has_text && (first == "-" || first.chars().all(|ch| ch == '*')) {
        return Some(ListFamily::Unordered);
    }
    if has_text
        && (first.chars().all(|ch| ch == '.')
            || first.strip_suffix('.').is_some_and(|number| {
                !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
            }))
    {
        return Some(ListFamily::Ordered);
    }
    (trimmed.contains("::") || trimmed.contains(";;")).then_some(ListFamily::Description)
}

pub(crate) fn leading_run(value: &str, needle: char) -> usize {
    value
        .chars()
        .take_while(|ch| *ch == needle)
        .map(char::len_utf8)
        .sum()
}
