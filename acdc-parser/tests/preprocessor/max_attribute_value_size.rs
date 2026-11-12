use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{
    Options, SafeMode,
    ast::{BlockKind, InlineKind},
    parse, parse_file,
};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDocument {
    directory: PathBuf,
    main: PathBuf,
}

impl TempDocument {
    fn new(source: &str) -> io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "acdc-parser-max-attribute-value-size-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let main = directory.join("main.adoc");
        fs::write(&main, source)?;
        Ok(Self { directory, main })
    }
}

impl Drop for TempDocument {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn paragraph_text(result: &acdc_parser::ParseResult) -> Result<String, Box<dyn Error>> {
    let paragraph = result
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                Some(paragraph)
            } else {
                None
            }
        })
        .ok_or("missing paragraph")?;
    paragraph
        .content()
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(text) = node.kind() {
                Ok(text.text())
            } else if let InlineKind::Raw(text) = node.kind() {
                Ok(text.text())
            } else {
                Err("expected text".into())
            }
        })
        .collect()
}

fn options(limit: &str) -> Result<Options<'_>, acdc_parser::Error> {
    Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .with_attribute("max-attribute-value-size", limit)
        .build()
}

#[test]
fn caller_limit_truncates_at_a_valid_utf8_boundary() -> TestResult {
    let result = parse(":value: ééé\n\n{value}", &options("5")?)?;

    assert_eq!(
        result
            .document()
            .attributes()
            .get("value")
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some("éé")
    );
    assert_eq!(paragraph_text(&result)?, "éé");
    Ok(())
}

#[test]
fn zero_malformed_and_negative_limits_follow_ruby_conversion() -> TestResult {
    for (limit, expected) in [("0", ""), ("junk", ""), ("-3", "abc")] {
        let result = parse(":value: abcdef\n\n{value}", &options(limit)?)?;

        assert_eq!(
            result
                .document()
                .attributes()
                .get("value")
                .and_then(acdc_parser::DocumentAttributeValue::text),
            Some(expected),
            "{limit}"
        );
        assert_eq!(paragraph_text(&result)?, expected, "{limit}");
    }
    Ok(())
}

#[test]
fn secure_mode_defaults_to_4096_bytes() -> TestResult {
    let value = "x".repeat(4_100);
    let source = format!(":value: {value}\n\n{{value}}");
    let options = Options::builder()
        .with_safe_mode(SafeMode::Secure)
        .build()?;

    let result = parse(&source, &options)?;

    assert_eq!(
        result
            .document()
            .attributes()
            .get("value")
            .and_then(acdc_parser::DocumentAttributeValue::text)
            .map(str::len),
        Some(4_096)
    );
    assert_eq!(paragraph_text(&result)?.len(), 4_096);
    Ok(())
}

#[test]
fn caller_limit_remains_locked_against_document_changes() -> TestResult {
    let options = Options::builder()
        .with_attribute("max-attribute-value-size", "4")
        .build()?;
    let result = parse(
        ":max-attribute-value-size: 8\n:value: 123456789\n\n{value}",
        &options,
    )?;

    assert_eq!(
        result
            .document()
            .attributes()
            .get("max-attribute-value-size")
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some("4")
    );
    assert_eq!(
        result
            .document()
            .attributes()
            .get("value")
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some("1234")
    );
    Ok(())
}

#[test]
fn truncated_attribute_values_affect_include_targets() -> TestResult {
    let document = TempDocument::new(":part: abcdef\n\ninclude::{part}.adoc[]")?;
    fs::write(document.directory.join("abcd.adoc"), "INCLUDED")?;

    let result = parse_file(&document.main, &options("4")?)?;

    assert_eq!(paragraph_text(&result)?, "INCLUDED");
    assert_eq!(
        result
            .document()
            .attributes()
            .get("part")
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some("abcd")
    );
    Ok(())
}

#[test]
fn clipped_formatted_values_cannot_restore_their_original_source() -> TestResult {
    let result = parse(
        ":name: Early\n:value: pass:q,a[*{name}*]\n\n{value}",
        &options("5")?,
    )?;
    assert_eq!(
        result
            .document()
            .attributes()
            .get("value")
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some("*Earl")
    );
    assert_eq!(paragraph_text(&result)?, "*Earl");
    Ok(())
}

#[rstest::rstest]
fn presence_valued_limits_apply_zero_and_explicit_unsets_disable_the_limit(
    #[values(SafeMode::Unsafe, SafeMode::Secure)] safe_mode: SafeMode,
    #[values(false, true)] present: bool,
) -> TestResult {
    let options = Options::builder()
        .with_safe_mode(safe_mode)
        .with_attribute("max-attribute-value-size", present)
        .build()?;
    let value = "x".repeat(4100);
    let result = parse(&format!(":value: {value}\n\nValue: {{value}}."), &options)?;
    assert_eq!(
        paragraph_text(&result)?,
        if present {
            "Value: .".to_owned()
        } else {
            format!("Value: {value}.")
        }
    );
    Ok(())
}

#[rstest::rstest]
#[case("4_096", "abcdef")]
#[case("1_0", "abcdef")]
#[case("1__0", "a")]
#[case("_10", "")]
#[case("-4_096", "abcdef")]
fn value_limits_honor_ruby_digit_separators(
    #[case] limit: &str,
    #[case] expected: &str,
) -> TestResult {
    let result = parse(":value: abcdef\n\n{value}", &options(limit)?)?;
    assert_eq!(paragraph_text(&result)?, expected);
    Ok(())
}

#[rstest::rstest]
fn relative_leveloffset_values_are_capped_after_resolution(
    #[values(false, true)] body: bool,
    #[values("+10", "-10")] offset: &str,
) -> TestResult {
    let prefix = if body {
        "= Document\n:leveloffset: 1\n\nBody.\n\n"
    } else {
        ""
    };
    let source = format!("{prefix}:leveloffset: {offset}\n\n== Section");
    let result = parse(&source, &options("1")?)?;
    let section = result
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Section(section) = block.kind() {
                Some(section)
            } else {
                None
            }
        })
        .ok_or("missing section")?;
    assert_eq!(section.level(), if offset.starts_with('+') { 2 } else { 1 });
    Ok(())
}
