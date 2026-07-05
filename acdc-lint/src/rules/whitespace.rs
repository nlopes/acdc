use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType, Document, Location};

use crate::LintId;

use super::{LintEmitter, SourceLine};

pub(crate) fn lint_source_whitespace(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    lines: &[SourceLine<'_>],
) {
    let mut blank_run = 0usize;
    let source_block_lines = source_block_body_lines(document, lines);

    for line in lines {
        lint_trailing_whitespace(emitter, line);
        lint_hard_tab(emitter, line);

        if is_source_block_body_line(line.number, &source_block_lines) {
            blank_run = 0;
            continue;
        }

        if line.text.trim().is_empty() {
            blank_run = blank_run.saturating_add(1);
            if blank_run > 1 {
                emitter.emit(
                    LintId::ExcessiveBlankLines,
                    "source contains repeated blank lines",
                    None,
                    Some(emitter.point_location(line.number, 1)),
                );
            }
        } else {
            blank_run = 0;
        }
    }
}

fn source_block_body_lines(document: &Document<'_>, lines: &[SourceLine<'_>]) -> Vec<bool> {
    let mut source_block_lines = vec![false; lines.len()];
    collect_source_block_body_lines(&document.blocks, &mut source_block_lines);
    source_block_lines
}

fn collect_source_block_body_lines(blocks: &[Block<'_>], source_block_lines: &mut [bool]) {
    for block in blocks {
        match block {
            Block::Admonition(block) => {
                collect_source_block_body_lines(&block.blocks, source_block_lines);
            }
            Block::CalloutList(list) => {
                for item in &list.items {
                    collect_source_block_body_lines(&item.blocks, source_block_lines);
                }
            }
            Block::DescriptionList(list) => {
                for item in &list.items {
                    collect_source_block_body_lines(&item.description, source_block_lines);
                }
            }
            Block::DelimitedBlock(block) => {
                collect_source_block_body_lines_for_delimited_block(block, source_block_lines);
            }
            Block::OrderedList(list) => {
                for item in &list.items {
                    collect_source_block_body_lines(&item.blocks, source_block_lines);
                }
            }
            Block::Section(section) => {
                collect_source_block_body_lines(&section.content, source_block_lines);
            }
            Block::UnorderedList(list) => {
                for item in &list.items {
                    collect_source_block_body_lines(&item.blocks, source_block_lines);
                }
            }
            Block::Audio(_)
            | Block::Comment(_)
            | Block::DiscreteHeader(_)
            | Block::DocumentAttribute(_)
            | Block::Image(_)
            | Block::PageBreak(_)
            | Block::Paragraph(_)
            | Block::TableOfContents(_)
            | Block::ThematicBreak(_)
            | Block::Video(_)
            | _ => {}
        }
    }
}

fn collect_source_block_body_lines_for_delimited_block(
    block: &DelimitedBlock<'_>,
    source_block_lines: &mut [bool],
) {
    match &block.inner {
        DelimitedBlockType::DelimitedListing(_) => {
            mark_delimited_block_body(block, source_block_lines);
        }
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks) => {
            collect_source_block_body_lines(blocks, source_block_lines);
        }
        DelimitedBlockType::DelimitedTable(table) => {
            for row in table
                .header
                .iter()
                .chain(table.rows.iter())
                .chain(table.footer.iter())
            {
                for column in &row.columns {
                    collect_source_block_body_lines(&column.content, source_block_lines);
                }
            }
        }
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedStem(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | _ => {}
    }
}

fn mark_delimited_block_body(block: &DelimitedBlock<'_>, source_block_lines: &mut [bool]) {
    let Some(open_line) = block
        .open_delimiter_location
        .as_ref()
        .and_then(location_start_line)
    else {
        return;
    };
    let end_line = block
        .close_delimiter_location
        .as_ref()
        .and_then(location_start_line)
        .and_then(|line| line.checked_sub(1))
        .or_else(|| location_end_line(&block.location));
    let Some(end_line) = end_line else {
        return;
    };
    mark_line_range(source_block_lines, open_line.saturating_add(1), end_line);
}

fn mark_line_range(lines: &mut [bool], start_line: usize, end_line: usize) {
    if start_line == 0 || end_line < start_line {
        return;
    }

    let start = start_line.saturating_sub(1);
    let end = end_line.min(lines.len());
    if start >= end {
        return;
    }

    if let Some(lines) = lines.get_mut(start..end) {
        for line in lines {
            *line = true;
        }
    }
}

fn location_start_line(location: &Location) -> Option<usize> {
    if location.start.file.is_some() {
        return None;
    }
    usize::try_from(location.start.line).ok()
}

fn location_end_line(location: &Location) -> Option<usize> {
    if location.end.file.is_some() {
        return None;
    }
    usize::try_from(location.end.line).ok()
}

fn is_source_block_body_line(line_number: usize, source_block_lines: &[bool]) -> bool {
    source_block_lines
        .get(line_number.saturating_sub(1))
        .copied()
        .unwrap_or(false)
}

fn lint_trailing_whitespace(emitter: &mut LintEmitter<'_>, line: &SourceLine<'_>) {
    if !line.text.ends_with(char::is_whitespace) || line.text.is_empty() {
        return;
    }

    emitter.emit(
        LintId::TrailingWhitespace,
        "source line has trailing whitespace",
        None,
        Some(emitter.point_location(line.number, line.text.chars().count().max(1))),
    );
}

fn lint_hard_tab(emitter: &mut LintEmitter<'_>, line: &SourceLine<'_>) {
    let Some(column) = line.text.chars().position(|ch| ch == '\t') else {
        return;
    };

    emitter.emit(
        LintId::HardTab,
        "source line contains a hard tab",
        None,
        Some(emitter.point_location(line.number, column.saturating_add(1))),
    );
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn trailing_whitespace_is_flagged() -> Result<(), Error> {
        let report = report_for("= Title \n\nContent.\n")?;

        assert!(has_lint(&report, LintId::TrailingWhitespace));
        Ok(())
    }

    #[test]
    fn hard_tabs_are_flagged() -> Result<(), Error> {
        let report = report_for("= Title\n\n\tContent.\n")?;

        assert!(has_lint(&report, LintId::HardTab));
        Ok(())
    }

    #[test]
    fn repeated_blank_lines_are_flagged() -> Result<(), Error> {
        let report = report_for("= Title\n\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::ExcessiveBlankLines));
        Ok(())
    }

    #[test]
    fn repeated_blank_lines_inside_source_blocks_are_not_flagged() -> Result<(), Error> {
        let report = report_for(
            "= Title\n\n[source,rust]\n----\nfn main() {}\n\n\nprintln!(\"done\");\n----\n",
        )?;

        assert!(!has_lint(&report, LintId::ExcessiveBlankLines));
        Ok(())
    }

    #[test]
    fn repeated_blank_lines_inside_listing_blocks_are_not_flagged() -> Result<(), Error> {
        let report =
            report_for("= Title\n\n----\nfirst listing line\n\n\nsecond listing line\n----\n")?;

        assert!(!has_lint(&report, LintId::ExcessiveBlankLines));
        Ok(())
    }

    #[test]
    fn repeated_blank_lines_after_listing_blocks_are_flagged() -> Result<(), Error> {
        let report = report_for(
            "= Title\n\n[source,rust]\n----\nfn main() {}\n\n\nprintln!(\"done\");\n----\n\n\nContent.\n",
        )?;

        assert!(has_lint(&report, LintId::ExcessiveBlankLines));
        Ok(())
    }
}
