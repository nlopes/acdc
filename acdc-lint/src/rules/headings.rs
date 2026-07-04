use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType, Document, Location};

use crate::LintId;

use super::{
    LintEmitter, SourceLine, delimiter_token, is_block_attribute_line, is_skipped_line,
    root_list_family, source_line_at, split_first_char,
};

pub(crate) fn lint_section_title_style(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    if let Some(header) = &document.header {
        lint_symmetric_title_line(emitter, lines, &header.location);
    }
    lint_section_title_blocks(emitter, &document.blocks, lines);
    lint_setext_title_style(emitter, lines, skipped_lines);
}

fn lint_section_title_blocks(
    emitter: &mut LintEmitter<'_>,
    blocks: &[Block<'_>],
    lines: &[SourceLine<'_>],
) {
    for block in blocks {
        match block {
            Block::Admonition(block) => lint_section_title_blocks(emitter, &block.blocks, lines),
            Block::CalloutList(list) => {
                for item in &list.items {
                    lint_section_title_blocks(emitter, &item.blocks, lines);
                }
            }
            Block::DescriptionList(list) => {
                for item in &list.items {
                    lint_section_title_blocks(emitter, &item.description, lines);
                }
            }
            Block::DelimitedBlock(block) => {
                lint_section_title_delimited_block(emitter, block, lines);
            }
            Block::DiscreteHeader(header) => {
                lint_symmetric_title_line(emitter, lines, &header.location);
            }
            Block::OrderedList(list) => {
                for item in &list.items {
                    lint_section_title_blocks(emitter, &item.blocks, lines);
                }
            }
            Block::Section(section) => {
                lint_symmetric_title_line(emitter, lines, &section.location);
                lint_section_title_blocks(emitter, &section.content, lines);
            }
            Block::UnorderedList(list) => {
                for item in &list.items {
                    lint_section_title_blocks(emitter, &item.blocks, lines);
                }
            }
            Block::Audio(_)
            | Block::Comment(_)
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

fn lint_section_title_delimited_block(
    emitter: &mut LintEmitter<'_>,
    block: &DelimitedBlock<'_>,
    lines: &[SourceLine<'_>],
) {
    match &block.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks) => {
            lint_section_title_blocks(emitter, blocks, lines);
        }
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedStem(_)
        | DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | _ => {}
    }
}

fn lint_symmetric_title_line(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    location: &Location,
) {
    let Some(line) = source_line_at(lines, location.start.line) else {
        return;
    };
    if is_symmetric_atx_title(line.text.trim()) {
        emitter.emit(
            LintId::SectionTitleStyle,
            "section title uses symmetric ATX markers",
            Some("remove the closing title marker".to_string()),
            Some(emitter.point_location(line.number, 1)),
        );
    }
}

fn lint_setext_title_style(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    for pair in lines.windows(2) {
        let [title_line, underline_line] = pair else {
            continue;
        };
        if is_skipped_line(title_line.number, skipped_lines) {
            continue;
        }
        let title = title_line.text.trim();
        let underline = underline_line.text.trim();
        if is_setext_title_pair(title, underline) {
            emitter.emit(
                LintId::SectionTitleStyle,
                "section title uses setext underline style",
                Some("use an asymmetric ATX title such as `== Section`".to_string()),
                Some(emitter.point_location(underline_line.number, 1)),
            );
        }
    }
}

pub(crate) fn lint_document_header(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    lines: &[SourceLine<'_>],
) {
    let Some(header) = document.header.as_ref() else {
        return;
    };
    let header_lines = collect_header_lines(lines);

    if header.authors.is_empty() {
        let line = header_lines.title.map_or_else(
            || line_after(header.location.end.line),
            |line| line.saturating_add(1),
        );
        emitter.emit(
            LintId::DocumentTitleAuthor,
            "document title is missing an author line",
            Some("add an author line immediately after the document title".to_string()),
            Some(emitter.point_location(line, 1)),
        );
        return;
    }

    let has_revision_attribute = document.attributes.contains_key("revnumber")
        || document.attributes.contains_key("revdate")
        || document.attributes.contains_key("revremark");
    if has_revision_attribute || header_lines.revision.is_some() {
        return;
    }

    let line = header_lines.author.map_or_else(
        || line_after(header.location.end.line),
        |line| line.saturating_add(1),
    );
    emitter.emit(
        LintId::DocumentTitleRevision,
        "document header is missing a revision line",
        Some("add a revision line after the author line".to_string()),
        Some(emitter.point_location(line, 1)),
    );
}

fn is_symmetric_atx_title(trimmed: &str) -> bool {
    let Some((marker, count)) = atx_marker(trimmed) else {
        return false;
    };
    let closing_marker = std::iter::repeat_n(marker, count).collect::<String>();
    let suffix = format!(" {closing_marker}");
    trimmed.ends_with(&suffix)
}

fn is_atx_title(trimmed: &str) -> bool {
    atx_marker(trimmed).is_some()
}

fn atx_marker(trimmed: &str) -> Option<(char, usize)> {
    let (marker, rest) = split_first_char(trimmed)?;
    if !matches!(marker, '=' | '#') {
        return None;
    }

    let count = trimmed.chars().take_while(|ch| *ch == marker).count();
    if !(1..=6).contains(&count) {
        return None;
    }

    let rest = rest.trim_start_matches(marker);
    rest.starts_with(char::is_whitespace)
        .then_some((marker, count))
}

fn is_setext_underline(trimmed: &str) -> bool {
    setext_underline_marker(trimmed).is_some()
}

fn is_setext_title_pair(title: &str, underline: &str) -> bool {
    is_setext_title_text(title)
        && is_setext_underline(underline)
        && title.chars().count().abs_diff(underline.chars().count()) <= 2
}

fn setext_underline_marker(trimmed: &str) -> Option<char> {
    let (marker, _) = split_first_char(trimmed)?;
    (matches!(marker, '=' | '-' | '~' | '^' | '+') && trimmed.chars().all(|ch| ch == marker))
        .then_some(marker)
}

fn is_setext_title_text(trimmed: &str) -> bool {
    !(trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with(':')
        || is_block_attribute_line(trimmed)
        || is_atx_title(trimmed)
        || root_list_family(trimmed).is_some()
        || delimiter_token(trimmed).is_some())
}

#[derive(Default)]
struct HeaderLines {
    title: Option<usize>,
    author: Option<usize>,
    revision: Option<usize>,
}

fn collect_header_lines(lines: &[SourceLine<'_>]) -> HeaderLines {
    let mut header = HeaderLines::default();
    let mut iter = lines.iter().peekable();

    while let Some(line) = iter.next() {
        let trimmed = line.text.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || is_block_attribute_line(trimmed) {
            continue;
        }
        if is_atx_title(trimmed) {
            header.title = Some(line.number);
            break;
        }
        if let Some(next) = iter.peek()
            && is_setext_underline(next.text.trim())
        {
            header.title = Some(line.number);
            break;
        }
        break;
    }

    let Some(title_line) = header.title else {
        return header;
    };

    let mut after_author = false;
    for line in lines.iter().filter(|line| line.number > title_line) {
        let trimmed = line.text.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.is_empty() {
            if after_author {
                break;
            }
            continue;
        }
        if !after_author {
            if trimmed.starts_with(':') {
                break;
            }
            header.author = Some(line.number);
            after_author = true;
            continue;
        }
        if looks_like_revision_line(trimmed) {
            header.revision = Some(line.number);
        }
        break;
    }

    header
}

fn looks_like_revision_line(trimmed: &str) -> bool {
    if is_date_like(trimmed) {
        return true;
    }
    let candidate = trimmed.strip_prefix('v').unwrap_or(trimmed);
    candidate
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
        && candidate.contains('.')
}

fn is_date_like(value: &str) -> bool {
    let mut parts = value.split('-');
    let year = parts.next();
    let month = parts.next();
    let day = parts.next();
    parts.next().is_none()
        && year.is_some_and(|part| {
            part.chars().count() == 4 && part.chars().all(|ch| ch.is_ascii_digit())
        })
        && month.is_some_and(|part| {
            part.chars().count() == 2 && part.chars().all(|ch| ch.is_ascii_digit())
        })
        && day.is_some_and(|part| {
            part.chars().count() == 2 && part.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn line_after(line: u32) -> usize {
    usize::try_from(line)
        .unwrap_or(usize::MAX)
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId, LintLevel, LintOptions, LintOverride, LintSelector, Lintable};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn section_title_style_flags_symmetric_titles() -> Result<(), Error> {
        let report = report_for("= Title\n\n== Section ==\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::SectionTitleStyle));
        Ok(())
    }

    #[test]
    fn section_title_style_ignores_symmetric_title_text_inside_listing() -> Result<(), Error> {
        let report = report_for("= Title\n\n----\n== Not Section ==\n----\n")?;

        assert!(!has_lint(&report, LintId::SectionTitleStyle));
        Ok(())
    }

    #[test]
    fn section_title_style_flags_setext_title_pairs() -> Result<(), Error> {
        let report = report_for("= Title\n\nSetext\n^^^^^^\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::SectionTitleStyle));
        Ok(())
    }

    #[test]
    fn document_header_lints_are_opt_in() -> Result<(), Error> {
        let source = "= Title\n\nContent.\n";
        let report = report_for(source)?;
        assert!(!has_lint(&report, LintId::DocumentTitleAuthor));

        let options = LintOptions::new(vec![LintOverride::new(
            LintLevel::Warn,
            LintSelector::Lint(LintId::DocumentTitleAuthor),
        )]);
        let report = source.lint(&options)?;
        assert!(has_lint(&report, LintId::DocumentTitleAuthor));

        Ok(())
    }

    #[test]
    fn document_title_revision_is_opt_in() -> Result<(), Error> {
        let source = "= Title\nAuthor Name\n\nContent.\n";
        let options = LintOptions::new(vec![LintOverride::new(
            LintLevel::Warn,
            LintSelector::Lint(LintId::DocumentTitleRevision),
        )]);
        let report = source.lint(&options)?;

        assert!(has_lint(&report, LintId::DocumentTitleRevision));
        Ok(())
    }
}
