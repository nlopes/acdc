use acdc_lint::{
    Error, LintGroup, LintId, LintLevel, LintOptions, LintOverride, LintReport, LintSelector,
    Lintable,
};

const MULTIPLE: &str = "multiple sentences on one source line";
const WRAPPED: &str = "sentence spans multiple source lines";

fn lint_options() -> LintOptions {
    LintOptions::new(vec![
        LintOverride::new(LintLevel::Allow, LintSelector::Group(LintGroup::All)),
        LintOverride::new(
            LintLevel::Deny,
            LintSelector::Lint(LintId::OneSentencePerLine),
        ),
    ])
}

fn assert_diagnostics(source: &str, expected: &[(&str, u32)]) -> Result<(), Error> {
    let report = source.lint(&lint_options())?;
    assert_report_diagnostics(&report, source, expected);
    Ok(())
}

fn assert_report_diagnostics(report: &LintReport, source: &str, expected: &[(&str, u32)]) {
    let actual = report
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.lint(), LintId::OneSentencePerLine);
            assert_eq!(diagnostic.level(), LintLevel::Deny);
            (
                diagnostic.message(),
                diagnostic
                    .location()
                    .map(|location| (location.location.start.line, location.location.start.column)),
            )
        })
        .collect::<Vec<_>>();
    let expected = expected
        .iter()
        .map(|&(message, line)| (message, Some((line, 1))))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "source:\n{source}");
    assert_eq!(
        report.has_errors(),
        !expected.is_empty(),
        "source:\n{source}"
    );
}

#[test]
fn one_sentence_per_line_allows_formatted_sentence_endings() -> Result<(), Error> {
    for (open, close) in [
        ("*", "*"),
        ("_", "_"),
        ("`", "`"),
        ("#", "#"),
        ("**", "**"),
        ("__", "__"),
        ("``", "``"),
        ("##", "##"),
        ("*_", "_*"),
        ("*\"", "\"*"),
        ("\"*", "*\""),
        ("\"`", "`\""),
        ("'`", "`'"),
    ] {
        for punctuation in ['.', '!', '?'] {
            let source =
                format!("= Title\n\n{open}One sentence{punctuation}{close}\nAnother sentence.\n");
            assert_diagnostics(&source, &[])?;
        }
    }
    for body in [
        "Text^sup.^\nAnother sentence.",
        "Text~sub.~\nAnother sentence.",
        "*One sentence*.\nAnother sentence.",
        "*Term*: One sentence.\nAnother sentence.",
        "*One sentence.*",
        "*One sentence.*\n\nAnother sentence.",
        "*One sentence.\nAnother sentence.*",
        "*Café sentence.*\nAnother sentence.",
        "Élan *one sentence.*\nAnother sentence.",
        "*One café.*\nAnother sentence.",
        "[.Mr.Smith]*One sentence.*\nAnother sentence.",
        "*One sentence.\n[.small]_Another sentence._*\nThird sentence.",
        "**One sentence.\n**\nAnother sentence.",
        "**One sentence.\n**",
        "**Word\n**",
    ] {
        assert_diagnostics(&format!("= Title\n\n{body}\n"), &[])?;
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_allows_formatted_list_sentences() -> Result<(), Error> {
    for marker in ["*", "-", ".", "1."] {
        for sentence in ["*One sentence.*", "_One sentence._", "`One sentence.`"] {
            let source = format!(
                "= Title\n\n{marker} {sentence}\n  Another sentence.\n  A third sentence.\n"
            );
            assert_diagnostics(&source, &[])?;
        }
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_flags_multiple_formatted_sentences() -> Result<(), Error> {
    for body in [
        "*One sentence.* Another sentence.",
        "One sentence. *Another sentence.*",
        "*One sentence.* *Another sentence.*",
        "One sentence. *Another sentence*.",
        "One sentence. _Another sentence_.",
        "One sentence. `Another sentence`.",
        "One sentence. #Another sentence#.",
        "One sentence. **Another sentence**.",
        "One sentence. __Another sentence__.",
        "One sentence. ``Another sentence``.",
        "One sentence. ##Another sentence##.",
        "One sentence. [small]*Another sentence*.",
        "One sentence. \"`Another sentence`\".",
        "One sentence. '`Another sentence`'.",
        "*_One sentence._* *_Another sentence._*",
        "*One sentence!* _Another sentence?_",
        "Text^sup.^ Another sentence.",
        "Text~sub.~ Another sentence.",
    ] {
        assert_diagnostics(&format!("= Title\n\n{body}\n"), &[(MULTIPLE, 3)])?;
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_reports_each_formatted_violation_at_its_source_line() -> Result<(), Error>
{
    assert_diagnostics(
        "= Title\n\nOne sentence. *Another sentence.*\nThird sentence. _Fourth sentence._\n",
        &[(MULTIPLE, 3), (MULTIPLE, 4)],
    )?;
    for marker in ["*", "-", ".", "1."] {
        assert_diagnostics(
            &format!("= Title\n\n{marker} *One sentence.* Another sentence.\n"),
            &[(MULTIPLE, 3)],
        )?;
        assert_diagnostics(
            &format!(
                "= Title\n\n{marker} *One sentence.*\n  Another sentence. *Third sentence.*\n"
            ),
            &[(MULTIPLE, 4)],
        )?;
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_allows_formatted_colon_lead_ins() -> Result<(), Error> {
    for lead_in in [
        "The supported values are:",
        "*The supported values are:*",
        "_The supported values are:_",
        "`The supported values are:`",
        "#The supported values are:#",
        "**The supported values are:**",
        "*_The supported values are:_*",
    ] {
        assert_diagnostics(
            &format!("= Title\n\n{lead_in}\nUse `foo` for one mode.\n"),
            &[],
        )?;
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_flags_wrapped_formatted_sentences() -> Result<(), Error> {
    for body in [
        "One sentence wraps\nonto this line.",
        "*One sentence wraps\nonto this line.*",
        "_One sentence wraps\nonto this line._",
        "*One sentence wraps*\nonto this line.",
        "One sentence wraps\n*onto this line.*",
        "This sentence starts\n*with a lead-in:*\nand ends here.",
    ] {
        assert_diagnostics(&format!("= Title\n\n{body}\n"), &[(WRAPPED, 3)])?;
    }
    assert_diagnostics(
        "= Title\n\n*One sentence.*\nThe next sentence wraps\nonto another line.\n",
        &[(WRAPPED, 4)],
    )?;
    assert_diagnostics(
        "= Title\n\n* *One sentence wraps\n  onto another line.*\n",
        &[(WRAPPED, 3)],
    )?;
    Ok(())
}

#[test]
fn one_sentence_per_line_preserves_literal_formatting_characters() -> Result<(), Error> {
    for body in [
        "The glob file.*TXT matches\ntext files.",
        "The glob file.\\*TXT matches\ntext files.",
        "The glob file.**TXT matches\ntext files.",
        "The glob *file.*TXT* matches\ntext files.",
        "\\*One sentence.\\*\nAnother sentence.",
        "_*One sentence.*_\nAnother sentence.",
    ] {
        assert_diagnostics(&format!("= Title\n\n{body}\n"), &[(WRAPPED, 3)])?;
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_preserves_non_boundary_punctuation() -> Result<(), Error> {
    for body in [
        "Ask *Dr.* Smith.\nAnother sentence.",
        "Ask _Mr._ Smith.\nAnother sentence.",
        "The value is *3.14*.\nAnother sentence.",
        "Use the callout <.> marker inside prose.\nAnother sentence.",
        "The command prints *ok.* and exits.\nAnother sentence.",
        "The command prints *ok.* _and exits_.\nAnother sentence.",
        "The command prints \"ok.\" and exits.\nAnother sentence.",
        "The command prints *ok!* and exits.\nAnother sentence.",
        "The command prints *ok?* and exits.\nAnother sentence.",
    ] {
        assert_diagnostics(&format!("= Title\n\n{body}\n"), &[])?;
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_checks_formatted_file_input() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("formatted.adoc");
    for (source, expected) in [
        ("= Title\n\n*One sentence.*\nAnother sentence.\n", None),
        (
            "= Title\n\nOne sentence. *Another sentence*.\n",
            Some((MULTIPLE, 3)),
        ),
    ] {
        std::fs::write(&path, source)?;
        let report = path.as_path().lint(&lint_options())?;
        assert_report_diagnostics(&report, source, expected.as_slice());
        for diagnostic in report.diagnostics() {
            assert_eq!(
                diagnostic
                    .location()
                    .and_then(|location| location.file.as_ref()),
                Some(&path),
            );
        }
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_recognizes_formatting_in_link_labels() -> Result<(), Error> {
    assert_diagnostics(
        "= Title\n\nhttps://example.org[*One sentence.*]\nAnother sentence.\n",
        &[],
    )?;
    assert_diagnostics(
        "= Title\n\n<<target,*One sentence.*>>\nAnother sentence.\n\n[[target]]\n== Target\n",
        &[],
    )?;
    assert_diagnostics(
        "= Title\n\nhttps://example.org[*One sentence.*] Another sentence.\n",
        &[(MULTIPLE, 3)],
    )?;
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn one_sentence_per_line_preserves_markers_when_quotes_are_disabled() -> Result<(), Error> {
    for substitutions in ["-quotes", "none", "specialcharacters"] {
        assert_diagnostics(
            &format!("= Title\n\n[subs=\"{substitutions}\"]\n*One sentence.*\nAnother sentence.\n"),
            &[(WRAPPED, 4)],
        )?;
    }
    assert_diagnostics(
        "= Title\n\n[subs=\"-quotes\"]\nThe glob file.*TXT matches\ntext files.\n",
        &[(WRAPPED, 4)],
    )?;
    Ok(())
}

#[cfg(not(feature = "pre-spec-subs"))]
#[test]
fn one_sentence_per_line_ignores_subs_without_pre_spec_subs() -> Result<(), Error> {
    assert_diagnostics(
        "= Title\n\n[subs=\"-quotes\"]\n*One sentence.*\nAnother sentence.\n",
        &[],
    )
}
