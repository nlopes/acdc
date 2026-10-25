#[allow(dead_code)]
#[path = "support/fixtures.rs"]
mod support;

use std::{fs, path::Path};

use acdc_parser::CompatibilityMode;
use support::{Case, Generation, Variant, discover_sources};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn write(root: &Path, name: &str, text: &str) -> Result<(), Error> {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

#[test]
fn discovery_is_sorted_and_excludes_include_inputs() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    for name in [
        "z.adoc",
        "nested/a.adoc",
        "includes/one.adoc",
        "nested/includes/two.adoc",
    ] {
        write(dir.path(), name, "Text.")?;
    }
    let paths = discover_sources(dir.path())?;
    assert_eq!(
        paths,
        [dir.path().join("nested/a.adoc"), dir.path().join("z.adoc")]
    );
    assert!(discover_sources(&dir.path().join("missing")).is_err());
    Ok(())
}

#[test]
fn shared_and_paired_layouts_have_stable_names() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "nested/example.adoc", "Text.")?;
    write(dir.path(), "nested/example.json", "{}")?;
    let source = dir.path().join("nested/example.adoc");
    let shared = Case::load(dir.path(), &source, false)?;
    assert_eq!(shared.name, "nested/example");
    assert_eq!(
        shared
            .expectations
            .first()
            .ok_or("missing expectation")?
            .variant,
        Variant::Shared
    );
    fs::remove_file(source.with_extension("json"))?;
    write(dir.path(), "nested/example.asciidoctor.json", "{}")?;
    write(
        dir.path(),
        "nested/example.draft.error",
        "intentional layout-test placeholder",
    )?;
    let paired = Case::load(dir.path(), &source, false)?;
    let [asciidoctor, draft] = paired.expectations.as_slice() else {
        return Err("missing pair".into());
    };
    assert_eq!(paired.test_name(asciidoctor), "nested/example::asciidoctor");
    assert_eq!(paired.test_name(draft), "nested/example::draft");
    Ok(())
}

#[test]
fn missing_conflicting_and_mixed_expectations_are_errors() -> Result<(), Error> {
    for files in [
        vec![],
        vec!["json", "error"],
        vec!["asciidoctor.json"],
        vec!["draft.json"],
        vec!["json", "asciidoctor.json", "draft.json"],
        vec!["asciidoctor.json", "asciidoctor.error", "draft.json"],
        vec!["diagnostics.json"],
        vec![
            "asciidoctor.json",
            "draft.json",
            "asciidoctor.diagnostics.json",
        ],
        vec!["asciidoctor.json", "draft.json", "diagnostics.json"],
    ] {
        let dir = tempfile::tempdir()?;
        write(dir.path(), "example.adoc", "Text.")?;
        for extension in &files {
            write(dir.path(), &format!("example.{extension}"), "{}")?;
        }
        assert!(
            Case::load(dir.path(), &dir.path().join("example.adoc"), false).is_err(),
            "{files:?}"
        );
    }
    Ok(())
}

#[test]
fn paired_diagnostics_never_fall_back_to_shared_files() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "example.adoc", "Text.")?;
    for suffix in ["asciidoctor", "draft"] {
        write(dir.path(), &format!("example.{suffix}.json"), "{}")?;
        write(
            dir.path(),
            &format!("example.{suffix}.diagnostics.json"),
            "{}",
        )?;
    }
    let case = Case::load(dir.path(), &dir.path().join("example.adoc"), false)?;
    for expected in &case.expectations {
        assert!(expected.diagnostics.is_some());
    }
    assert_eq!(
        case.observe(CompatibilityMode::Asciidoctor)?,
        case.observe(CompatibilityMode::Draft)?
    );
    Ok(())
}

#[test]
fn generation_is_scoped_and_records_diagnostics_and_recovery() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "example.adoc", "----\nUnclosed source.\n")?;
    write(dir.path(), "untouched.adoc", "Other text.")?;
    write(dir.path(), "untouched.json", "leave this alone")?;
    let request = Generation::from_args(["--diagnostics", "example.adoc"])?;
    assert_eq!(request.generate(dir.path())?, 1);
    assert_eq!(
        fs::read_to_string(dir.path().join("untouched.json"))?,
        "leave this alone"
    );
    let case = Case::load(dir.path(), &dir.path().join("example.adoc"), false)?;
    case.verify(case.expectations.first().ok_or("missing expectation")?)?;
    let diagnostics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        dir.path().join("example.diagnostics.json"),
    )?)?;
    assert!(
        diagnostics
            .get("recovery")
            .is_some_and(|value| !value.is_null())
    );
    assert_eq!(
        diagnostics
            .pointer("/warnings/0/location/file")
            .and_then(serde_json::Value::as_str),
        Some("example.adoc")
    );
    assert!(
        diagnostics
            .pointer("/warnings/0/location/absolute_start")
            .is_some_and(serde_json::Value::is_number)
    );
    Ok(())
}

#[test]
fn generation_failure_does_not_write_earlier_cases() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "a.adoc", "Text.")?;
    write(dir.path(), "a.json", "original")?;
    write(dir.path(), "b.adoc", "Also valid.")?;
    write(
        dir.path(),
        "b.error",
        "This case should fail instead of silently switching to JSON.",
    )?;
    let request = Generation::from_args(["a.adoc", "b.adoc"])?;
    assert!(request.generate(dir.path()).is_err());
    assert_eq!(fs::read_to_string(dir.path().join("a.json"))?, "original");
    assert!(!dir.path().join("b.json").exists());
    let request = Generation::from_args(["--pair", "a.adoc", "b.adoc"])?;
    assert!(request.generate(dir.path()).is_err());
    assert_eq!(fs::read_to_string(dir.path().join("a.json"))?, "original");
    assert!(!dir.path().join("a.asciidoctor.json").exists());
    Ok(())
}

#[test]
fn generator_rejects_unknown_inputs_and_missing_mode_expectations() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "example.adoc", "Text.")?;
    for args in [
        vec!["missing.adoc"],
        vec!["../example.adoc"],
        vec!["--mode", "unknown", "example.adoc"],
        vec!["--mode", "draft", "example.adoc"],
        vec!["--unknown"],
        vec!["--pair"],
        vec!["--pair", "--mode", "draft", "example.adoc"],
        vec!["--mode", "draft", "--pair", "example.adoc"],
    ] {
        let result = Generation::from_args(args).and_then(|request| request.generate(dir.path()));
        assert!(result.is_err());
        assert!(!dir.path().join("example.json").exists());
        assert!(!dir.path().join("example.draft.json").exists());
    }
    Ok(())
}

#[test]
fn a_mode_specific_regeneration_preserves_the_other_expectation() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "example.adoc", "Text.")?;
    write(dir.path(), "example.asciidoctor.json", "original")?;
    write(dir.path(), "example.draft.json", "layout-test placeholder")?;
    let invalid =
        Generation::from_args(["--mode", "asciidoctor", "--diagnostics", "example.adoc"])?;
    assert!(invalid.generate(dir.path()).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("example.asciidoctor.json"))?,
        "original"
    );
    let request = Generation::from_args(["--mode", "asciidoctor", "example.adoc"])?;
    assert_eq!(request.generate(dir.path())?, 1);
    assert_eq!(
        fs::read_to_string(dir.path().join("example.draft.json"))?,
        "layout-test placeholder"
    );
    assert!(!dir.path().join("example.json").exists());
    let case = Case::load(dir.path(), &dir.path().join("example.adoc"), false)?;
    case.verify(case.expectations.first().ok_or("missing expectation")?)?;
    Ok(())
}

#[test]
fn shared_generation_rejects_mode_differences_without_writing() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "example.adoc", "[title=\"*Title*\"]\nText.")?;
    write(dir.path(), "example.json", "original")?;
    assert!(
        Generation::from_args(["example.adoc"])?
            .generate(dir.path())
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("example.json"))?,
        "original"
    );
    Ok(())
}

#[test]
fn paired_generation_runs_both_modes_and_replaces_shared_expectations() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "example.adoc", "[title=\"*Title*\"]\nText.")?;
    write(dir.path(), "example.json", "original")?;
    write(
        dir.path(),
        "example.diagnostics.json",
        "original diagnostics",
    )?;
    assert_eq!(
        Generation::from_args(["--pair", "example.adoc"])?.generate(dir.path())?,
        1
    );
    assert!(!dir.path().join("example.json").exists());
    assert!(!dir.path().join("example.diagnostics.json").exists());
    let case = Case::load(dir.path(), &dir.path().join("example.adoc"), false)?;
    for expected in &case.expectations {
        assert!(expected.diagnostics.is_some());
        case.verify(expected)?;
    }
    assert_ne!(
        fs::read_to_string(dir.path().join("example.asciidoctor.json"))?,
        fs::read_to_string(dir.path().join("example.draft.json"))?
    );
    Ok(())
}

#[test]
fn paired_generation_creates_both_outcomes_and_diagnostics_for_new_cases() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "example.adoc", "[title=\"*Title*\"]\nText.")?;
    assert_eq!(
        Generation::from_args(["--pair", "--diagnostics", "example.adoc"])?.generate(dir.path())?,
        1
    );
    let case = Case::load(dir.path(), &dir.path().join("example.adoc"), false)?;
    for expected in &case.expectations {
        assert!(expected.diagnostics.is_some());
        case.verify(expected)?;
    }
    let before = fs::read_to_string(dir.path().join("example.asciidoctor.json"))?;
    Generation::from_args(["--mode", "draft", "example.adoc"])?.generate(dir.path())?;
    assert_eq!(
        fs::read_to_string(dir.path().join("example.asciidoctor.json"))?,
        before
    );
    Ok(())
}

#[test]
fn expected_parse_errors_are_generated_and_checked_without_a_json_fallback() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("invalid.adoc"), [0xff])?;
    write(dir.path(), "invalid.error", "")?;
    let request = Generation::from_args(["--diagnostics", "invalid.adoc"])?;
    assert_eq!(request.generate(dir.path())?, 1);
    assert_ne!(fs::read_to_string(dir.path().join("invalid.error"))?, "");
    assert!(!dir.path().join("invalid.json").exists());
    let case = Case::load(dir.path(), &dir.path().join("invalid.adoc"), false)?;
    case.verify(case.expectations.first().ok_or("missing expectation")?)?;
    Ok(())
}

#[test]
fn substitution_fixtures_are_available_in_every_build() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "subs_example.adoc", "Text.")?;
    write(dir.path(), "subs_example.json", "original")?;
    let request = Generation::from_args(["subs_example.adoc"])?;
    assert_eq!(request.generate(dir.path())?, 1);
    let case = Case::load(dir.path(), &dir.path().join("subs_example.adoc"), false)?;
    case.verify(case.expectations.first().ok_or("missing expectation")?)?;
    Ok(())
}

#[cfg(not(feature = "setext"))]
#[test]
fn disabled_setext_cannot_regenerate_a_selected_fixture() -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "title_with_setext.adoc", "Heading\n=======\n")?;
    assert!(
        Generation::from_args(["title_with_setext.adoc"])?
            .generate(dir.path())
            .is_err()
    );
    assert!(!dir.path().join("title_with_setext.json").exists());
    Ok(())
}
