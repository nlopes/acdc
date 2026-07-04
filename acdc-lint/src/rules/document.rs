use std::path::Path;

use acdc_parser::{Document, ParseResult, WarningKind};

use crate::LintId;

use super::{LintEmitter, SourceLine, clone_source_location, is_skipped_line, tables};

pub(crate) fn lint_document_extension(emitter: &mut LintEmitter<'_>, path: &Path) {
    let extension = path.extension().and_then(std::ffi::OsStr::to_str);
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("adoc")) {
        return;
    }

    let message = if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("asc")) {
        "prefer the .adoc extension over .asc"
    } else {
        "AsciiDoc files should use the .adoc extension"
    };
    emitter.emit(
        LintId::DocumentExtension,
        message,
        Some("rename the file to use the .adoc extension".to_string()),
        None,
    );
}

pub(crate) fn lint_parser_warnings(emitter: &mut LintEmitter<'_>, parsed: &ParseResult) {
    for warning in parsed.warnings() {
        let Some(lint) = lint_for_warning(&warning.kind) else {
            continue;
        };
        let location = warning.source_location().map(clone_source_location);
        emitter.emit(
            lint,
            warning.kind.to_string(),
            warning.advice().map(ToString::to_string),
            location,
        );
    }
}

fn lint_for_warning(kind: &WarningKind) -> Option<LintId> {
    if let Some(lint) = tables::lint_for_parser_warning(kind) {
        return Some(lint);
    }

    match kind {
        WarningKind::SectionLevelOutOfSequence { .. } => Some(LintId::SectionLevelSequence),
        WarningKind::UnterminatedDelimitedBlock { .. } => Some(LintId::UnterminatedDelimitedBlock),
        WarningKind::UnterminatedTable { .. } => Some(LintId::UnterminatedTable),
        WarningKind::Other(message)
            if message.contains("Counters (") && message.contains("not supported") =>
        {
            Some(LintId::CounterSyntax)
        }
        WarningKind::NonStandardAuthorLine { .. }
        | WarningKind::UnresolvedReference { .. }
        | WarningKind::LegacyFloatDiscreteHeading
        | WarningKind::Other(_)
        | _ => None,
    }
}

pub(crate) fn lint_multiple_document_title(
    emitter: &mut LintEmitter<'_>,
    document: &Document<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    if document
        .attributes
        .get_string("doctype")
        .is_some_and(|doctype| doctype == "book")
    {
        return;
    }

    let Some(header) = &document.header else {
        return;
    };
    let header_line = usize::try_from(header.location.start.line).unwrap_or(usize::MAX);

    for line in lines.iter().filter(|line| line.number > header_line) {
        if is_skipped_line(line.number, skipped_lines) {
            continue;
        }
        let trimmed = line.text.trim_start();
        if is_document_title_line(trimmed) {
            emitter.emit(
                LintId::MultipleDocumentTitle,
                "document contains more than one top-level document title",
                Some("use section titles (`==`) after the document title".to_string()),
                Some(emitter.point_location(line.number, 1)),
            );
        }
    }
}

fn is_document_title_line(trimmed: &str) -> bool {
    trimmed
        .strip_prefix('=')
        .or_else(|| trimmed.strip_prefix('#'))
        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId, LintOptions, Lintable};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn document_extension_warns_for_non_adoc_names() -> Result<(), Error> {
        let path = std::env::temp_dir().join(format!(
            "acdc-lint-{}-document-extension.asc",
            std::process::id()
        ));
        std::fs::write(&path, "= Title\n\nContent.\n")?;
        let report = path.as_path().lint(&LintOptions::default())?;
        let _ = std::fs::remove_file(&path);

        assert!(has_lint(&report, LintId::DocumentExtension));

        let report = "= Title\n\nContent.\n".lint(&LintOptions::default())?;
        assert!(!has_lint(&report, LintId::DocumentExtension));

        Ok(())
    }

    #[test]
    fn parser_section_sequence_warning_is_linted() -> Result<(), Error> {
        let report = report_for("= Title\n\n=== Skipped\n")?;

        assert!(has_lint(&report, LintId::SectionLevelSequence));
        Ok(())
    }

    #[test]
    fn parser_table_warnings_are_linted() -> Result<(), Error> {
        let report = report_for("[format=psv]\n|===\n|a\n|===\n")?;

        assert!(has_lint(&report, LintId::TableUnknownFormat));
        Ok(())
    }

    #[test]
    fn parser_counter_warning_is_linted() -> Result<(), Error> {
        let report = report_for("= Title\n\n{counter:hits}\n")?;

        assert!(has_lint(&report, LintId::CounterSyntax));
        Ok(())
    }

    #[test]
    fn multiple_document_title_is_flagged() -> Result<(), Error> {
        let report = report_for("= Title\n\n= Another\n")?;

        assert!(has_lint(&report, LintId::MultipleDocumentTitle));
        Ok(())
    }
}
