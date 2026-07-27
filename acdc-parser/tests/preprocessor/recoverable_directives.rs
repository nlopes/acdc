use std::{
    error::Error,
    fs,
    io::{self, Cursor, Read},
};

use acdc_parser::{
    CompatibilityMode, IncludeLoader, IncludeSource, IncludeSourceError, IncludeSourceErrorKind,
    IncludeSourceProvider, IncludeSourceTarget, Options, ParseResult, WarningKind, parse,
    parse_file, parse_from_reader,
};
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn Error>>;

fn assert_recovery(parsed: &ParseResult, message: &str, line: u32) -> TestResult {
    let warning = parsed
        .warnings()
        .iter()
        .find(|warning| warning.kind.to_string().contains(message))
        .ok_or_else(|| format!("missing {message:?} in {:?}", parsed.warnings()))?;
    assert!(matches!(warning.kind, WarningKind::ContentRecovery { .. }));
    assert_eq!(
        warning
            .source_location()
            .ok_or("missing warning location")?
            .location
            .start
            .line,
        line
    );
    assert!(parsed.source_recovery().is_some());
    Ok(())
}

#[rstest::rstest]
#[case("bogus=1,lines=2", false)]
#[case("indent=nope,lines=2", true)]
#[case("indent=4097,lines=2", true)]
fn include_options_recover_for_all_entry_points(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] attributes: &str,
    #[case] warns: bool,
) -> TestResult {
    let directory = tempdir()?;
    fs::write(
        directory.path().join("part.adoc"),
        "NOT SELECTED\nSelected.\nNOT SELECTED",
    )?;
    let source = format!("Before.\n\ninclude::part.adoc[{attributes}]\n\nAfter.");
    let entry = directory.path().join("main.adoc");
    fs::write(&entry, &source)?;
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .with_base_dir(directory.path())
        .build()?;
    for parsed in [
        parse(&source, &options)?,
        parse_file(&entry, &options)?,
        parse_from_reader(Cursor::new(&source), &options)?,
    ] {
        assert_eq!(
            parsed.preprocessed_source(),
            "Before.\n\nSelected.\n\nAfter."
        );
        if warns {
            assert_eq!(parsed.warnings().len(), 1);
            assert_recovery(&parsed, "ignoring", 3)?;
        } else {
            assert_eq!(parsed.warnings(), []);
        }
    }
    Ok(())
}

struct DenyRetrieval;
impl IncludeSourceProvider for DenyRetrieval {
    fn open(&self, _: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        Err(IncludeSourceError::new(
            IncludeSourceErrorKind::Fatal,
            "must not retrieve invalid include",
        ))
    }
}

#[rstest::rstest]
fn invalid_include_selection_syntax_and_urls_never_read_targets(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "include::part.adoc[lines=nope]",
        "include::part.adoc[lines=1..x]",
        "include::part.adoc[lines=1..2..3]",
        "include::https://[]"
    )]
    directive: &str,
) -> TestResult {
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .with_include_loader(IncludeLoader::custom(DenyRetrieval))
        .build()?;
    let source = format!("Before.\n\n{directive}\n\nAfter.");
    let parsed = parse(&source, &options)?;
    assert_eq!(parsed.preprocessed_source(), source);
    assert_recovery(&parsed, "invalid include directive", 3)?;
    Ok(())
}

#[rstest::rstest]
fn nested_recovery_retains_original_file_and_line(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let directory = tempdir()?;
    let parent = directory.path().join("parent.adoc");
    let child = directory.path().join("child.adoc");
    fs::write(&parent, "Parent.\n\ninclude::child.adoc[lines=2..4]")?;
    fs::write(
        &child,
        "Not selected.\nChild.\n\ninclude::part.adoc[indent=nope,lines=2]",
    )?;
    fs::write(
        directory.path().join("part.adoc"),
        "Not selected.\nGrandchild.",
    )?;
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .with_base_dir(directory.path())
        .build()?;
    let parsed = parse("Before.\n\ninclude::parent.adoc[]\n\nAfter.", &options)?;
    assert_eq!(
        parsed.preprocessed_source(),
        "Before.\n\nParent.\n\nChild.\n\nGrandchild.\n\nAfter."
    );
    assert_recovery(&parsed, "invalid include indent", 4)?;
    assert_eq!(
        parsed
            .warnings()
            .first()
            .and_then(|w| w.source_location())
            .and_then(|loc| loc.file.as_deref()),
        Some(child.as_path())
    );
    Ok(())
}

#[rstest::rstest]
fn unevaluable_conditionals_exclude_bodies_and_keep_nesting(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "ifeval::[1 = 1]",
        "ifeval::[1 == \"1\"]",
        "ifdef::a,b+c[]",
        "ifndef::a[",
        "ifeval::[1 == 1"
    )]
    opening: &str,
) -> TestResult {
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .build()?;
    let source = format!(
        "Before.\n\n{opening}\nExcluded.\nifndef::missing[]\ninclude::must-not-open.adoc[lines=nope]\nendif::[]\nendif::[]\n\nAfter."
    );
    let parsed = parse(&source, &options)?;
    assert_eq!(parsed.preprocessed_source(), "Before.\n\n\nAfter.");
    assert_recovery(&parsed, "unevaluable content", 3)?;
    assert_eq!(parsed.warnings().len(), 1);
    let decisions = parsed.conditionals().collect::<Vec<_>>();
    let [outer, inner] = decisions.as_slice() else {
        return Err("expected two conditional decisions".into());
    };
    assert!(!outer.is_active());
    assert_eq!(outer.end_line(), Some(8));
    assert!(!inner.is_active());
    assert_eq!(inner.end_line(), Some(7));
    Ok(())
}

#[rstest::rstest]
fn mismatched_and_malformed_endif_leave_the_frame_open(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("endif::other[]", "endif::missing[broken]")] terminator: &str,
    #[values(true, false)] active: bool,
) -> TestResult {
    let opening = if active {
        "ifndef::missing[]"
    } else {
        "ifdef::missing[]"
    };
    let source = format!("{opening}\nFirst.\n{terminator}\nSecond.\nendif::missing[]\n\nAfter.");
    let parsed = parse(
        &source,
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    assert_eq!(
        parsed.preprocessed_source().contains("First.\nSecond."),
        active
    );
    assert!(parsed.preprocessed_source().ends_with("After."));
    assert_recovery(&parsed, "leaving the current conditional open", 3)?;
    assert_eq!(
        parsed
            .conditionals()
            .next()
            .ok_or("missing conditional")?
            .end_line(),
        Some(5)
    );
    Ok(())
}

struct FailingRead;
impl Read for FailingRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("unreadable"))
    }
}
struct FailingProvider(Option<IncludeSourceErrorKind>);
impl IncludeSourceProvider for FailingProvider {
    fn open(&self, _: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        self.0.map_or_else(
            || Ok(IncludeSource::from_reader(FailingRead)),
            |kind| Err(IncludeSourceError::new(kind, "provider failure")),
        )
    }
}

#[rstest::rstest]
fn optional_local_include_failures_recover_except_explicit_fatal_errors(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    for kind in [
        Some(IncludeSourceErrorKind::NotFound),
        Some(IncludeSourceErrorKind::Unavailable),
        Some(IncludeSourceErrorKind::Unsupported),
        Some(IncludeSourceErrorKind::Fatal),
        None,
    ] {
        let options = Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .with_include_loader(IncludeLoader::custom(FailingProvider(kind)))
            .build()?;
        let result = parse(
            "Before.\n\ninclude::part.adoc[opts=optional]\n\nAfter.",
            &options,
        );
        match kind {
            Some(IncludeSourceErrorKind::Fatal) => {
                assert!(matches!(result, Err(acdc_parser::Error::IncludeSource(_))));
            }
            Some(IncludeSourceErrorKind::NotFound) => assert_eq!(result?.warnings(), []),
            Some(IncludeSourceErrorKind::Unavailable | IncludeSourceErrorKind::Unsupported)
            | None => {
                let parsed = result?;
                assert_recovery(&parsed, "not readable", 3)?;
                assert!(
                    parsed
                        .preprocessed_source()
                        .contains("Unresolved directive")
                );
                assert!(parsed.preprocessed_source().ends_with("After."));
            }
            Some(_) => return Err("unexpected provider kind".into()),
        }
    }
    assert!(matches!(
        parse_from_reader(FailingRead, &Options::default()),
        Err(acdc_parser::Error::Io(_))
    ));
    Ok(())
}

struct UndecodableProvider;
impl IncludeSourceProvider for UndecodableProvider {
    fn open(&self, _: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        Ok(IncludeSource::from_bytes(
            b"Partial include content.\n\xff".to_vec(),
        ))
    }
}

#[rstest::rstest]
fn failed_include_decoding_discards_partial_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("part.adoc", "https://example.com/part.adoc")] target: &str,
    #[values("", "opts=optional")] attributes: &str,
) -> TestResult {
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .with_attribute("allow-uri-read", "")
        .with_include_loader(IncludeLoader::custom(UndecodableProvider))
        .build()?;
    let parsed = parse(
        &format!("Before.\n\ninclude::{target}[{attributes}]\n\nAfter."),
        &options,
    )?;
    assert_recovery(&parsed, "include", 3)?;
    assert!(
        !parsed
            .preprocessed_source()
            .contains("Partial include content.")
    );
    assert!(
        parsed
            .preprocessed_source()
            .contains("Unresolved directive")
    );
    assert!(parsed.preprocessed_source().ends_with("After."));
    Ok(())
}

#[rstest::rstest]
fn malformed_inline_and_nested_inactive_conditionals_keep_following_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse(
        "Before.\nifdef::a,b+c[Excluded inline.]\nAfter inline.\nifdef::missing[]\nifeval::[1 == \"1\"]\nExcluded nested.\nendif::[]\nendif::missing[]\nAfter block.",
        &options,
    )?;
    assert_eq!(
        parsed.preprocessed_source(),
        "Before.\nAfter inline.\nAfter block."
    );
    assert_recovery(&parsed, "invalid conditional", 2)?;
    assert_eq!(parsed.warnings().len(), 1);
    Ok(())
}

#[rstest::rstest]
fn invalid_include_leveloffset_keeps_the_document_heading_context(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let directory = tempdir()?;
    fs::write(
        directory.path().join("part.adoc"),
        "== Included heading\n\nBody.",
    )?;
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .with_base_dir(directory.path())
        .build()?;
    let parsed = parse(
        "= Document\n:leveloffset: +1\n\ninclude::part.adoc[leveloffset=nope]",
        &options,
    )?;
    let Some(acdc_parser::ast::BlockKind::Section(section)) = parsed
        .document()
        .blocks()
        .next()
        .map(acdc_parser::ast::BlockRef::kind)
    else {
        return Err("missing included section".into());
    };
    assert_eq!(section.level(), 2);
    assert_recovery(&parsed, "invalid include leveloffset", 4)?;
    Ok(())
}
