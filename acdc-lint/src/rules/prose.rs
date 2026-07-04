use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType, Document, InlineNode};

use crate::LintId;

use super::{LintEmitter, SourceLine, line_range_for_inlines, source_lines_for_range};

pub(crate) fn lint_one_sentence_per_line(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    lines: &[SourceLine<'_>],
) {
    lint_one_sentence_blocks(emitter, &document.blocks, lines);
}

fn lint_one_sentence_blocks(
    emitter: &mut LintEmitter<'_>,
    blocks: &[Block<'_>],
    lines: &[SourceLine<'_>],
) {
    for block in blocks {
        match block {
            Block::Admonition(block) => lint_one_sentence_blocks(emitter, &block.blocks, lines),
            Block::CalloutList(list) => {
                for item in &list.items {
                    lint_one_sentence_blocks(emitter, &item.blocks, lines);
                }
            }
            Block::DescriptionList(list) => {
                for item in &list.items {
                    lint_one_sentence_blocks(emitter, &item.description, lines);
                }
            }
            Block::DelimitedBlock(block) => {
                lint_one_sentence_delimited_block(emitter, block, lines);
            }
            Block::OrderedList(list) => {
                for item in &list.items {
                    lint_one_sentence_inlines(emitter, &item.principal, lines);
                    lint_one_sentence_blocks(emitter, &item.blocks, lines);
                }
            }
            Block::Paragraph(paragraph) => {
                lint_one_sentence_inlines(emitter, &paragraph.content, lines);
            }
            Block::Section(section) => lint_one_sentence_blocks(emitter, &section.content, lines),
            Block::UnorderedList(list) => {
                for item in &list.items {
                    lint_one_sentence_inlines(emitter, &item.principal, lines);
                    lint_one_sentence_blocks(emitter, &item.blocks, lines);
                }
            }
            Block::Audio(_)
            | Block::Comment(_)
            | Block::DiscreteHeader(_)
            | Block::DocumentAttribute(_)
            | Block::Image(_)
            | Block::PageBreak(_)
            | Block::TableOfContents(_)
            | Block::ThematicBreak(_)
            | Block::Video(_)
            | _ => {}
        }
    }
}

fn lint_one_sentence_delimited_block(
    emitter: &mut LintEmitter<'_>,
    block: &DelimitedBlock<'_>,
    lines: &[SourceLine<'_>],
) {
    match &block.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks) => {
            lint_one_sentence_blocks(emitter, blocks, lines);
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

fn lint_one_sentence_inlines(
    emitter: &mut LintEmitter<'_>,
    inlines: &[InlineNode<'_>],
    lines: &[SourceLine<'_>],
) {
    let Some(range) = line_range_for_inlines(inlines) else {
        return;
    };
    lint_prose_lines(emitter, source_lines_for_range(lines, range));
}

fn lint_prose_lines(emitter: &mut LintEmitter<'_>, paragraph: &[SourceLine<'_>]) {
    if paragraph.is_empty() {
        return;
    }

    for line in paragraph {
        let count = sentence_ending_count(prose_text(line.text));
        if count > 1 {
            emitter.emit(
                LintId::OneSentencePerLine,
                "multiple sentences on one source line",
                Some("write each sentence on its own source line".to_string()),
                Some(emitter.point_location(line.number, 1)),
            );
        }
    }

    let mut open_sentence_line = None;
    for line in paragraph {
        let count = sentence_ending_count(prose_text(line.text));
        if open_sentence_line.is_some() && count > 0 {
            if let Some(open_line) = open_sentence_line {
                emitter.emit(
                    LintId::OneSentencePerLine,
                    "sentence spans multiple source lines",
                    Some("keep each sentence on a single source line".to_string()),
                    Some(emitter.point_location(open_line, 1)),
                );
            }
            return;
        }
        if count == 0 {
            open_sentence_line.get_or_insert(line.number);
        } else {
            open_sentence_line = None;
        }
    }

    if paragraph.len() > 1
        && let Some(line) = open_sentence_line
    {
        emitter.emit(
            LintId::OneSentencePerLine,
            "sentence spans multiple source lines",
            Some("keep each sentence on a single source line".to_string()),
            Some(emitter.point_location(line, 1)),
        );
    }
}

fn prose_text(line: &str) -> &str {
    let trimmed = line.trim_start();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let first = parts.next();
    let rest = parts.next();

    if let (Some(marker), Some(rest)) = (first, rest)
        && is_list_marker_token(marker)
    {
        return rest.trim_start();
    }

    trimmed
}

fn is_list_marker_token(marker: &str) -> bool {
    marker == "-"
        || marker.chars().all(|ch| ch == '*')
        || marker.chars().all(|ch| ch == '.')
        || marker.strip_suffix('.').is_some_and(|number| {
            !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn sentence_ending_count(text: &str) -> usize {
    let mut count = 0;
    let mut previous = None;
    let mut chars = text.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        let next = chars.peek().map(|(_, next)| *next);
        if matches!(ch, '.' | '!' | '?') && is_sentence_boundary(text, index, ch, previous, next) {
            count += 1;
        }
        previous = Some(ch);
    }

    count
}

fn is_sentence_boundary(
    text: &str,
    index: usize,
    ch: char,
    previous: Option<char>,
    next: Option<char>,
) -> bool {
    if ch == '.' && previous == Some('<') && next == Some('>') {
        return false;
    }

    if ch == '.'
        && previous.is_some_and(|previous| previous.is_ascii_digit())
        && next.is_some_and(|next| next.is_ascii_digit())
    {
        return false;
    }

    if ch == '.'
        && text
            .get(..index)
            .is_some_and(ends_with_sentence_abbreviation)
    {
        return false;
    }

    let rest = text
        .get(index.saturating_add(ch.len_utf8())..)
        .unwrap_or_default();
    let rest = trim_closing_sentence_punctuation(rest);
    if rest.is_empty() {
        return true;
    }

    let rest = rest.trim_start();
    rest.is_empty()
        || rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn trim_closing_sentence_punctuation(mut rest: &str) -> &str {
    loop {
        let Some(ch) = rest.chars().next() else {
            return rest;
        };
        if !matches!(ch, '"' | '\'' | ')' | ']' | '}' | '>') {
            return rest;
        }
        rest = &rest[ch.len_utf8()..];
    }
}

fn ends_with_sentence_abbreviation(prefix: &str) -> bool {
    let Some(word) = prefix.split_whitespace().last() else {
        return false;
    };
    let word = word.trim_matches(|ch: char| !(ch.is_ascii_alphabetic() || ch == '.'));
    matches!(
        word,
        "Mr" | "Mrs" | "Ms" | "Dr" | "Prof" | "Sr" | "Jr" | "St" | "vs" | "etc" | "e.g" | "i.e"
    ) || (word.chars().count() == 1
        && word
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn one_sentence_per_line_flags_wrapped_sentence() -> Result<(), Error> {
        let report = report_for("= Title\n\nThis sentence wraps\nonto another line.\n")?;

        assert!(has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }

    #[test]
    fn one_sentence_per_line_allows_dot_ordered_list_items() -> Result<(), Error> {
        let report = report_for("= Title\n\n. First item\n. Second item.\n")?;

        assert!(!has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }

    #[test]
    fn one_sentence_per_line_allows_numbered_ordered_list_items() -> Result<(), Error> {
        let report = report_for("= Title\n\n1. First item\n2. Second item.\n")?;

        assert!(!has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }

    #[test]
    fn one_sentence_per_line_ignores_auto_callout_markers() -> Result<(), Error> {
        let report = report_for("= Title\n\nUse the callout <.> marker inside prose.\n")?;

        assert!(!has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }

    #[test]
    fn one_sentence_per_line_allows_quoted_punctuation_continuations() -> Result<(), Error> {
        let report = report_for("= Title\n\nThe command prints \"ok.\" and exits.\n")?;

        assert!(!has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }

    #[test]
    fn one_sentence_per_line_flags_quoted_punctuation_before_new_sentence() -> Result<(), Error> {
        let report = report_for("= Title\n\nThe command prints \"ok.\" Next sentence.\n")?;

        assert!(has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }

    #[test]
    fn one_sentence_per_line_flags_multiple_sentences_on_one_line() -> Result<(), Error> {
        let report = report_for("= Title\n\nThis is one sentence. This is another.\n")?;

        assert!(has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }
}
