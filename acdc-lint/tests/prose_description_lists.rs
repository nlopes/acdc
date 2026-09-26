use acdc_lint::{
    Error, LintGroup, LintId, LintLevel, LintOptions, LintOverride, LintSelector, Lintable,
};

const MULTIPLE: &str = "multiple sentences on one source line";
const WRAPPED: &str = "sentence spans multiple source lines";

fn assert_diagnostics(source: &str, expected: &[(&str, u32)]) -> Result<(), Error> {
    let options = LintOptions::new(vec![
        LintOverride::new(LintLevel::Allow, LintSelector::Group(LintGroup::All)),
        LintOverride::new(
            LintLevel::Deny,
            LintSelector::Lint(LintId::OneSentencePerLine),
        ),
    ]);
    let report = source.lint(&options)?;
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
    Ok(())
}

#[test]
fn one_sentence_per_line_checks_description_list_values() -> Result<(), Error> {
    for separator in ["::", ":::", "::::", ";;"] {
        assert_diagnostics(
            &format!("= Title\n\nTerm{separator} One sentence. Another sentence.\n"),
            &[(MULTIPLE, 3)],
        )?;
        assert_diagnostics(
            &format!("= Title\n\nTerm{separator} One sentence.\nAnother sentence.\n"),
            &[],
        )?;
    }
    assert_diagnostics(
        "= Title\n\nTerm::\nOne sentence. Another sentence.\n",
        &[(MULTIPLE, 4)],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm::   One sentence. Another sentence.\n",
        &[(MULTIPLE, 3)],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm::\n  One sentence. Another sentence.\n",
        &[(MULTIPLE, 4)],
    )?;
    Ok(())
}

#[test]
fn one_sentence_per_line_checks_wrapped_description_list_values() -> Result<(), Error> {
    assert_diagnostics(
        "= Title\n\nTerm:: This sentence wraps\nonto another line.\n",
        &[(WRAPPED, 3)],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm::\nThis sentence wraps\nonto another line.\n",
        &[(WRAPPED, 4)],
    )?;
    Ok(())
}

#[test]
fn one_sentence_per_line_excludes_description_list_terms() -> Result<(), Error> {
    for term in [
        "One term. Another",
        "*One term.* *Another term.*",
        "Élan. Another term",
        "Term ending with a period.",
    ] {
        assert_diagnostics(&format!("= Title\n\n{term}:: One sentence.\n"), &[])?;
        assert_diagnostics(
            &format!("= Title\n\n{term}:: One sentence. Another sentence.\n"),
            &[(MULTIPLE, 3)],
        )?;
    }
    Ok(())
}

#[test]
fn one_sentence_per_line_handles_formatted_description_list_values() -> Result<(), Error> {
    assert_diagnostics(
        "= Title\n\nTerm:: *One sentence.*\n_Another sentence._\n",
        &[],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm:: *One sentence.* _Another sentence._\n",
        &[(MULTIPLE, 3)],
    )?;
    assert_diagnostics(
        "= Title\n\nÉlan. Another term:: *One café.*\nAnother sentence.\n",
        &[],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm:: *This sentence wraps\nonto another line.*\n",
        &[(WRAPPED, 3)],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm:: *The supported values are:*\nUse `foo` for one mode.\n",
        &[],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm:: The glob file.*TXT matches\ntext files.\n",
        &[(WRAPPED, 3)],
    )?;
    Ok(())
}

#[test]
fn one_sentence_per_line_checks_description_list_continuations_once() -> Result<(), Error> {
    assert_diagnostics(
        "= Title\n\nTerm:: One sentence. Another sentence.\n+\nThird sentence. Fourth sentence.\n",
        &[(MULTIPLE, 3), (MULTIPLE, 5)],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm:: One sentence.\n+\nThis sentence wraps\nonto another line.\n",
        &[(WRAPPED, 5)],
    )?;
    Ok(())
}

#[test]
fn one_sentence_per_line_checks_nested_description_lists_once() -> Result<(), Error> {
    assert_diagnostics(
        "= Title\n\nParent:: One sentence. Another sentence.\nChild::: Third sentence. Fourth sentence.\n",
        &[(MULTIPLE, 3), (MULTIPLE, 4)],
    )?;
    assert_diagnostics(
        "= Title\n\nTerm:: One sentence.\n\n* First list sentence. Second list sentence.\n",
        &[(MULTIPLE, 5)],
    )?;
    Ok(())
}

#[test]
fn one_sentence_per_line_allows_empty_and_short_description_list_values() -> Result<(), Error> {
    for body in [
        "Term::",
        "First term::\nSecond term::",
        "Term:: A short value",
        "First term:: One sentence.\nSecond term:: Another sentence.",
        "Parent:: One sentence.\nChild::: Another sentence.",
        "Term::\nOne sentence.",
        "Term::\nOne sentence.\nAnother sentence.",
        "Term::   One sentence.",
    ] {
        assert_diagnostics(&format!("= Title\n\n{body}\n"), &[])?;
    }
    Ok(())
}
