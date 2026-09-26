use acdc_parser::{
    Block, DelimitedBlock, DelimitedBlockType, Document, InlineMacro, InlineNode, Location,
};

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
    let mut locations = Vec::new();
    if inlines
        .iter()
        .any(|node| formatting_content(node).is_some())
    {
        collect_prose_locations(inlines, &mut locations);
    }
    lint_prose_lines(emitter, source_lines_for_range(lines, range), &locations);
}

fn formatting_content<'a, 's>(node: &'a InlineNode<'s>) -> Option<&'a [InlineNode<'s>]> {
    match node {
        InlineNode::BoldText(text) => Some(&text.content),
        InlineNode::ItalicText(text) => Some(&text.content),
        InlineNode::MonospaceText(text) => Some(&text.content),
        InlineNode::HighlightText(text) => Some(&text.content),
        InlineNode::SubscriptText(text) => Some(&text.content),
        InlineNode::SuperscriptText(text) => Some(&text.content),
        InlineNode::CurvedQuotationText(text) => Some(&text.content),
        InlineNode::CurvedApostropheText(text) => Some(&text.content),
        InlineNode::Macro(macro_node) => formatted_link_label(macro_node),
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::CalloutRef(_)
        | _ => None,
    }
}

fn formatted_link_label<'a, 's>(macro_node: &'a InlineMacro<'s>) -> Option<&'a [InlineNode<'s>]> {
    let label = match macro_node {
        InlineMacro::Link(link) => &link.text,
        InlineMacro::Url(link) => &link.text,
        InlineMacro::Mailto(link) => &link.text,
        InlineMacro::CrossReference(link) => &link.text,
        InlineMacro::Autolink(_)
        | InlineMacro::Footnote(_)
        | InlineMacro::Icon(_)
        | InlineMacro::Image(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Button(_)
        | InlineMacro::Menu(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Stem(_)
        | InlineMacro::IndexTerm(_)
        | _ => return None,
    };
    let parent = macro_node.location();
    let is_source_label = label.iter().all(|node| {
        let location = node.location();
        location.start.file == parent.start.file
            && location.end.file == parent.end.file
            && location.absolute_start >= parent.absolute_start
            && location.absolute_end <= parent.absolute_end
    });
    (is_source_label && label.iter().any(|node| formatting_content(node).is_some()))
        .then_some(label.as_slice())
}

fn collect_prose_locations<'a>(inlines: &'a [InlineNode<'_>], locations: &mut Vec<&'a Location>) {
    for inline in inlines {
        if let Some(content) = formatting_content(inline) {
            collect_prose_locations(content, locations);
        } else {
            locations.push(inline.location());
        }
    }
}

fn write_prose_line(line: SourceLine<'_>, locations: &mut &[&Location], text: &mut String) {
    text.clear();
    text.reserve(line.text.len());

    while let Some((location, remaining)) = locations.split_first() {
        if location.start.file.is_some()
            || location.end.file.is_some()
            || usize::try_from(location.end.line).unwrap_or(usize::MAX) < line.number
        {
            *locations = remaining;
        } else {
            break;
        }
    }

    // Columns count Unicode characters. Advancing once also avoids copying overlapping
    // source spans more than once when attributes expand into several inline nodes.
    let mut chars = line.text.chars().enumerate().peekable();
    for location in *locations {
        if location.start.file.is_some() || location.end.file.is_some() {
            continue;
        }
        let start_line = usize::try_from(location.start.line).unwrap_or(usize::MAX);
        let end_line = usize::try_from(location.end.line).unwrap_or(usize::MAX);
        if line.number < start_line {
            break;
        }
        if line.number > end_line {
            continue;
        }

        let start_column = if line.number == start_line {
            usize::try_from(location.start.column.saturating_sub(1)).unwrap_or(usize::MAX)
        } else {
            0
        };
        let end_column = if line.number == end_line {
            usize::try_from(location.end.column).unwrap_or(usize::MAX)
        } else {
            usize::MAX
        };
        while chars
            .next_if(|(column, _)| *column < start_column)
            .is_some()
        {}
        while let Some((_, ch)) = chars.next_if(|(column, _)| *column < end_column) {
            text.push(ch);
        }
    }
}

fn lint_prose_lines(
    emitter: &mut LintEmitter<'_>,
    paragraph: &[SourceLine<'_>],
    mut locations: &[&Location],
) {
    if paragraph.is_empty() {
        return;
    }

    let has_formatting = !locations.is_empty();
    let mut line_text = String::new();
    let boundaries: Vec<_> = paragraph
        .iter()
        .filter_map(|line| {
            let text = if has_formatting {
                write_prose_line(*line, &mut locations, &mut line_text);
                line_text.as_str()
            } else {
                prose_text(line.text)
            };
            if has_formatting && text.trim().is_empty() {
                return None;
            }
            Some((
                line.number,
                sentence_ending_count(text),
                is_colon_lead_in(text),
            ))
        })
        .collect();
    for &(line, count, _) in &boundaries {
        if count > 1 {
            emitter.emit(
                LintId::OneSentencePerLine,
                "multiple sentences on one source line",
                None,
                Some(emitter.point_location(line, 1)),
            );
        }
    }

    let mut open_sentence_line = None;
    for &(line, count, colon_lead_in) in &boundaries {
        if open_sentence_line.is_some() && count > 0 {
            if let Some(open_line) = open_sentence_line {
                emitter.emit(
                    LintId::OneSentencePerLine,
                    "sentence spans multiple source lines",
                    None,
                    Some(emitter.point_location(open_line, 1)),
                );
            }
            return;
        }
        if count == 0 {
            if open_sentence_line.is_none() && colon_lead_in {
                continue;
            }
            open_sentence_line.get_or_insert(line);
        } else {
            open_sentence_line = None;
        }
    }

    if boundaries.len() > 1
        && let Some(line) = open_sentence_line
    {
        emitter.emit(
            LintId::OneSentencePerLine,
            "sentence spans multiple source lines",
            None,
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

fn is_colon_lead_in(text: &str) -> bool {
    text.trim_end().ends_with(':')
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
    fn one_sentence_per_line_allows_colon_lead_in_before_prose() -> Result<(), Error> {
        let report = report_for("= Title\n\nThe supported values are:\nUse `foo` for one mode.\n")?;

        assert!(!has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }

    #[test]
    fn one_sentence_per_line_allows_colon_lead_in_before_list() -> Result<(), Error> {
        let report = report_for("= Title\n\nThe supported values are:\n\n* `foo`\n* `bar`\n")?;

        assert!(!has_lint(&report, LintId::OneSentencePerLine));
        Ok(())
    }

    #[test]
    fn one_sentence_per_line_flags_colon_line_inside_wrapped_sentence() -> Result<(), Error> {
        let report =
            report_for("= Title\n\nThis sentence starts\nwith a lead-in:\nand ends here.\n")?;

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
