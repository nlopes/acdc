use std::{
    error::Error,
    fs,
    io::{self, Cursor, Read},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{
    IncludeLoader, IncludeSource, IncludeSourceError, IncludeSourceErrorKind, IncludeSourceTarget,
    Options, Parser, SafeMode, parse, parse_file, parse_from_reader, parse_inline,
};

type TestResult = Result<(), Box<dyn Error>>;

fn assert_no_source_data(lines: &[&str]) -> Result<(), String> {
    if !lines.iter().any(|line| line.contains("acdc_parser::")) {
        return Err("the test did not capture parser tracing".into());
    }
    if lines
        .iter()
        .any(|line| line.to_ascii_uppercase().contains("PRIVATE_"))
    {
        return Err("parser tracing contains a private test value".into());
    }
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn document_traces_omit_source_and_options() -> TestResult {
    let options = Options::builder()
        .with_attribute("caller-secret", "PRIVATE_CALLER_VALUE")
        .build()?;
    let source = "= PRIVATE_TITLE\nPRIVATE_AUTHOR <PRIVATE_EMAIL@example.invalid>\nv1.0, PRIVATE_REVISION\n:private-attribute: PRIVATE_ATTRIBUTE\n\n\
        == PRIVATE_SECTION\n\n\
        A *PRIVATE_BOLD* _PRIVATE_ITALIC_ `PRIVATE_CODE` +PRIVATE_PASS+ paragraph.\n\n\
        * PRIVATE_LIST\n\n\
        PRIVATE_TERM:: PRIVATE_DESCRIPTION\n\n\
        [quote,PRIVATE_ATTRIBUTION,PRIVATE_CITETITLE]\n____\nPRIVATE_QUOTE\n____\n\n\
        [subs=PRIVATE_SUBSTITUTION]\n----\nPRIVATE_LISTING\n----\n\n\
        ifdef::private-attribute[]\n{private-attribute}\nendif::[]\n";
    let parsed = parse(source, &options)?;
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("private-attribute")
            .and_then(|value| value.text()),
        Some("PRIVATE_ATTRIBUTE")
    );
    assert!(logs_contain("input_len="));
    logs_assert(assert_no_source_data);
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn inline_traces_omit_text_labels_and_targets() -> TestResult {
    let source = "PRIVATE_TEXT *PRIVATE_BOLD* footnote:PRIVATE_ID[PRIVATE_FOOTNOTE] \
        https://example.invalid/PRIVATE_URL[PRIVATE_LABEL] \
        mailto:PRIVATE_EMAIL@example.invalid[PRIVATE_EMAIL_LABEL] \
        pass:[PRIVATE_PASS] <<PRIVATE_MISSING,PRIVATE_XREF>> \
        menu:PRIVATE_MENU[PRIVATE_ITEM] btn:[PRIVATE_BUTTON] kbd:[PRIVATE_KEY]";
    let options = Options::builder()
        .with_attribute("experimental", true)
        .build()?;
    assert_ne!(parse_inline(source, &options)?.inlines(), []);
    assert_ne!(Parser::new(source).parse_inline()?.inlines(), []);
    logs_assert(assert_no_source_data);
    Ok(())
}

struct SourceFile(PathBuf);

impl SourceFile {
    fn new(source: &str) -> io::Result<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "acdc-PRIVATE_PATH-{}-{}.adoc",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, source)?;
        Ok(Self(path))
    }
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
#[tracing_test::traced_test]
fn reader_file_and_include_traces_omit_source_details() -> TestResult {
    let source = "PRIVATE_BODY\n\ninclude::PRIVATE_TARGET.adoc[]";
    let file = SourceFile::new(source)?;
    for parsed in [
        parse_from_reader(Cursor::new(source), &Options::default())?,
        parse_file(&file.0, &Options::default())?,
    ] {
        assert!(parsed.source_recovery().is_some());
        assert!(
            parsed
                .warnings()
                .iter()
                .any(|warning| warning.to_string().contains("PRIVATE_TARGET"))
        );
    }

    let provider = |_: &IncludeSourceTarget| -> Result<IncludeSource, IncludeSourceError> {
        Err(IncludeSourceError::new(
            IncludeSourceErrorKind::Unavailable,
            "PRIVATE_PROVIDER_ERROR",
        ))
    };
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_attribute("allow-uri-read", true)
        .with_include_loader(IncludeLoader::custom(provider))
        .build()?;
    let parsed = parse(
        "include::https://example.invalid/PRIVATE_REMOTE[]",
        &options,
    )?;
    assert!(parsed.source_recovery().is_some());
    logs_assert(assert_no_source_data);
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn nested_include_traces_omit_content_and_source_paths() -> TestResult {
    let provider = |target: &IncludeSourceTarget| -> Result<IncludeSource, IncludeSourceError> {
        let first = match target {
            IncludeSourceTarget::File(path) => path.ends_with("PRIVATE_FIRST.adoc"),
            IncludeSourceTarget::Uri(uri) => uri.ends_with("PRIVATE_FIRST.adoc"),
            _ => {
                return Err(IncludeSourceError::new(
                    IncludeSourceErrorKind::Unsupported,
                    "unsupported include target in tracing test",
                ));
            }
        };
        Ok(IncludeSource::from_string(if first {
            "include::PRIVATE_SECOND.adoc[]\n\nPRIVATE_FIRST_BODY\n"
        } else {
            "*PRIVATE_NESTED_BODY* {counter:PRIVATE_NESTED_WARNING}\n"
        }))
    };
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_base_dir(std::env::temp_dir())
        .with_attribute("allow-uri-read", true)
        .with_include_loader(IncludeLoader::custom(provider))
        .build()?;
    for source in [
        "include::PRIVATE_FIRST.adoc[]",
        "include::https://example.invalid/PRIVATE_FIRST.adoc[]",
    ] {
        let parsed = parse(source, &options)?;
        assert!(
            parsed
                .warnings()
                .iter()
                .any(|warning| warning.to_string().contains("PRIVATE_NESTED_WARNING"))
        );
        let block = parsed
            .document()
            .blocks
            .first()
            .ok_or("expected included content")?;
        assert!(block.location().start.file.as_ref().is_some_and(|chain| {
            chain
                .iter()
                .any(|path| path.ends_with("PRIVATE_SECOND.adoc"))
        }));
    }
    assert!(logs_contain("node_count="));
    logs_assert(assert_no_source_data);
    Ok(())
}

#[test]
#[tracing_test::traced_test]
fn manpage_traces_omit_titles_and_filenames() -> TestResult {
    let options = Options::builder()
        .with_attribute("doctype", "manpage")
        .build()?;
    for title in ["PRIVATE_TITLE(1)", "PRIVATE_TITLE(PRIVATE_VOLUME)"] {
        let file = SourceFile::new(&format!("= {title}\n\nPRIVATE_BODY\n"))?;
        let parsed = parse_file(&file.0, &options)?;
        parsed
            .document()
            .blocks
            .first()
            .ok_or("expected manpage body")?;
    }
    assert!(logs_contain("derived manpage attributes from header"));
    assert!(logs_contain("using fallback manpage attributes"));
    logs_assert(assert_no_source_data);
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
#[tracing_test::traced_test]
fn setext_traces_omit_titles() -> TestResult {
    let options = Options::builder().with_setext().build()?;
    let parsed = parse("PRIVATE_TITLE\n=============\n\nPRIVATE_BODY\n", &options)?;
    assert!(parsed.document().header.is_some());
    assert!(logs_contain("Processing setext document title"));
    logs_assert(assert_no_source_data);
    Ok(())
}

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("PRIVATE_READER_ERROR"))
    }
}

#[test]
#[tracing_test::traced_test]
fn failure_traces_omit_diagnostic_payloads() -> TestResult {
    let error = parse_from_reader(FailingReader, &Options::default())
        .err()
        .ok_or("expected reader failure")?;
    assert!(error.to_string().contains("PRIVATE_READER_ERROR"));
    let error = parse("ifeval::[PRIVATE_INVALID_EXPRESSION]", &Options::default())
        .err()
        .ok_or("expected conditional failure")?;
    assert!(error.source_location().is_some());
    assert!(logs_contain("current_offset=0"));
    let parsed = parse("{counter:PRIVATE_COUNTER}", &Options::default())?;
    assert!(
        parsed
            .warnings()
            .iter()
            .any(|warning| warning.to_string().contains("PRIVATE_COUNTER"))
    );
    logs_assert(assert_no_source_data);
    Ok(())
}
