use std::path::Path;

use acdc_parser::{ParseResult, WarningKind, ast::DocumentRef};

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
    emitter.emit(LintId::DocumentExtension, message, None, None);
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
        WarningKind::UnsupportedCounter { .. } => Some(LintId::CounterSyntax),
        WarningKind::HeaderOnlyAttribute { .. } => Some(LintId::HeaderOnlyAttribute),
        WarningKind::UnsetAttributeValue { .. } => Some(LintId::UnsetAttributeValue),
        WarningKind::NonStandardAuthorLine { .. }
        | WarningKind::UnresolvedReference { .. }
        | WarningKind::LegacyFloatDiscreteHeading
        | WarningKind::Other(_)
        | _ => None,
    }
}

pub(crate) fn lint_multiple_document_title(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    if matches!(
        document.attributes().get("doctype"),
        Some(value) if value.as_str() == Some("book")
    ) {
        return;
    }

    let Some(header) = document.header() else {
        return;
    };
    let header_line = usize::try_from(header.location().start().line()).unwrap_or(usize::MAX);

    for line in lines.iter().filter(|line| line.number > header_line) {
        if is_skipped_line(line.number, skipped_lines) {
            continue;
        }
        let trimmed = line.text.trim_start();
        if is_document_title_line(trimmed) {
            emitter.emit(
                LintId::MultipleDocumentTitle,
                "document contains more than one top-level document title",
                None,
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
    use crate::{
        Error, LintId, LintLevel, LintOptions, LintOverride, LintReport, LintSelector, Lintable,
    };

    use super::super::test_support::{has_lint, report_for};

    /// The diagnostics for `lint` as `(message, line)`.
    fn diagnostics(report: &LintReport, lint: LintId) -> Vec<(String, Option<u32>)> {
        report
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.lint() == lint)
            .map(|diagnostic| {
                let start_line = diagnostic
                    .location()
                    .map(|source| source.location.start.line);
                (diagnostic.message().to_owned(), start_line)
            })
            .collect()
    }

    const GROUPED: &str = "= User Guide\n:description: Install it.\n\n:doctype: book\n:toc: left\n:sectnums:\n\n== Install\n\nText.\n";

    #[test]
    fn parser_header_only_attribute_warning_is_linted() -> Result<(), Error> {
        let report = report_for(GROUPED)?;
        assert_eq!(
            diagnostics(&report, LintId::HeaderOnlyAttribute),
            [
                (
                    "attribute `doctype` only takes effect in the document header".to_owned(),
                    Some(4)
                ),
                (
                    "attribute `toc` only takes effect in the document header".to_owned(),
                    Some(5)
                ),
            ]
        );
        assert!(report.diagnostics().iter().all(|diagnostic| {
            diagnostic.lint() != LintId::HeaderOnlyAttribute || diagnostic.help().is_some()
        }));

        // A comment line in place of the empty line keeps the entries in the header.
        let report = report_for(&GROUPED.replace(".\n\n:doctype:", ".\n// layout\n:doctype:"))?;
        assert!(!has_lint(&report, LintId::HeaderOnlyAttribute));

        let allowed = LintOptions::new(vec![LintOverride::new(
            LintLevel::Allow,
            LintSelector::Lint(LintId::HeaderOnlyAttribute),
        )]);
        assert!(!has_lint(
            &GROUPED.lint(&allowed)?,
            LintId::HeaderOnlyAttribute
        ));
        Ok(())
    }

    #[test]
    fn parser_unset_attribute_value_warning_is_linted() -> Result<(), Error> {
        let report = report_for("= Title\n\n:!foo: ignored\n\nText.\n")?;
        assert_eq!(
            diagnostics(&report, LintId::UnsetAttributeValue),
            [(
                "unset attribute entry `foo` ignores the text `ignored`".to_owned(),
                Some(3)
            )]
        );

        let report = report_for("= Title\n\n:!foo:\n:bar!:\n\nText.\n")?;
        assert!(!has_lint(&report, LintId::UnsetAttributeValue));
        Ok(())
    }

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

    #[test]
    fn book_doctype_uses_effective_header_value() -> Result<(), Error> {
        let report = report_for("= Title\n:doctype: book\n\n= Part\n")?;

        assert!(!has_lint(&report, LintId::MultipleDocumentTitle));
        Ok(())
    }
}
