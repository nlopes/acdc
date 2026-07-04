use crate::LintId;

use super::{
    LintEmitter, SourceLine, delimiter_token, is_skipped_line, leading_run, split_first_char,
};

pub(crate) fn lint_markdown_syntax(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    lint_markdown_lines(emitter, lines);
    lint_markdown_tables(emitter, lines, skipped_lines);
}

fn lint_markdown_lines(emitter: &mut LintEmitter<'_>, lines: &[SourceLine<'_>]) {
    let mut active_delimiter: Option<String> = None;

    for line in lines {
        let trimmed = line.text.trim();
        if let Some(delimiter) = active_delimiter.as_deref() {
            if trimmed == delimiter {
                active_delimiter = None;
            }
            continue;
        }

        if let Some(delimiter) = markdown_code_fence(trimmed) {
            emitter.emit(
                LintId::MarkdownCodeFence,
                "Markdown code fence used in AsciiDoc source",
                Some("use an AsciiDoc listing block delimiter such as `----`".to_string()),
                Some(emitter.point_location(line.number, 1)),
            );
            active_delimiter = Some(delimiter.to_string());
            continue;
        }

        if let Some(delimiter) = delimiter_token(trimmed) {
            active_delimiter = Some(delimiter.to_string());
            continue;
        }

        if markdown_heading_marker_len(trimmed).is_some() {
            emitter.emit(
                LintId::MarkdownHeading,
                "Markdown heading marker used in AsciiDoc source",
                Some("use AsciiDoc section markers such as `== Section`".to_string()),
                Some(emitter.point_location(line.number, 1)),
            );
        }

        if let Some(column) = markdown_image_column(line.text) {
            emitter.emit(
                LintId::MarkdownImage,
                "Markdown image syntax used in AsciiDoc source",
                Some("use `image::target[alt]` or `image:target[alt]`".to_string()),
                Some(emitter.point_location(line.number, column)),
            );
        }

        if let Some(column) = markdown_link_column(line.text) {
            emitter.emit(
                LintId::MarkdownLink,
                "Markdown link syntax used in AsciiDoc source",
                Some("use `link:target[text]` or an AsciiDoc URL macro".to_string()),
                Some(emitter.point_location(line.number, column)),
            );
        }
    }
}

fn lint_markdown_tables(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    for pair in lines.windows(2) {
        let [row, separator] = pair else {
            continue;
        };
        if is_skipped_line(row.number, skipped_lines)
            || is_skipped_line(separator.number, skipped_lines)
        {
            continue;
        }
        if row.text.contains('|') && is_markdown_table_separator(separator.text.trim()) {
            emitter.emit(
                LintId::MarkdownTable,
                "Markdown table syntax used in AsciiDoc source",
                Some("use an AsciiDoc table block such as `|===`".to_string()),
                Some(emitter.point_location(separator.number, 1)),
            );
        }
    }
}

fn markdown_code_fence(trimmed: &str) -> Option<&str> {
    let backtick_len = leading_run(trimmed, '`');
    (backtick_len >= 3)
        .then(|| trimmed.get(..backtick_len))
        .flatten()
}

fn markdown_heading_marker_len(trimmed: &str) -> Option<usize> {
    let (marker, _) = split_first_char(trimmed)?;
    if marker != '#' {
        return None;
    }
    let marker_len = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&marker_len) {
        return None;
    }
    trimmed
        .chars()
        .nth(marker_len)
        .is_some_and(char::is_whitespace)
        .then_some(marker_len)
}

fn markdown_image_column(line: &str) -> Option<usize> {
    let start = line.find("![")?;
    has_markdown_destination(line.get(start.saturating_add(2)..)?).then_some(start + 1)
}

fn markdown_link_column(line: &str) -> Option<usize> {
    let mut offset = 0;
    let mut remainder = line;

    while let Some(index) = remainder.find('[') {
        let absolute = offset + index;
        let previous = line
            .get(..absolute)
            .and_then(|prefix| prefix.chars().next_back());
        let after_open = line.get(absolute.saturating_add(1)..)?;
        if previous != Some('!') && has_markdown_destination(after_open) {
            return Some(absolute + 1);
        }

        offset = absolute.saturating_add(1);
        remainder = line.get(offset..)?;
    }

    None
}

fn has_markdown_destination(after_open: &str) -> bool {
    let Some(close_and_open) = after_open.find("](") else {
        return false;
    };
    let Some(destination) = after_open.get(close_and_open.saturating_add(2)..) else {
        return false;
    };
    destination.contains(')')
}

fn is_markdown_table_separator(trimmed: &str) -> bool {
    let trimmed = trimmed.trim_matches('|').trim();
    if !trimmed.contains('|') {
        return false;
    }

    let mut cells = 0usize;
    for cell in trimmed.split('|') {
        if !is_markdown_table_separator_cell(cell.trim()) {
            return false;
        }
        cells = cells.saturating_add(1);
    }
    cells >= 2
}

fn is_markdown_table_separator_cell(cell: &str) -> bool {
    let hyphens = cell.chars().filter(|ch| *ch == '-').count();
    hyphens >= 3 && cell.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn markdown_heading_is_flagged() -> Result<(), Error> {
        let report = report_for("# Title\n\nContent.\n")?;

        assert!(has_lint(&report, LintId::MarkdownHeading));
        Ok(())
    }

    #[test]
    fn markdown_code_fence_is_flagged() -> Result<(), Error> {
        let report = report_for("= Title\n\n```rust\nfn main() {}\n```\n")?;

        assert!(has_lint(&report, LintId::MarkdownCodeFence));
        Ok(())
    }

    #[test]
    fn markdown_link_and_image_are_flagged() -> Result<(), Error> {
        let report = report_for("= Title\n\nSee [docs](https://example.com).\n\n![alt](a.png)\n")?;

        assert!(has_lint(&report, LintId::MarkdownLink));
        assert!(has_lint(&report, LintId::MarkdownImage));
        Ok(())
    }

    #[test]
    fn markdown_table_is_flagged() -> Result<(), Error> {
        let report = report_for("= Title\n\n| A | B |\n| --- | --- |\n| C | D |\n")?;

        assert!(has_lint(&report, LintId::MarkdownTable));
        Ok(())
    }
}
