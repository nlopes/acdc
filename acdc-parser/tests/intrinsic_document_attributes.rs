use std::{
    error::Error,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use acdc_parser::{DocumentAttributes, Options, SafeMode, parse, parse_file, parse_from_reader};
use chrono::{DateTime, NaiveDate};

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> std::io::Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acdc-parser-intrinsics-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text<'a>(attributes: &'a DocumentAttributes<'_>, name: &str) -> Option<&'a str> {
    attributes
        .get(name)
        .and_then(|value| value.text())
        .map(acdc_parser::strip_quotes)
}

fn assert_safe_mode(attributes: &DocumentAttributes<'_>, safe_mode: SafeMode) {
    let expected_name = safe_mode.name();
    assert_eq!(
        text(attributes, "safe-mode-level"),
        Some(safe_mode.level().to_string().as_str())
    );
    assert_eq!(text(attributes, "safe-mode-name"), Some(expected_name));

    let conveniences = ["unsafe", "safe", "server", "secure"];
    for name in conveniences {
        assert_eq!(
            attributes.contains_key(&format!("safe-mode-{name}")),
            name == expected_name,
            "safe mode {expected_name}, convenience {name}"
        );
    }

    let expected_home = if safe_mode >= SafeMode::Server {
        ".".into()
    } else {
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
    };
    assert_eq!(text(attributes, "user-home"), Some(expected_home.as_str()));
}

fn assert_local_timestamp(attributes: &DocumentAttributes<'_>) -> TestResult {
    let localdate = text(attributes, "localdate").ok_or("missing localdate")?;
    let localdatetime = text(attributes, "localdatetime").ok_or("missing localdatetime")?;
    let localtime = text(attributes, "localtime").ok_or("missing localtime")?;
    let localyear = text(attributes, "localyear").ok_or("missing localyear")?;

    NaiveDate::parse_from_str(localdate, "%Y-%m-%d")?;
    DateTime::parse_from_str(localdatetime, "%Y-%m-%d %H:%M:%S %z")?;
    DateTime::parse_from_str(&format!("{localdate} {localtime}"), "%Y-%m-%d %H:%M:%S %z")?;
    assert_eq!(localyear, &localdate[..4]);
    Ok(())
}

fn assert_document_timestamp(attributes: &DocumentAttributes<'_>) -> TestResult {
    let docdate = text(attributes, "docdate").ok_or("missing docdate")?;
    let docdatetime = text(attributes, "docdatetime").ok_or("missing docdatetime")?;
    let doctime = text(attributes, "doctime").ok_or("missing doctime")?;
    let docyear = text(attributes, "docyear").ok_or("missing docyear")?;

    NaiveDate::parse_from_str(docdate, "%Y-%m-%d")?;
    DateTime::parse_from_str(docdatetime, "%Y-%m-%d %H:%M:%S %z")?;
    DateTime::parse_from_str(&format!("{docdate} {doctime}"), "%Y-%m-%d %H:%M:%S %z")?;
    assert_eq!(docyear, &docdate[..4]);
    Ok(())
}

#[test]
fn string_and_reader_inputs_initialize_intrinsics_in_all_safe_modes() -> TestResult {
    for safe_mode in [
        SafeMode::Unsafe,
        SafeMode::Safe,
        SafeMode::Server,
        SafeMode::Secure,
    ] {
        let options = Options::builder().with_safe_mode(safe_mode).build()?;
        let string = parse("", &options)?;
        assert_safe_mode(&string.document().attributes, safe_mode);
        assert_local_timestamp(&string.document().attributes)?;
        assert_document_timestamp(&string.document().attributes)?;

        let reader = parse_from_reader(Cursor::new(""), &options)?;
        assert_safe_mode(&reader.document().attributes, safe_mode);
        assert_local_timestamp(&reader.document().attributes)?;
        assert_document_timestamp(&reader.document().attributes)?;
        for name in ["docdir", "docfile", "docfilesuffix", "docname"] {
            assert!(!reader.document().attributes.contains_key(name), "{name}");
        }
    }
    Ok(())
}

#[test]
fn intrinsic_initialization_does_not_restore_an_unset_universal_default() -> TestResult {
    let options = Options::builder()
        .with_attribute("figure-caption", ())
        .build()?;

    let parsed = parse("", &options)?;

    assert!(!parsed.document().attributes.contains_key("figure-caption"));
    assert!(parsed.document().attributes.contains_key("safe-mode-name"));
    Ok(())
}

#[test]
fn string_metadata_overrides_are_retained_but_reader_overrides_are_removed() -> TestResult {
    let options = Options::builder()
        .with_attribute("docdir", "/caller")
        .with_attribute("docfile", "/caller/input.adoc")
        .with_attribute("docfilesuffix", ".adoc")
        .with_attribute("docname", "input")
        .build()?;

    let string = parse("", &options)?;
    assert_eq!(
        text(&string.document().attributes, "docdir"),
        Some("/caller")
    );

    let reader = parse_from_reader(Cursor::new(""), &options)?;
    for name in ["docdir", "docfile", "docfilesuffix", "docname"] {
        assert!(!reader.document().attributes.contains_key(name), "{name}");
    }
    Ok(())
}

#[test]
fn file_input_initializes_source_metadata_and_masks_sensitive_paths() -> TestResult {
    let directory = TempDirectory::new()?;
    let path = directory.0.join("guide.adoc");
    fs::write(&path, "")?;
    let absolute = std::path::absolute(&path)?;

    for safe_mode in [
        SafeMode::Unsafe,
        SafeMode::Safe,
        SafeMode::Server,
        SafeMode::Secure,
    ] {
        let options = Options::builder()
            .with_safe_mode(safe_mode)
            .with_attribute("docfile", "spoof.adoc")
            .build()?;
        let parsed = parse_file(&path, &options)?;
        let attributes = &parsed.document().attributes;
        assert_safe_mode(attributes, safe_mode);
        assert_eq!(text(attributes, "docfilesuffix"), Some(".adoc"));
        assert_eq!(text(attributes, "docname"), Some("guide"));

        if safe_mode >= SafeMode::Server {
            assert_eq!(text(attributes, "docdir"), Some(""));
            assert_eq!(text(attributes, "docfile"), Some("guide.adoc"));
        } else {
            assert_eq!(
                text(attributes, "docdir"),
                absolute.parent().and_then(Path::to_str)
            );
            assert_eq!(text(attributes, "docfile"), absolute.to_str());
        }

        for name in ["docdate", "docdatetime", "doctime", "docyear"] {
            assert!(attributes.contains_key(name), "{name}");
        }
    }
    Ok(())
}

#[test]
fn filetype_override_only_applies_to_string_input() -> TestResult {
    let mut processor = std::collections::HashMap::<
        std::borrow::Cow<'_, str>,
        acdc_parser::AttributeValue<'_>,
    >::new();
    processor.insert("filetype".into(), "html".into());
    processor.insert("filetype-html".into(), "".into());

    let options = Options::builder()
        .with_attribute("filetype", "caller")
        .with_defaults(processor)
        .build()?;

    let string = parse("", &options)?;
    assert_eq!(
        text(&string.document().attributes, "filetype"),
        Some("caller")
    );
    assert!(string.document().attributes.contains_key("filetype-caller"));
    assert!(!string.document().attributes.contains_key("filetype-html"));

    let reader = parse_from_reader(Cursor::new(""), &options)?;
    assert_eq!(
        text(&reader.document().attributes, "filetype"),
        Some("html")
    );
    assert!(reader.document().attributes.contains_key("filetype-html"));

    let directory = TempDirectory::new()?;
    let path = directory.0.join("input.adoc");
    fs::write(&path, "")?;
    let file = parse_file(path, &options)?;
    assert_eq!(text(&file.document().attributes, "filetype"), Some("html"));
    Ok(())
}

#[test]
fn intrinsic_assignments_drive_preprocessing_and_the_final_model_consistently() -> TestResult {
    let parsed = parse(
        ":localyear: 1999\n:outfilesuffix: .custom\n:backend: spoof\n:docname: spoof\n:outfile: spoof\nifdef::localyear[]\n:conditional-year: {localyear}\nendif::[]\nifdef::outfilesuffix[]\n:conditional-suffix: {outfilesuffix}\nendif::[]\nifdef::outfile[]\n:post-conversion-visible:\nendif::[]\n\nText.\n\n:localyear: 2000\n:outfilesuffix!:\n",
        &Options::default(),
    )?;

    let attributes = &parsed.document().attributes;
    assert_eq!(text(attributes, "localyear"), Some("1999"));
    assert_eq!(text(attributes, "conditional-year"), Some("1999"));
    assert_eq!(text(attributes, "outfilesuffix"), Some(".custom"));
    assert_eq!(text(attributes, "conditional-suffix"), Some(".custom"));
    assert_eq!(text(attributes, "backend"), Some("spoof"));
    assert!(!attributes.contains_key("docname"));
    assert!(!attributes.contains_key("outfile"));
    assert!(!attributes.contains_key("post-conversion-visible"));
    Ok(())
}

#[test]
fn source_cannot_spoof_an_inactive_convenience_attribute() -> TestResult {
    let parsed = parse(
        ":safe-mode-secure:\nifdef::safe-mode-secure[]\n:spoofed:\nendif::[]\nifdef::safe-mode-unsafe[]\n:actual:\nendif::[]\n",
        &Options::default(),
    )?;

    assert!(!parsed.document().attributes.contains_key("spoofed"));
    assert!(parsed.document().attributes.contains_key("actual"));
    assert!(
        parsed
            .document()
            .attributes
            .contains_key("safe-mode-unsafe")
    );
    assert!(
        !parsed
            .document()
            .attributes
            .contains_key("safe-mode-secure")
    );
    Ok(())
}

#[test]
fn header_only_intrinsics_reject_body_set_and_unset_entries() -> TestResult {
    for name in [
        "docdate",
        "docdatetime",
        "doctime",
        "docyear",
        "localdate",
        "localdatetime",
        "localtime",
        "localyear",
        "outfilesuffix",
    ] {
        let source = format!(":{name}: header-value\n\nText.\n\n:{name}: body-value\n:{name}!:\n");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(
            text(&parsed.document().attributes, name),
            Some("header-value"),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn included_header_attribute_affects_following_conditionals() -> TestResult {
    let directory = TempDirectory::new()?;
    let main = directory.0.join("main.adoc");
    fs::write(directory.0.join("attrs.adoc"), ":localyear: 1999\n")?;
    fs::write(
        &main,
        "include::attrs.adoc[]\nifdef::localyear[]\n:included-value: {localyear}\nendif::[]\n",
    )?;

    let parsed = parse_file(main, &Options::default())?;
    assert_eq!(
        text(&parsed.document().attributes, "localyear"),
        Some("1999")
    );
    assert_eq!(
        text(&parsed.document().attributes, "included-value"),
        Some("1999")
    );
    Ok(())
}

#[test]
fn post_conversion_attributes_are_not_parser_visible() -> TestResult {
    let options = Options::builder()
        .with_attribute("outdir", "caller-dir")
        .with_attribute("outfile", "caller-file")
        .build()?;
    let parsed = parse(
        ":outdir: document-dir\n:outfile: document-file\nifdef::outfile[]\n:visible:\nendif::[]\n\n{outdir}|{outfile}\n",
        &options,
    )?;

    assert!(!parsed.document().attributes.contains_key("outdir"));
    assert!(!parsed.document().attributes.contains_key("outfile"));
    assert!(!parsed.document().attributes.contains_key("visible"));

    let mut attributes = std::collections::HashMap::<
        std::borrow::Cow<'_, str>,
        acdc_parser::AttributeValue<'_>,
    >::new();
    attributes.insert("outdir".into(), "processor-dir".into());
    attributes.insert("outfile".into(), "processor-file".into());
    let attributes = acdc_parser::Options::builder()
        .with_defaults(attributes)
        .build()?
        .into_document_attributes();
    assert!(!attributes.contains_key("outdir"));
    assert!(!attributes.contains_key("outfile"));
    Ok(())
}

#[test]
fn source_date_epoch_produces_utc_intrinsics() -> TestResult {
    const CHILD: &str = "ACDC_INTRINSIC_EPOCH_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let directory = TempDirectory::new()?;
        let path = directory.0.join("epoch.adoc");
        fs::write(&path, "")?;
        let parsed = parse_file(path, &Options::default())?;
        let attributes = &parsed.document().attributes;
        for prefix in ["local", "doc"] {
            assert_eq!(
                text(attributes, &format!("{prefix}date")),
                Some("1970-01-01")
            );
            assert_eq!(
                text(attributes, &format!("{prefix}datetime")),
                Some("1970-01-01 00:00:00 UTC")
            );
            let time_name = if prefix == "local" {
                "localtime"
            } else {
                "doctime"
            };
            assert_eq!(text(attributes, time_name), Some("00:00:00 UTC"));
            assert_eq!(text(attributes, &format!("{prefix}year")), Some("1970"));
        }
        return Ok(());
    }

    let status = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("source_date_epoch_produces_utc_intrinsics")
        .arg("--nocapture")
        .env(CHILD, "1")
        .env("SOURCE_DATE_EPOCH", "0")
        .status()?;
    assert!(status.success());
    Ok(())
}

#[test]
fn intrinsic_values_do_not_change_json_shape() -> TestResult {
    let parsed = parse("", &Options::default())?;

    assert_eq!(
        serde_json::to_value(&parsed.document().attributes)?,
        serde_json::json!({})
    );
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("safe-mode-level")
            .and_then(|value| value.text()),
        Some("0")
    );
    Ok(())
}
