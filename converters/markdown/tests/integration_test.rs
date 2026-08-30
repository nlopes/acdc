use std::path::{Path, PathBuf};

use acdc_converters_core::{Converter, GeneratorMetadata, Options as ConverterOptions};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_markdown::{MarkdownVariant, Processor};
use acdc_parser::{DocumentAttributes, Options as ParserOptions};

type Error = Box<dyn std::error::Error>;

fn assert_canonical_final_newline(output: &str, fixture: &str) {
    assert!(
        output.ends_with('\n'),
        "Markdown output must end with a newline for fixture: {fixture}",
    );
    assert!(
        !output.ends_with("\n\n"),
        "Markdown output must not end with a blank line for fixture: {fixture}",
    );
}

/// Parses the input `.adoc` file, converts to Markdown (GFM), and compares with expected output.
/// Excludes commonmark_* files which have their own test function.
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_gfm_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;

    // Skip commonmark_* files - they have their own test
    if file_name.starts_with("commonmark_") {
        return Ok(());
    }
    let expected_path = Path::new("tests")
        .join("fixtures")
        .join("expected")
        .join(file_name)
        .with_extension("md");

    // Parse the AsciiDoc input with rendering defaults
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse_file(&path, &parser_options)?;
    let doc = parsed.document();

    // Convert to Markdown (GFM variant)
    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = Processor::new(converter_options, doc.attributes.clone())
        .with_variant(MarkdownVariant::GitHubFlavored);
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("markdown");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, Some(&path), None, &mut diagnostics)?;

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    assert_canonical_final_newline(&actual, file_name);
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "Markdown output mismatch for fixture: {file_name}",
    );
    Ok(())
}

/// Test `CommonMark` variant separately for features that differ
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_commonmark_variant(
    #[files("tests/fixtures/source/commonmark_*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;
    let expected_path = Path::new("tests")
        .join("fixtures")
        .join("expected")
        .join(file_name)
        .with_extension("md");

    // Parse the AsciiDoc input
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse_file(&path, &parser_options)?;
    let doc = parsed.document();

    // Convert to Markdown (CommonMark variant)
    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = Processor::new(converter_options, doc.attributes.clone())
        .with_variant(MarkdownVariant::CommonMark);
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("markdown");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, Some(&path), None, &mut diagnostics)?;

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    assert_canonical_final_newline(&actual, file_name);
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "CommonMark output mismatch for fixture: {file_name}",
    );
    Ok(())
}

fn convert_str_with_variant(
    input: &str,
    variant: MarkdownVariant,
) -> Result<(String, Vec<acdc_converters_core::Warning>), Error> {
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse(input, &parser_options)?;
    let doc = parsed.document();

    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = Processor::new(converter_options, doc.attributes.clone()).with_variant(variant);
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("markdown");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;

    Ok((String::from_utf8(output)?, warnings))
}

fn convert_str(input: &str) -> Result<(String, Vec<acdc_converters_core::Warning>), Error> {
    convert_str_with_variant(input, MarkdownVariant::GitHubFlavored)
}

fn has_numbering_style_warning(warnings: &[acdc_converters_core::Warning]) -> bool {
    warnings.iter().any(|warning| {
        warning
            .message
            .contains("non-numeric ordered list numbering styles not natively supported")
    })
}

#[test]
fn markdown_block_spacing_and_final_newline_are_canonical() -> Result<(), Error> {
    for (input, expected) in [
        ("", "\n"),
        ("Paragraph.\n", "Paragraph.\n"),
        ("First.\n\nSecond.\n", "First.\n\nSecond.\n"),
        ("= Title\n", "# Title\n"),
        ("= Title\n\nBody.\n", "# Title\n\nBody.\n"),
        (
            "[discrete]\n== Heading\n",
            "<a id=\"_heading\"></a>\n## Heading\n",
        ),
    ] {
        let (output, _warnings) = convert_str(input)?;
        pretty_assertions::assert_eq!(output, expected, "input: {input:?}");
    }
    Ok(())
}

#[test]
fn ordered_list_with_non_numeric_style_warns_and_renders_numerically() -> Result<(), Error> {
    let (output, warnings) = convert_str("[upperalpha]\n. First\n. Second\n")?;

    assert!(
        has_numbering_style_warning(&warnings),
        "expected a numbering-style warning, got: {warnings:?}"
    );
    assert!(output.contains("1. First"), "output was: {output}");
    assert!(output.contains("2. Second"), "output was: {output}");
    Ok(())
}

#[test]
fn ordered_list_without_style_does_not_warn() -> Result<(), Error> {
    let (output, warnings) = convert_str(". First\n. Second\n")?;

    assert!(
        !has_numbering_style_warning(&warnings),
        "did not expect a numbering-style warning, got: {warnings:?}"
    );
    assert!(output.contains("1. First"), "output was: {output}");
    Ok(())
}

#[test]
fn ordered_list_with_arabic_style_does_not_warn() -> Result<(), Error> {
    let (_output, warnings) = convert_str("[arabic]\n. First\n. Second\n")?;

    assert!(
        !has_numbering_style_warning(&warnings),
        "did not expect a numbering-style warning, got: {warnings:?}"
    );
    Ok(())
}

#[test]
fn cross_references_render_as_links_to_the_target_anchor() -> Result<(), Error> {
    let (output, _warnings) = convert_str(
        "[[titled]]\n.A *title*\n====\nbody\n====\n\n\
         Some [[labelled,A label]]text.\n\n\
         [[untitled]]\n====\nbody\n====\n\n\
         See <<titled>>, <<labelled>>, <<untitled>>, <<missing>>, and <<titled,own text>>.\n",
    )?;

    assert!(
        output.contains(
            "See [A **title**](#titled), [A label](#labelled), [[untitled]](#untitled), \
             [[missing]](#missing), and [own text](#titled)."
        ),
        "output was: {output}"
    );
    Ok(())
}

#[test]
fn block_cross_references_have_stable_destinations_in_both_variants() -> Result<(), Error> {
    let input = ":toc: macro\n\n\
                 == Generated Section\n\n\
                 [#explicit-section]\n=== Explicit Section\n\n\
                 [#discrete-id,discrete]\n==== Discrete Heading\n\n\
                 [[paragraph-id]]\nParagraph.\n\n\
                 [#list-id]\n* Item\n\n\
                 [[ordered-list-id]]\n. Item\n\n\
                 [#description-list-id]\nTerm:: Definition\n\n\
                 [[admonition-id]]\nNOTE: Note.\n\n\
                 [[listing-id]]\n----\ncode\n----\n\n\
                 [#image-id]\nimage::image.png[]\n\n\
                 [[audio-id]]\naudio::audio.mp3[]\n\n\
                 [#video-id]\nvideo::video.mp4[]\n\n\
                 [[table-id]]\n|===\n|Cell\n|===\n\n\
                 [#toc-id]\ntoc::[]\n\n\
                 [source]\n----\ncallout <1>\n----\n\
                 [[callout-list-id]]\n<1> Explanation.\n\n\
                 [#page-id]\n<<<\n\n\
                 [[thematic-id]]\n'''\n\n\
                 See <<Generated Section>>, <<explicit-section>>, <<discrete-id>>, \
                 <<paragraph-id>>, <<list-id>>, <<ordered-list-id>>, <<description-list-id>>, \
                 <<admonition-id>>, <<listing-id>>, <<image-id>>, <<audio-id>>, <<video-id>>, \
                 <<table-id>>, <<toc-id>>, <<callout-list-id>>, <<page-id>>, and \
                 <<thematic-id>>.\n";

    for variant in [MarkdownVariant::GitHubFlavored, MarkdownVariant::CommonMark] {
        let (output, _warnings) = convert_str_with_variant(input, variant)?;

        for id in [
            "_generated_section",
            "explicit-section",
            "discrete-id",
            "paragraph-id",
            "list-id",
            "ordered-list-id",
            "description-list-id",
            "admonition-id",
            "listing-id",
            "image-id",
            "audio-id",
            "video-id",
            "table-id",
            "toc-id",
            "callout-list-id",
            "page-id",
            "thematic-id",
        ] {
            let anchor = format!(r#"<a id="{id}"></a>"#);
            assert_eq!(
                output.matches(&anchor).count(),
                1,
                "expected one {anchor:?} in {output}"
            );
        }
        for destination in [
            "#_generated_section",
            "#explicit-section",
            "#discrete-id",
            "#paragraph-id",
            "#list-id",
            "#ordered-list-id",
            "#description-list-id",
            "#admonition-id",
            "#listing-id",
            "#image-id",
            "#audio-id",
            "#video-id",
            "#table-id",
            "#toc-id",
            "#callout-list-id",
            "#page-id",
            "#thematic-id",
        ] {
            assert!(
                output.contains(&format!("]({destination})")),
                "missing link to {destination:?} in {output}"
            );
        }
    }
    Ok(())
}

#[test]
fn interdocument_xref_macros_link_to_other_markdown_documents() -> Result<(), Error> {
    let (output, _warnings) = convert_str(
        "Empty: xref:Other.adoc[].\n\nExplicit: xref:Other.adoc[Other].\n\nShorthand: <<Other.adoc>>.\n\nFragment: xref:Foo#Bar[].\n\n== Other.adoc\n\n== Foo#Bar\n",
    )?;

    for expected in [
        "Empty: [Other.md](Other.md).",
        "Explicit: [Other](Other.md).",
        "Shorthand: [Other.adoc](#_other_adoc).",
        "Fragment: [Foo.md](Foo.md#Bar).",
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
    let (output, _warnings) = convert_str(
        "Title macro: <<Pass raw Title>>.\nTitle plus: <<Plus raw Title>>.\nTarget macro: <<Target pass:[raw] Title>>.\nTarget plus: <<Target +raw+ Title>>.\nMissing macro: <<Missing pass:[raw] Title>>.\nMissing plus: <<Missing +raw+ Title>>.\nControl: <<Control Title>>.\n\n== Pass pass:[raw] Title\n\n== Plus +raw+ Title\n\n== Target raw Title\n\n== Control Title\n",
    )?;

    for expected in [
        "Title macro: [Pass raw Title](#_pass_raw_title).",
        "Title plus: [Plus raw Title](#_plus_raw_title).",
        "Target macro: [[Target raw Title]](#Target raw Title).",
        "Target plus: [[Target raw Title]](#Target raw Title).",
        "Missing macro: [[Missing raw Title]](#Missing raw Title).",
        "Missing plus: [[Missing raw Title]](#Missing raw Title).",
        "Control: [Control Title](#_control_title).",
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
fn a_cross_reference_inside_reference_text_is_not_a_nested_link() -> Result<(), Error> {
    // Markdown links do not nest, and the resolution must terminate.
    let (output, _warnings) =
        convert_str("[[a]]\n.See <<a>> again\n====\nbody\n====\n\nRef: <<a>>.\n")?;

    assert!(
        output.contains("Ref: [See [a] again](#a)."),
        "output was: {output}"
    );
    Ok(())
}

#[test]
fn captioned_cross_references_honor_source_order_xrefstyle() -> Result<(), Error> {
    let (output, _warnings) = convert_str(
        ":figure-caption: BeforeFigure\n:table-caption: BeforeTable\n:xrefstyle: short\n\nForward short: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: full\n\nForward full: <<figure-target>> and <<table-target>>.\n\n:figure-caption: TargetFigure\n:table-caption: TargetTable\n\n[[figure-target]]\n.A figure title\nimage::figure.svg[]\n\n[[table-target]]\n.A table title\n|===\n|Cell\n|===\n\n:figure-caption: AfterFigure\n:table-caption: AfterTable\n:xrefstyle: short\n\nBackward short: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: full\n\nBackward full: <<figure-target>> and <<table-target>>.\n",
    )?;

    for expected in [
        "Forward short: [TargetFigure 1](#figure-target) and [BeforeTable 1](#table-target)",
        "Forward full: [TargetFigure 1, “A figure title”](#figure-target) and [BeforeTable 1, “A table title”](#table-target)",
        "Backward short: [TargetFigure 1](#figure-target) and [AfterTable 1](#table-target)",
        "Backward full: [TargetFigure 1, “A figure title”](#figure-target) and [AfterTable 1, “A table title”](#table-target)",
    ] {
        assert!(
            output.contains(expected),
            "expected {expected:?} in {output}"
        );
    }
    Ok(())
}

#[test]
fn source_presentation_fallback_warnings_are_deduplicated() -> Result<(), Error> {
    let input = "[source,rust,linenums,highlight=2]\n----\none\ntwo\n----\n\n[source,rust,linenums,highlight=2]\n----\nrepeat one\nrepeat two\n----\n\n[source,php,options=mixed]\n----\n<?php echo 'one'; ?>\n----\n\n[source,php,options=mixed]\n----\n<?php echo 'two'; ?>\n----\n";
    let (output, warnings) = convert_str(input)?;

    for message in [
        "source line numbering is not supported",
        "selected source-line highlighting is not supported",
        "PHP source block mixed-mode highlighting is not supported",
    ] {
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.message.contains(message))
                .count(),
            1,
            "{warnings:?}"
        );
    }
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    assert!(
        warnings.iter().all(|warning| {
            warning.source.converter == "markdown" && warning.advice().is_some()
        })
    );
    for source_line in [
        "one",
        "two",
        "repeat one",
        "repeat two",
        "<?php echo 'one'; ?>",
    ] {
        assert!(
            output.contains(source_line),
            "missing {source_line:?} in {output}"
        );
    }
    Ok(())
}
