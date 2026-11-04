use acdc_parser::ast::{BlockKind, BlockRef, DocumentRef, LocationRef, StructuralRef};

use super::{LintEmitter, SourceLine, delimiter_token, is_block_attribute_line, split_first_char};

use crate::LintId;

pub(crate) fn lint_delimited_block_layout(emitter: &mut LintEmitter<'_>, lines: &[SourceLine<'_>]) {
    let mut active_delimiter: Option<String> = None;

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.text.trim();
        let Some(delimiter) = delimiter_token(trimmed) else {
            continue;
        };

        if let Some(active) = active_delimiter.as_deref() {
            if trimmed == active {
                if let Some(next) = lines.get(index.saturating_add(1))
                    && !next.text.trim().is_empty()
                {
                    emitter.emit(
                        LintId::DelimitedBlockTrailingBlankLine,
                        "delimited block should be followed by a blank line",
                        None,
                        Some(emitter.point_location(next.number, 1)),
                    );
                }
                active_delimiter = None;
            }
            continue;
        }

        if should_check_leading_blank(lines, index)
            && let Some(previous) = previous_source_line(lines, index)
            && !previous.text.trim().is_empty()
        {
            emitter.emit(
                LintId::DelimitedBlockLeadingBlankLine,
                "delimited block should be preceded by a blank line",
                None,
                Some(emitter.point_location(line.number, 1)),
            );
        }

        active_delimiter = Some(delimiter.to_string());
    }
}

fn should_check_leading_blank(lines: &[SourceLine<'_>], index: usize) -> bool {
    let Some(previous) = previous_source_line(lines, index) else {
        return false;
    };
    let trimmed = previous.text.trim();
    !(trimmed.is_empty()
        || trimmed.starts_with('.')
        || trimmed.starts_with("[[")
        || trimmed.starts_with("[#")
        || is_block_attribute_line(trimmed))
}

fn previous_source_line<'a>(lines: &'a [SourceLine<'a>], index: usize) -> Option<SourceLine<'a>> {
    index.checked_sub(1).and_then(|idx| lines.get(idx)).copied()
}

pub(crate) fn lint_blocks(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
    lines: &[SourceLine<'_>],
) {
    for node in document.descendants() {
        if let StructuralRef::Block(block) = node
            && let Some(source) = DelimitedBlockSource::from_block(block)
        {
            lint_delimited_block(emitter, source, lines);
        }
    }
}

#[derive(Clone, Copy)]
struct DelimitedBlockSource<'doc> {
    delimiter: &'doc str,
    open: Option<LocationRef<'doc>>,
    close: Option<LocationRef<'doc>>,
    location: LocationRef<'doc>,
}

impl<'doc> DelimitedBlockSource<'doc> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only delimited forms carry delimiter spelling and ranges"
    )]
    fn from_block(block: BlockRef<'doc>) -> Option<Self> {
        macro_rules! source {
            ($node:ident) => {
                Self {
                    delimiter: $node.delimiter(),
                    open: $node.open_delimiter_location(),
                    close: $node.close_delimiter_location(),
                    location: $node.location(),
                }
            };
        }
        Some(match block.kind() {
            BlockKind::Listing(node) => source!(node),
            BlockKind::Literal(node) => source!(node),
            BlockKind::Example(node) => source!(node),
            BlockKind::Open(node) => source!(node),
            BlockKind::Sidebar(node) => source!(node),
            BlockKind::Quote(node) => source!(node),
            BlockKind::Verse(node) => source!(node),
            BlockKind::Passthrough(node) => source!(node),
            BlockKind::DelimitedComment(node) => source!(node),
            BlockKind::Stem(node) => source!(node),
            BlockKind::Table(node) => source!(node),
            _ => return None,
        })
    }
}

fn lint_delimited_block(
    emitter: &mut LintEmitter<'_>,
    block: DelimitedBlockSource<'_>,
    lines: &[SourceLine<'_>],
) {
    if let Some(minimum) = shortest_safe_delimiter_len(block, lines) {
        let actual = block.delimiter.chars().count();
        if actual > minimum {
            let location = block.open.unwrap_or(block.location);
            emitter.emit(
                LintId::DelimitedBlockMinimalDelimiter,
                format!(
                    "delimited block uses too many delimiter characters: `{}` can be shortened to {minimum} characters",
                    block.delimiter
                ),
                None,
                Some(emitter.source_location(location)),
            );
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DelimiterShape {
    Run { character: char, minimum: usize },
    Table { prefix: char, minimum: usize },
}

impl DelimiterShape {
    fn minimum(self) -> usize {
        match self {
            Self::Run { minimum, .. } | Self::Table { minimum, .. } => minimum,
        }
    }

    fn matching_len(self, line: &str) -> Option<usize> {
        match self {
            Self::Run { character, minimum } => {
                let len = line.chars().count();
                (len >= minimum && line.chars().all(|ch| ch == character)).then_some(len)
            }
            Self::Table { prefix, minimum } => {
                let (first, rest) = split_first_char(line)?;
                let len = line.chars().count();
                (first == prefix
                    && len >= minimum
                    && !rest.is_empty()
                    && rest.chars().all(|ch| ch == '='))
                .then_some(len)
            }
        }
    }
}

fn shortest_safe_delimiter_len(
    block: DelimitedBlockSource<'_>,
    lines: &[SourceLine<'_>],
) -> Option<usize> {
    let shape = delimiter_shape(block.delimiter)?;
    let actual = block.delimiter.chars().count();
    (shape.minimum()..=actual).find(|len| !block_body_contains_delimiter_len(block, lines, *len))
}

fn block_body_contains_delimiter_len(
    block: DelimitedBlockSource<'_>,
    lines: &[SourceLine<'_>],
    len: usize,
) -> bool {
    let Some(shape) = delimiter_shape(block.delimiter) else {
        return false;
    };
    block_body_lines(block, lines).any(|line| shape.matching_len(line.text.trim()) == Some(len))
}

fn block_body_lines<'a>(
    block: DelimitedBlockSource<'_>,
    lines: &'a [SourceLine<'a>],
) -> impl Iterator<Item = SourceLine<'a>> + 'a {
    let start_line = block
        .open
        .and_then(location_start_line)
        .and_then(|line| line.checked_add(1));
    let end_line = block
        .close
        .and_then(location_start_line)
        .and_then(|line| line.checked_sub(1))
        .or_else(|| location_end_line(block.location));

    lines.iter().copied().filter(move |line| {
        start_line
            .zip(end_line)
            .is_some_and(|(start, end)| line.number >= start && line.number <= end)
    })
}

fn location_start_line(location: LocationRef<'_>) -> Option<usize> {
    if location.start().include_chain().is_some() {
        return None;
    }
    usize::try_from(location.start().line()).ok()
}

fn location_end_line(location: LocationRef<'_>) -> Option<usize> {
    if location.end().include_chain().is_some() {
        return None;
    }
    usize::try_from(location.end().line()).ok()
}

fn delimiter_shape(delimiter: &str) -> Option<DelimiterShape> {
    let (first, rest) = split_first_char(delimiter)?;
    if first == '`' {
        return None;
    }
    if matches!(first, '|' | '!' | ',' | ':') && rest.chars().all(|ch| ch == '=') {
        return Some(DelimiterShape::Table {
            prefix: first,
            minimum: 4,
        });
    }
    if delimiter == "--" {
        return Some(DelimiterShape::Run {
            character: '-',
            minimum: 2,
        });
    }
    if matches!(first, '/' | '=' | '-' | '.' | '*' | '+' | '_' | '~')
        && delimiter.chars().all(|ch| ch == first)
    {
        Some(DelimiterShape::Run {
            character: first,
            minimum: 4,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn delimited_block_minimal_delimiter_flags_long_fences() -> Result<(), Error> {
        let report = report_for("= Title\n\n=====\nExample.\n=====\n")?;

        assert!(has_lint(&report, LintId::DelimitedBlockMinimalDelimiter));
        Ok(())
    }

    #[test]
    fn delimited_block_minimal_delimiter_allows_required_long_fences() -> Result<(), Error> {
        let report = report_for("= Title\n\n-----\n----\n-----\n")?;

        assert!(!has_lint(&report, LintId::DelimitedBlockMinimalDelimiter));
        Ok(())
    }

    #[test]
    fn delimited_block_minimal_delimiter_accounts_for_body_delimiters() -> Result<(), Error> {
        let report = report_for("= Title\n\n------\n----\n------\n")?;
        let message = report
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.lint() == LintId::DelimitedBlockMinimalDelimiter)
            .map(crate::LintDiagnostic::message);

        assert_eq!(
            message,
            Some(
                "delimited block uses too many delimiter characters: `------` can be shortened to 5 characters"
            )
        );
        Ok(())
    }

    #[test]
    fn delimited_block_layout_flags_missing_leading_blank() -> Result<(), Error> {
        let report = report_for("= Title\n\nParagraph.\n----\ncode\n----\n")?;

        assert!(has_lint(&report, LintId::DelimitedBlockLeadingBlankLine));
        Ok(())
    }

    #[test]
    fn delimited_block_layout_allows_block_title() -> Result<(), Error> {
        let report = report_for("= Title\n\n.Block title\n----\ncode\n----\n")?;

        assert!(!has_lint(&report, LintId::DelimitedBlockLeadingBlankLine));
        Ok(())
    }

    #[test]
    fn delimited_block_layout_flags_missing_trailing_blank() -> Result<(), Error> {
        let report = report_for("= Title\n\n----\ncode\n----\nNext paragraph.\n")?;

        assert!(has_lint(&report, LintId::DelimitedBlockTrailingBlankLine));
        Ok(())
    }
}
