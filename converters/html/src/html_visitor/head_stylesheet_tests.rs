use std::fs;

use acdc_converters_core::{Diagnostics, WarningSource};
use acdc_parser::{AttributeValue, Options, parse};

use super::resolve_custom_css;

type Error = Box<dyn std::error::Error>;

#[test]
fn custom_stylesheet_paths_respect_the_source_directory_and_absolute_paths() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let stylesdir = temp.path().join("css");
    fs::create_dir(&stylesdir)?;
    let stylesheet = stylesdir.join("custom.css");
    let contents = "body { color: purple; }";
    fs::write(&stylesheet, contents)?;
    let source = WarningSource::new("html");
    for (file, directory, source_dir) in [
        ("custom.css".into(), "css".into(), Some(temp.path())),
        ("custom.css".into(), stylesdir.to_string_lossy(), None),
        (
            stylesheet.to_string_lossy(),
            "unused".into(),
            Some(temp.path()),
        ),
    ] {
        let options = Options::builder()
            .with_attribute("stylesheet", AttributeValue::String(file))
            .with_attribute("stylesdir", AttributeValue::String(directory))
            .build()?;
        let parsed = parse("Body.\n", &options)?;
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let actual = resolve_custom_css(
            parsed.document_ref().attributes(),
            source_dir,
            &mut diagnostics,
        );
        assert_eq!(actual.as_deref(), Some(contents));
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    Ok(())
}

#[test]
fn missing_custom_stylesheet_returns_a_structured_fallback_warning() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let options = Options::builder()
        .with_attribute("stylesheet", "missing.css")
        .with_attribute("stylesdir", "css")
        .build()?;
    let parsed = parse("Body.\n", &options)?;
    let source = WarningSource::new("html");
    let mut warnings = Vec::new();
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    assert!(
        resolve_custom_css(
            parsed.document_ref().attributes(),
            Some(temp.path()),
            &mut diagnostics,
        )
        .is_none()
    );
    assert_eq!(warnings.len(), 1);
    let warning = warnings.first().ok_or("expected a warning")?;
    assert!(warning.message.contains("falling back to default"));
    assert!(
        warning.message.contains(
            &temp
                .path()
                .join("css")
                .join("missing.css")
                .display()
                .to_string()
        )
    );
    assert_eq!(warning.advice(), Some(crate::STYLESHEET_ADVICE));
    Ok(())
}

#[test]
fn absent_empty_and_unset_stylesheets_do_not_attempt_a_custom_file_read() -> Result<(), Error> {
    let source = WarningSource::new("html");
    for value in [
        None,
        Some(AttributeValue::String("".into())),
        Some(AttributeValue::Bool(false)),
    ] {
        let mut builder = Options::builder();
        if let Some(value) = value {
            builder = builder.with_attribute("stylesheet", value);
        }
        let parsed = parse("Body.\n", &builder.build()?)?;
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        assert!(
            resolve_custom_css(parsed.document_ref().attributes(), None, &mut diagnostics)
                .is_none()
        );
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    Ok(())
}
