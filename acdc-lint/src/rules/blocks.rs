use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType};

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

pub(crate) fn lint_blocks(emitter: &mut LintEmitter<'_>, blocks: &[Block<'_>]) {
    for block in blocks {
        match block {
            Block::Admonition(block) => lint_blocks(emitter, &block.blocks),
            Block::CalloutList(list) => {
                for item in &list.items {
                    lint_blocks(emitter, &item.blocks);
                }
            }
            Block::DescriptionList(list) => {
                for item in &list.items {
                    lint_blocks(emitter, &item.description);
                }
            }
            Block::DelimitedBlock(block) => lint_delimited_block(emitter, block),
            Block::OrderedList(list) => {
                for item in &list.items {
                    lint_blocks(emitter, &item.blocks);
                }
            }
            Block::Section(section) => lint_blocks(emitter, &section.content),
            Block::UnorderedList(list) => {
                for item in &list.items {
                    lint_blocks(emitter, &item.blocks);
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

fn lint_delimited_block(emitter: &mut LintEmitter<'_>, block: &DelimitedBlock<'_>) {
    if let Some(minimum) = minimum_delimiter_len(block.delimiter) {
        let actual = block.delimiter.chars().count();
        if actual > minimum {
            let location = block
                .open_delimiter_location
                .as_ref()
                .unwrap_or(&block.location);
            emitter.emit(
                LintId::DelimitedBlockMinimalDelimiter,
                format!(
                    "delimited block uses `{}` but only {minimum} delimiter characters are needed",
                    block.delimiter
                ),
                None,
                Some(emitter.source_location(location)),
            );
        }
    }

    match &block.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks) => lint_blocks(emitter, blocks),
        DelimitedBlockType::DelimitedTable(table) => {
            for row in table
                .header
                .iter()
                .chain(table.rows.iter())
                .chain(table.footer.iter())
            {
                for column in &row.columns {
                    lint_blocks(emitter, &column.content);
                }
            }
        }
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedStem(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | _ => {}
    }
}

fn minimum_delimiter_len(delimiter: &str) -> Option<usize> {
    let (first, rest) = split_first_char(delimiter)?;
    if first == '`' {
        return None;
    }
    if matches!(first, '|' | '!' | ',' | ':') && rest.chars().all(|ch| ch == '=') {
        return Some(4);
    }
    if delimiter == "--" {
        return Some(2);
    }
    if matches!(first, '/' | '=' | '-' | '.' | '*' | '+' | '_' | '~')
        && delimiter.chars().all(|ch| ch == first)
    {
        return Some(4);
    }
    None
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
