use std::fs;

use acdc_parser::{CompatibilityMode, Options, SafeMode, WarningKind, parse, parse_file};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("lines=2..4", "Excluded.\n:compat-mode: λ\n\nIncluded body.\n", 2)]
#[case(
    "tag=warning",
    "Excluded.\n// tag::warning[]\n:compat-mode: λ\n\nIncluded body.\n// end::warning[]\n",
    3
)]
#[case("", "Excluded.\n\n:compat-mode: λ\n\nIncluded body.\n", 3)]
#[case("lines=2..4", "π🚀 excluded.\n:compat-mode: λ\n\nIncluded body.\n", 2)]
fn selected_include_warnings_use_original_byte_offsets(
    #[values(CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] nested: bool,
    #[case] selector: &str,
    #[case] child_source: &str,
    #[case] line: u32,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let child = directory.path().join("child.adoc");
    fs::write(&child, child_source)?;
    let main = directory.path().join("main.adoc");
    let body = if nested {
        fs::write(
            directory.path().join("outer.adoc"),
            format!("Outer.\n\ninclude::child.adoc[{selector}]\n"),
        )?;
        "include::outer.adoc[]\n".to_owned()
    } else {
        format!("include::child.adoc[{selector}]\n")
    };
    fs::write(&main, format!("= Main\n\n{body}"))?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let location = parsed
        .warnings()
        .iter()
        .find(|warning| matches!(warning.kind, WarningKind::UnsupportedCompatMode))
        .and_then(|warning| warning.source_location())
        .ok_or("located warning")?;
    assert_eq!(
        location
            .file
            .as_ref()
            .map(|path| path.canonicalize())
            .transpose()?,
        Some(child.canonicalize()?)
    );
    assert_eq!(location.location.start.line, line);
    assert_eq!(location.location.start.column, 1);
    assert_eq!(
        location.location.absolute_start,
        child_source.find(":compat-mode:").ok_or("entry")?
    );
    assert_eq!(
        location.location.absolute_end,
        child_source.find('λ').ok_or("value")?
    );
    Ok(())
}

#[rstest::rstest]
fn warnings_after_an_include_keep_primary_source_offsets(
    #[values(CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] named: bool,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("child.adoc"), "Included body.\n")?;
    let source = "= Main\n\ninclude::child.adoc[]\n\n:compat-mode: λ\n\nAfter.\n";
    let options = Options::builder()
        .with_base_dir(directory.path())
        .with_safe_mode(SafeMode::Unsafe)
        .with_compatibility_mode(mode)
        .build()?;
    let path = directory.path().join("main.adoc");
    fs::write(&path, source)?;
    let parsed = if named {
        parse_file(&path, &options)?
    } else {
        parse(source, &options)?
    };
    let location = parsed
        .warnings()
        .iter()
        .find(|warning| matches!(warning.kind, WarningKind::UnsupportedCompatMode))
        .and_then(|warning| warning.source_location())
        .ok_or("located warning")?;
    let expected_file = if named {
        Some(path.canonicalize()?)
    } else {
        None
    };
    assert_eq!(
        location
            .file
            .as_ref()
            .map(|path| path.canonicalize())
            .transpose()?,
        expected_file
    );
    assert_eq!(location.location.start.line, 5);
    assert_eq!(
        location.location.absolute_start,
        source.find(":compat-mode:").ok_or("entry")?
    );
    assert_eq!(
        location.location.absolute_end,
        source.find('λ').ok_or("value")?
    );
    Ok(())
}

#[rstest::rstest]
fn reindented_include_warnings_keep_transformed_byte_offsets(
    #[values(CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("child.adoc"),
        "Excluded.\n    :compat-mode: λ\n\n    Included body.\n",
    )?;
    let path = directory.path().join("main.adoc");
    fs::write(
        &path,
        "= Main\n\ninclude::child.adoc[lines=2..4,indent=0]\n",
    )?;
    let parsed = parse_file(
        &path,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let location = parsed
        .warnings()
        .iter()
        .find(|warning| matches!(warning.kind, WarningKind::UnsupportedCompatMode))
        .and_then(|warning| warning.source_location())
        .ok_or("located warning")?;
    assert_eq!(
        (location.location.start.line, location.location.start.column),
        (2, 5)
    );
    assert_eq!(location.location.absolute_start, "= Main\n\n".len());
    assert_eq!(
        location.location.absolute_end,
        "= Main\n\n:compat-mode: ".len()
    );
    Ok(())
}
