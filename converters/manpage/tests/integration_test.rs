use std::path::{Path, PathBuf};

use acdc_converters_core::{Converter, GeneratorMetadata, Options as ConverterOptions};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_manpage::Processor;
use acdc_parser::{DocumentAttributes, Options as ParserOptions};

type Error = Box<dyn std::error::Error>;

fn temp_output_path(name: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!("acdc-{name}-{}.{extension}", std::process::id()))
}

fn run_manpage_fixture(path: &Path, expected_dir: &Path, embedded: bool) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;

    // Fixtures whose name contains `subs` test `[subs="…"]` behaviour, which
    // only takes effect under the `pre-spec-subs` feature. When the feature
    // is off, skip — the expected output captures the feature-on behaviour
    // and cannot match.
    #[cfg(not(feature = "pre-spec-subs"))]
    if file_name.contains("subs") {
        return Ok(());
    }

    let expected_path = expected_dir.join(file_name).with_extension("man");

    // Parse the `AsciiDoc` input with rendering defaults
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse_file(path, &parser_options)?;
    let doc = parsed.document();

    // Convert to manpage output
    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .embedded(embedded)
        .build();
    let processor = Processor::new(converter_options, doc.attributes.clone());
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("manpage");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, Some(path), None, &mut diagnostics)?;

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "Manpage output mismatch for fixture: {file_name}",
    );

    Ok(())
}

/// Parses the input `.adoc` file, converts to manpage output, and compares with expected output.
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_with_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    run_manpage_fixture(&path, Path::new("tests/fixtures/expected"), false)
}

/// Parses the input `.adoc` file, converts to embedded manpage output, and compares with expected.
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_embedded_with_fixtures(
    #[files("tests/fixtures/source/embedded/*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    run_manpage_fixture(&path, Path::new("tests/fixtures/expected/embedded"), true)
}

#[test]
fn section_order_warning_is_returned_in_conversion_result() -> Result<(), Error> {
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse(
        "= cmd(1)\n:doctype: manpage\n\n== OVERVIEW\n\ntext\n",
        &parser_options,
    )?;
    let doc = parsed.document();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let output_path = temp_output_path("manpage-warning", "1");

    let result = processor.convert_to_file(doc, None, &output_path)?;
    let _ = std::fs::remove_file(&output_path);

    assert!(result.warnings().iter().any(|warning| {
        warning.source.converter == "manpage"
            && warning
                .message
                .contains("name section should be first, got `OVERVIEW`")
    }));
    Ok(())
}

#[test]
fn custom_name_section_title_is_not_out_of_order() -> Result<(), Error> {
    let source_path = Path::new("tests/fixtures/source/manpage_name_front_matter.adoc");
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse_file(source_path, &parser_options)?;
    let doc = parsed.document();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let output_path = temp_output_path("manpage-custom-name-section", "1");

    let result = processor.convert_to_file(doc, Some(source_path), &output_path)?;
    let _ = std::fs::remove_file(&output_path);

    assert!(result.warnings().is_empty(), "{:?}", result.warnings());
    Ok(())
}

#[test]
fn static_media_playback_warning_is_deduplicated() -> Result<(), Error> {
    let input = include_str!("fixtures/source/video_audio.adoc");
    let parsed = acdc_parser::parse(input, &ParserOptions::default())?;
    let doc = parsed.document();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("manpage");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);

    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;

    assert_eq!(warnings.len(), 1, "unexpected warnings: {warnings:?}");
    let warning = warnings.first().ok_or("missing playback warning")?;
    assert!(
        warning
            .message
            .contains("audio and video playback are not available"),
        "unexpected warning: {warnings:?}"
    );
    assert!(warning.advice().is_some(), "missing advice: {warnings:?}");
    Ok(())
}

#[test]
fn table_fallback_warnings_are_deduplicated() -> Result<(), Error> {
    let input = r"= table-warnings(1)
:doctype: manpage

== NAME

table-warnings - test table fallbacks

== SYNOPSIS

table-warnings

== DESCRIPTION

[stripes=odd,float=right]
|===
| one
|===

[stripes=even,float=right]
|===
| two
|===

[align=right]
|===
| three
|===
";
    let parsed = acdc_parser::parse(input, &ParserOptions::default())?;
    let doc = parsed.document();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("manpage");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);

    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;

    assert_eq!(warnings.len(), 3, "unexpected warnings: {warnings:?}");
    for expected in [
        "table row stripes are not supported",
        "table floats are not supported",
        "right table alignment is not supported",
    ] {
        let warning = warnings
            .iter()
            .find(|warning| warning.message.contains(expected))
            .ok_or_else(|| format!("missing warning containing {expected:?}: {warnings:?}"))?;
        assert!(warning.advice().is_some(), "missing advice: {warning:?}");
    }
    Ok(())
}

#[test]
fn captioned_cross_references_honor_source_order_xrefstyle() -> Result<(), Error> {
    let input = r"= xrefstyle(1)
:doctype: manpage
:figure-caption: BeforeFigure
:table-caption: BeforeTable

== NAME

xrefstyle - test captioned references

== DESCRIPTION

:xrefstyle: basic

Forward basic: <<figure-target>> and <<table-target>>.

:xrefstyle: short

Forward short: <<figure-target>> and <<table-target>>.

:xrefstyle: full

Forward full: <<figure-target>> and <<table-target>>.

:figure-caption: TargetFigure
:table-caption: TargetTable

[[figure-target]]
.A figure title
image::figure.svg[]

[[table-target]]
.A table title
|===
|Cell
|===

:figure-caption: AfterFigure
:table-caption: AfterTable
:xrefstyle: basic

Backward basic: <<figure-target>> and <<table-target>>.

:xrefstyle: short

Backward short: <<figure-target>> and <<table-target>>.

:xrefstyle: full

Backward full: <<figure-target>> and <<table-target>>.
";
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse(input, &parser_options)?;
    let doc = parsed.document();
    let mut output = Vec::new();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("manpage");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
    let output = String::from_utf8(output)?;

    for expected in [
        "Forward basic: A figure title and A table title",
        "Forward short: TargetFigure 1 and BeforeTable 1",
        "Forward full: TargetFigure 1, \\(lqA figure title\\(rq and BeforeTable 1, \\(lqA table title\\(rq",
        "\\fBTargetFigure 1. A figure title\\fP",
        "\\fBTargetTable 1. A table title\\fP",
        "Backward basic: A figure title and A table title",
        "Backward short: TargetFigure 1 and AfterTable 1",
        "Backward full: TargetFigure 1, \\(lqA figure title\\(rq and AfterTable 1, \\(lqA table title\\(rq",
    ] {
        assert!(
            output.contains(expected),
            "expected {expected:?} in {output}"
        );
    }
    Ok(())
}

#[test]
fn interdocument_xref_macros_do_not_use_matching_local_titles() -> Result<(), Error> {
    let input = "= xref-targets(1)\n:doctype: manpage\n\n== NAME\n\nxref-targets - verify xref targets\n\n== SYNOPSIS\n\nEmpty: xref:Other.adoc[].\n\nExplicit: xref:Other.adoc[Other].\n\nShorthand: <<Other.adoc>>.\n\nFragment: xref:Foo#Bar[].\n\n== Other.adoc\n\n== Foo#Bar\n";
    let parsed = acdc_parser::parse(input, &ParserOptions::default())?;
    let doc = parsed.document();
    let mut output = Vec::new();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("manpage");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
    let output = String::from_utf8(output)?;

    for expected in [
        "Empty: [Other.adoc]\\&.",
        "Explicit: Other\\&.",
        "Shorthand: OTHER.ADOC\\&.",
        "Fragment: [Foo#Bar]\\&.",
    ] {
        assert!(
            output.contains(expected),
            "expected {expected:?} in {output}"
        );
    }
    Ok(())
}

#[test]
fn passthroughs_are_restored_before_natural_xref_resolution() -> Result<(), Error> {
    let input = "= passthrough-xrefs(1)\n:doctype: manpage\n\n== NAME\n\npassthrough-xrefs - verify passthrough natural references\n\n== SYNOPSIS\n\nTitle macro: <<Pass raw Title>>.\nTitle plus: <<Plus raw Title>>.\nTarget macro: <<Target pass:[raw] Title>>.\nTarget plus: <<Target +raw+ Title>>.\nMissing macro: <<Missing pass:[raw] Title>>.\nMissing plus: <<Missing +raw+ Title>>.\nControl: <<Control Title>>.\n\n== Pass pass:[raw] Title\n\n== Plus +raw+ Title\n\n== Target raw Title\n\n== Control Title\n";
    let parsed = acdc_parser::parse(input, &ParserOptions::default())?;
    let doc = parsed.document();
    let mut output = Vec::new();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("manpage");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
    let output = String::from_utf8(output)?;

    for expected in [
        "Title macro: PASS RAW TITLE\\&.",
        "Title plus: PLUS RAW TITLE\\&.",
        "Target macro: [Target raw Title]\\&.",
        "Target plus: [Target raw Title]\\&.",
        "Missing macro: [Missing raw Title]\\&.",
        "Missing plus: [Missing raw Title]\\&.",
        "Control: CONTROL TITLE\\&.",
    ] {
        assert!(
            output.contains(expected),
            "expected {expected:?} in {output}"
        );
    }
    assert!(!output.contains('\u{fffd}'), "{output}");
    Ok(())
}

#[test]
fn explicit_ordered_list_numbering_styles() -> Result<(), Error> {
    // An explicit `[<style>]` on an ordered list drives the `.IP` tag text.
    let cases = [
        ("upperalpha", [".IP A. 4", ".IP B. 4", ".IP C. 4"]),
        ("lowerroman", [".IP i. 4", ".IP ii. 4", ".IP iii. 4"]),
        ("lowergreek", [".IP α. 4", ".IP β. 4", ".IP γ. 4"]),
    ];
    for (style, expected_tags) in cases {
        let input = format!("[{style}]\n. one\n. two\n. three\n");
        let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
        let parsed = acdc_parser::parse(&input, &parser_options)?;
        let doc = parsed.document();
        let mut output = Vec::new();
        let converter_options = ConverterOptions::builder().embedded(true).build();
        let processor = Processor::new(converter_options, doc.attributes.clone());
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("manpage");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
        let actual = String::from_utf8(output)?;
        for tag in expected_tags {
            assert!(
                actual.contains(tag),
                "style `{style}` should render `{tag}`:\n{actual}"
            );
        }
    }
    Ok(())
}
