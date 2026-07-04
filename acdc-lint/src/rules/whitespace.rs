use crate::LintId;

use super::{LintEmitter, SourceLine};

pub(crate) fn lint_source_whitespace(emitter: &mut LintEmitter<'_>, lines: &[SourceLine<'_>]) {
    let mut blank_run = 0usize;

    for line in lines {
        lint_trailing_whitespace(emitter, line);
        lint_hard_tab(emitter, line);

        if line.text.trim().is_empty() {
            blank_run = blank_run.saturating_add(1);
            if blank_run > 1 {
                emitter.emit(
                    LintId::ExcessiveBlankLines,
                    "source contains repeated blank lines",
                    Some("keep a single blank line between adjacent blocks".to_string()),
                    Some(emitter.point_location(line.number, 1)),
                );
            }
        } else {
            blank_run = 0;
        }
    }
}

fn lint_trailing_whitespace(emitter: &mut LintEmitter<'_>, line: &SourceLine<'_>) {
    if !line.text.ends_with(char::is_whitespace) || line.text.is_empty() {
        return;
    }

    emitter.emit(
        LintId::TrailingWhitespace,
        "source line has trailing whitespace",
        Some("remove the trailing whitespace".to_string()),
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
        Some("replace the tab with spaces".to_string()),
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
}
