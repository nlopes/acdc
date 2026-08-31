use std::path::PathBuf;

use acdc_converters_core::{
    Converter, Diagnostics, Options as ConverterOptions, WarningSource, visitor::Visitor,
};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_terminal::{Capabilities, Processor, TerminalVisitor};
use acdc_parser::{DocumentAttributes, Options as ParserOptions};

type Error = Box<dyn std::error::Error>;

#[cfg(feature = "images")]
fn temp_output_path(name: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!("acdc-{name}-{}.{extension}", std::process::id()))
}

fn render_terminal(
    input: &str,
    width: usize,
    capabilities: Capabilities,
) -> Result<(String, Vec<String>), Error> {
    crossterm::style::force_color_output(true);
    let parsed = acdc_parser::parse(input, &ParserOptions::default())?;
    let doc = parsed.document();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone())
        .with_terminal_width(width)
        .with_dark_mode(true)
        .with_terminal_capabilities(capabilities);
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
    Ok((
        String::from_utf8(output)?,
        warnings
            .into_iter()
            .map(|warning| warning.message.into_owned())
            .collect(),
    ))
}

const TEXT_TERMINAL: Capabilities = Capabilities {
    unicode: true,
    osc8_links: false,
};

const OSC8_TERMINAL: Capabilities = Capabilities {
    unicode: true,
    osc8_links: true,
};

#[test]
fn unhandled_parser_block_warning_is_structured() -> Result<(), Error> {
    let parsed = acdc_parser::parse("Paragraph.\n", &ParserOptions::default())?;
    let doc = parsed.document();
    let block = doc.blocks.first().ok_or("missing test block")?;
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("terminal");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    {
        let mut visitor = TerminalVisitor::new(&mut output, processor, diagnostics.reborrow());
        visitor.visit_unhandled_block(block)?;
    }

    assert!(output.is_empty());
    let warning = warnings.first().ok_or("missing fallback warning")?;
    assert_eq!(warning.source.converter, "terminal");
    assert!(warning.message.contains("omitted from terminal output"));
    assert!(warning.advice().is_some());
    Ok(())
}

fn strip_terminal_sequences(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for control in chars.by_ref() {
                    if ('@'..='~').contains(&control) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(control) = chars.next() {
                    if control == '\u{7}' {
                        break;
                    }
                    if control == '\u{1b}' && chars.next_if_eq(&'\\').is_some() {
                        break;
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    output
}

// Generate one test per fixture. `requires:` omits fixtures whose expected
// output depends on an unavailable terminal feature.
macro_rules! terminal_fixture_catalog {
    ( [ $( ($name:ident, $has_osc8_variant:expr $(, requires: $cfg:meta )? ) ),* $(,)? ] ) => {
        const TERMINAL_FIXTURES: &[(&str, bool)] = &[
            $((stringify!($name), $has_osc8_variant)),*
        ];

        $(
            $( #[cfg($cfg)] )?
            #[cfg(test)]
            mod $name {
                use super::*;
                #[test]
                fn test() -> Result<(), Error> {
                    let fixture_name = stringify!($name);
                    test_fixture_variants(fixture_name, $has_osc8_variant)
                }
            }
        )*
    };
}

include!("fixtures/catalog.rs");

fn test_fixture_variants(fixture_name: &str, has_osc8_variant: bool) -> Result<(), Error> {
    // Fixtures whose name contains `subs` test `[subs="…"]` behaviour, which
    // only takes effect under the `pre-spec-subs` feature. When the feature
    // is off, skip — the expected output captures the feature-on behaviour
    // and cannot match.
    #[cfg(not(feature = "pre-spec-subs"))]
    if fixture_name.contains("subs") {
        return Ok(());
    }

    test_fixture_variant(fixture_name, false)?;
    if has_osc8_variant {
        test_fixture_variant(fixture_name, true)?;
    }
    Ok(())
}

fn test_fixture_variant(fixture_name: &str, osc8: bool) -> Result<(), Error> {
    crossterm::style::force_color_output(true);

    let input_path = PathBuf::from("tests/fixtures/source").join(format!("{fixture_name}.adoc"));

    // Parse the `AsciiDoc` input with rendering defaults
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse_file(&input_path, &parser_options)?;
    let doc = parsed.document();

    // Convert to Terminal output
    let mut output = Vec::new();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone())
        .with_terminal_width(80)
        .with_dark_mode(true)
        .with_terminal_capabilities(Capabilities {
            unicode: true,
            osc8_links: osc8,
        });
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        doc,
        &mut output,
        Some(input_path.as_path()),
        None,
        &mut diagnostics,
    )?;

    let fixture_name = if osc8 {
        format!("{fixture_name}.osc8.txt")
    } else {
        format!("{fixture_name}.txt")
    };
    let expected_path = PathBuf::from("tests/fixtures/expected").join(&fixture_name);

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "Terminal output mismatch for fixture: {fixture_name}",
    );

    Ok(())
}

#[test]
fn osc8_expected_files_match_fixture_catalog() -> Result<(), Error> {
    use std::collections::BTreeSet;

    let expected_names = PathBuf::from("tests/fixtures/expected")
        .read_dir()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".osc8.txt").map(str::to_string))
        .collect::<BTreeSet<_>>();
    let fixture_names = TERMINAL_FIXTURES
        .iter()
        .filter_map(|(name, has_osc8_variant)| has_osc8_variant.then_some((*name).to_string()))
        .collect::<BTreeSet<_>>();

    assert_eq!(fixture_names, expected_names);
    Ok(())
}

#[test]
fn explicit_ordered_list_numbering_styles() -> Result<(), Error> {
    // An explicit `[<style>]` on an ordered list drives the literal marker text.
    let cases = [
        ("upperalpha", ["A. one", "B. two", "C. three"]),
        ("loweralpha", ["a. one", "b. two", "c. three"]),
        ("lowerroman", ["i. one", "ii. two", "iii. three"]),
        ("upperroman", ["I. one", "II. two", "III. three"]),
        ("lowergreek", ["α. one", "β. two", "γ. three"]),
    ];
    for (style, expected_markers) in cases {
        let input = format!("[{style}]\n. one\n. two\n. three\n");
        let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
        let parsed = acdc_parser::parse(&input, &parser_options)?;
        let doc = parsed.document();
        let mut output = Vec::new();
        let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone())
            .with_terminal_width(80);
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
        let actual = String::from_utf8(output)?;
        for marker in expected_markers {
            assert!(
                actual.contains(marker),
                "style `{style}` should render marker `{marker}`:\n{actual}"
            );
        }
    }
    Ok(())
}

#[test]
fn captioned_cross_references_honor_source_order_xrefstyle() -> Result<(), Error> {
    let input = ":figure-caption: BeforeFigure\n:table-caption: BeforeTable\n:xrefstyle: short\n\nForward short: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: full\n\nForward full: <<figure-target>> and <<table-target>>.\n\n:figure-caption: TargetFigure\n:table-caption: TargetTable\n\n[[figure-target]]\n.A figure title\nimage::figure.svg[]\n\n[[table-target]]\n.A table title\n|===\n|Cell\n|===\n\n:figure-caption: AfterFigure\n:table-caption: AfterTable\n:xrefstyle: short\n\nBackward short: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: full\n\nBackward full: <<figure-target>> and <<table-target>>.\n";
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse(input, &parser_options)?;
    let doc = parsed.document();
    let mut output = Vec::new();
    let processor =
        Processor::new(ConverterOptions::default(), doc.attributes.clone()).with_terminal_width(80);
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
    let output = String::from_utf8(output)?;

    for expected in [
        "TargetFigure 1",
        "BeforeTable 1",
        "TargetFigure 1, “A figure title”",
        "BeforeTable 1, “A table title”",
        "AfterTable 1",
        "AfterTable 1, “A table title”",
    ] {
        assert!(
            output.contains(expected),
            "expected {expected:?} in {output:?}"
        );
    }
    Ok(())
}

#[test]
fn interdocument_xref_macros_do_not_use_matching_local_titles() -> Result<(), Error> {
    let input = "Empty: xref:Other.adoc[].\n\nExplicit: xref:Other.adoc[Other].\n\nShorthand: <<Other.adoc>>.\n\nFragment: xref:Foo#Bar[].\n\n== Other.adoc\n\n== Foo#Bar\n";
    let parsed = acdc_parser::parse(input, &ParserOptions::default())?;
    let doc = parsed.document();
    let mut output = Vec::new();
    let processor =
        Processor::new(ConverterOptions::default(), doc.attributes.clone()).with_terminal_width(80);
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
    let output = String::from_utf8(output)?;

    for expected in [
        "\u{1b}[4m[Other.adoc]\u{1b}[24m",
        "\u{1b}[4mOther\u{1b}[24m",
        "\u{1b}[4mOther.adoc\u{1b}[24m",
        "\u{1b}[4m[Foo#Bar]\u{1b}[24m",
    ] {
        assert!(
            output.contains(expected),
            "expected {expected:?} in {output:?}"
        );
    }
    Ok(())
}

#[test]
fn passthroughs_are_restored_before_natural_xref_resolution() -> Result<(), Error> {
    let input = "Title macro: <<Pass raw Title>>.\nTitle plus: <<Plus raw Title>>.\nTarget macro: <<Target pass:[raw] Title>>.\nTarget plus: <<Target +raw+ Title>>.\nMissing macro: <<Missing pass:[raw] Title>>.\nMissing plus: <<Missing +raw+ Title>>.\nControl: <<Control Title>>.\n\n== Pass pass:[raw] Title\n\n== Plus +raw+ Title\n\n== Target raw Title\n\n== Control Title\n";
    let parsed = acdc_parser::parse(input, &ParserOptions::default())?;
    let doc = parsed.document();
    let mut output = Vec::new();
    let processor =
        Processor::new(ConverterOptions::default(), doc.attributes.clone()).with_terminal_width(80);
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
    let output = String::from_utf8(output)?;

    for resolved in ["Pass raw Title", "Plus raw Title"] {
        assert!(!output.contains(&format!("[{resolved}]")), "{output:?}");
    }
    assert_eq!(
        output.matches("[Target raw Title]").count(),
        2,
        "{output:?}"
    );
    assert_eq!(
        output.matches("[Missing raw Title]").count(),
        2,
        "{output:?}"
    );
    assert!(output.contains("Control Title"), "{output:?}");
    assert!(!output.contains('\u{fffd}'), "{output:?}");
    Ok(())
}

#[test]
fn none_ordered_list_style_suppresses_marker() -> Result<(), Error> {
    let input = ". numbered\n\n[none]\n. unmarked\n";
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse(input, &parser_options)?;
    let doc = parsed.document();
    let mut output = Vec::new();
    let processor =
        Processor::new(ConverterOptions::default(), doc.attributes.clone()).with_terminal_width(80);
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
    let actual = String::from_utf8(output)?;

    assert!(actual.contains("1. numbered"), "{actual}");
    assert!(actual.lines().any(|line| line == "unmarked"), "{actual}");
    Ok(())
}

#[test]
fn markerless_ordered_list_styles_suppress_markers() -> Result<(), Error> {
    for style in ["no-bullet", "unstyled", "unnumbered"] {
        let input = format!("[{style}]\n. unmarked\n");
        let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
        let parsed = acdc_parser::parse(&input, &parser_options)?;
        let doc = parsed.document();
        let mut output = Vec::new();
        let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone())
            .with_terminal_width(80);
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
        let actual = String::from_utf8(output)?;

        assert!(actual.lines().any(|line| line == "unmarked"), "{actual}");
    }
    Ok(())
}

#[test]
fn markerless_unordered_list_styles_suppress_markers() -> Result<(), Error> {
    for style in ["none", "no-bullet", "unstyled"] {
        let input = format!("[{style}]\n* unmarked\n");
        let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
        let parsed = acdc_parser::parse(&input, &parser_options)?;
        let doc = parsed.document();
        let mut output = Vec::new();
        let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone())
            .with_terminal_width(80);
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
        let actual = String::from_utf8(output)?;

        assert!(actual.lines().any(|line| line == "unmarked"), "{actual}");
    }
    Ok(())
}

#[test]
fn markerless_checklist_styles_keep_the_checkbox() -> Result<(), Error> {
    for style in ["none", "no-bullet", "unstyled"] {
        let input = format!("[{style}]\n* [ ] task\n");
        let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
        let parsed = acdc_parser::parse(&input, &parser_options)?;
        let doc = parsed.document();
        let mut output = Vec::new();
        let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone())
            .with_terminal_width(80);
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;
        let actual = String::from_utf8(output)?;

        assert!(actual.contains("[ ]"), "{actual}");
        assert!(
            !actual.lines().any(|line| line.starts_with("* ")),
            "{actual}"
        );
    }
    Ok(())
}

#[cfg(feature = "images")]
#[test]
fn image_failure_warning_is_returned_in_conversion_result() -> Result<(), Error> {
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse("image::definitely-missing-image.png[]\n", &parser_options)?;
    let doc = parsed.document();
    let processor =
        Processor::new(ConverterOptions::default(), doc.attributes.clone()).with_terminal_width(80);
    let output_path = temp_output_path("terminal-warning", "txt");

    let result = processor.convert_to_file(doc, None, &output_path)?;
    let _ = std::fs::remove_file(&output_path);

    assert!(result.warnings().iter().any(|warning| {
        warning.source.converter == "terminal"
            && warning.message.contains("definitely-missing-image.png")
    }));
    Ok(())
}

#[test]
fn document_structure_keeps_revision_navigation_and_hidden_section_meaning() -> Result<(), Error> {
    let input = "= Guide\n:doctype: book\n:toc:\n:toclevels: 3\n:sectnums:\n:partnums:\n:version-label: Release\n:revnumber: v2.1\n:chapter-signifier: Unit\n\n= Part One\n\n== First\n\nSee <<hidden-heading>> and footnote:[Navigation note].\n\n[[hidden-heading]]\n[%notitle]\n=== Hidden Heading\n\nHidden body.\n\n[index]\n== Early Index\n\nIndex body.\n\n== Second\n\nSecond body.\n";
    let (output, warnings) = render_terminal(input, 80, TEXT_TERMINAL)?;
    let plain = strip_terminal_sequences(&output);

    assert!(warnings.is_empty(), "{warnings:?}");
    for expected in [
        "Release v2.1",
        "I: Part One",
        "1.1. Hidden Heading",
        "Early Index",
        "Index body.",
        "Hidden body.",
        "[1] Navigation note",
    ] {
        assert!(
            plain.contains(expected),
            "missing {expected:?} in {output:?}"
        );
    }
    assert_eq!(plain.matches("Unit 1. First").count(), 2, "{output:?}");
    assert_eq!(plain.matches("Unit 2. Second").count(), 2, "{output:?}");
    assert_eq!(plain.matches("Hidden Heading").count(), 2, "{output:?}");

    let (osc8, osc8_warnings) = render_terminal(input, 80, OSC8_TERMINAL)?;
    let osc8_plain = strip_terminal_sequences(&osc8);
    assert!(osc8_warnings.is_empty(), "{osc8_warnings:?}");
    assert_eq!(osc8_plain.matches("Hidden Heading").count(), 2, "{osc8:?}");
    assert!(osc8_plain.contains("[1] Navigation note"), "{osc8:?}");
    Ok(())
}

#[test]
fn unset_chapter_signifier_keeps_only_the_chapter_number() -> Result<(), Error> {
    let input = "= Guide\n:doctype: book\n:toc:\n:sectnums:\n:chapter-signifier!:\n\n== Start\n";
    let (output, _) = render_terminal(input, 80, TEXT_TERMINAL)?;
    let plain = strip_terminal_sequences(&output);

    assert_eq!(plain.matches("1. Start").count(), 2, "{output:?}");
    assert!(!plain.contains("Chapter 1."), "{output:?}");
    Ok(())
}

#[test]
fn default_chapter_signifier_labels_heading_and_toc_entry() -> Result<(), Error> {
    let input = "= Guide\n:doctype: book\n:toc:\n:sectnums:\n\n== Start\n";
    let (output, _) = render_terminal(input, 80, TEXT_TERMINAL)?;
    let plain = strip_terminal_sequences(&output);

    assert_eq!(plain.matches("Chapter 1. Start").count(), 2, "{output:?}");
    Ok(())
}

#[test]
fn blocks_roles_and_admonition_modes_have_terminal_distinctions() -> Result<(), Error> {
    let input = ".Sample\n[example]\nExample text.\n\n[abstract]\nSummary text.\n\n.Open title\n--\nOpen content.\n--\n\n[quote,Block Poet,Block Work]\n____\nQuoted text.\n____\n\n[verse,Verse Poet,Verse Work]\n____\nVerse text.\n____\n\n[.lead]\nLead text.\n\n[.small]\nSmall text.\n\nNOTE: Pay attention.\n";
    let (output, warnings) = render_terminal(input, 80, TEXT_TERMINAL)?;
    let plain = strip_terminal_sequences(&output);

    assert!(warnings.is_empty(), "{warnings:?}");
    for expected in [
        "Sample",
        "│ Example text.",
        "ABSTRACT",
        "Summary text.",
        "Open title",
        "Open content.",
        "Block Poet",
        "Block Work",
        "Verse Poet",
        "Verse Work",
        "Lead text.",
        "Small text.",
        "Note:",
    ] {
        assert!(
            plain.contains(expected),
            "missing {expected:?} in {output:?}"
        );
    }
    assert!(!output.contains("ℹ️"), "icons are disabled: {output:?}");

    let ascii = Capabilities {
        unicode: false,
        osc8_links: false,
    };
    let (no_unicode, _) = render_terminal(":icons: font\n\nNOTE: Text.\n", 80, ascii)?;
    assert!(!no_unicode.contains("ℹ️"), "{no_unicode:?}");
    assert!(
        strip_terminal_sequences(&no_unicode).contains("| Note:"),
        "{no_unicode:?}"
    );
    Ok(())
}

#[test]
fn list_styles_bibliography_and_callout_numbers_are_preserved() -> Result<(), Error> {
    let input = "[%reversed,start=5]\n. Five\n. Four\n. Three\n\n[%reversed]\n. Default three\n. Default two\n. Default one\n\n[disc]\n* Disc\n\n[circle]\n* Circle\n\n[square]\n* Square\n\n[ordered]\nFirst:: One.\nSecond:: Two.\n\n[unordered]\nAlpha:: A.\nBeta:: B.\n\n[source,rust]\n----\nlet value = 1; // <3>\n----\n<3> Explicit three\n\n[bibliography]\n== References\n\n* [[[ref,Reference Label]]] Entry.\n";
    let (output, warnings) = render_terminal(input, 80, TEXT_TERMINAL)?;
    let plain = strip_terminal_sequences(&output);

    assert!(warnings.is_empty(), "{warnings:?}");
    for expected in [
        "5. Five",
        "4. Four",
        "3. Three",
        "3. Default three",
        "2. Default two",
        "1. Default one",
        "• Disc",
        "◦ Circle",
        "▪ Square",
        "1. First",
        "2. Second",
        "• Alpha",
        "• Beta",
        "<3> Explicit three",
        "[Reference Label] Entry.",
    ] {
        assert!(
            plain.contains(expected),
            "missing {expected:?} in {output:?}"
        );
    }
    assert!(!output.contains("// <3>"), "{output:?}");

    let ascii = Capabilities {
        unicode: false,
        osc8_links: false,
    };
    let (ascii_output, _) = render_terminal(input, 80, ascii)?;
    let ascii_plain = strip_terminal_sequences(&ascii_output);
    for marker in ["* Disc", "o Circle", "+ Square"] {
        assert!(ascii_plain.contains(marker), "{ascii_output:?}");
    }
    Ok(())
}

#[test]
fn source_options_and_mixed_php_fallback_are_visible_and_deduplicated() -> Result<(), Error> {
    let input = ":project: acdc\n\n[source,rust,linenums,start=10,highlight=11..12]\n----\nlet one = 1; // <1>\nlet two = \"λ\";\nlet long_name = \"abcdefghijklmnopqrstuvwxyz0123456789\";\n----\n<1> First value\n\n[source,unknown-language,subs=\"+attributes\"]\n----\n{project}\n----\n\n[source,rust]\n----\nslash(); // <2>\nhash = 1 # <3>\nSELECT 1; -- <4>\n(def x 1) ;; <5>\nsingle ; <6>\n<tag/> <!--8-->\nmultiple(); // <9> <10>\nauto(); // <.>\n----\n<2> Slash guard\n<3> Hash guard\n<4> Dash guard\n<5> Double-semicolon guard\n<6> Single semicolon remains\n<8> XML guard\n<9> First marker on one line\n<10> Second marker on one line\n<.> Automatic marker\n\n[source,php,options=mixed]\n----\n<?php echo 'one'; ?>\n----\n\n[source,php,options=mixed]\n----\n<?php echo 'two'; ?>\n----\n";
    let (output, warnings) = render_terminal(input, 48, TEXT_TERMINAL)?;

    for expected in [
        "10 │",
        "11 │",
        "12 │",
        "<1>",
        "λ",
        "long_name",
        "slash(); <2>",
        "hash = 1 <3>",
        "SELECT 1; <4>",
        "(def x 1) <5>",
        "single ; <6>",
        "<tag/> <8>",
        "multiple(); <9> <10>",
        "auto(); <1>",
        "<1> Automatic marker",
    ] {
        assert!(
            strip_terminal_sequences(&output).contains(expected),
            "missing {expected:?} in {output:?}"
        );
    }
    #[cfg(feature = "pre-spec-subs")]
    assert!(
        strip_terminal_sequences(&output).contains("acdc"),
        "{output:?}"
    );
    #[cfg(not(feature = "pre-spec-subs"))]
    assert!(
        strip_terminal_sequences(&output).contains("{project}"),
        "{output:?}"
    );
    assert!(!output.contains("// <1>"), "{output:?}");
    let highlighted_line = output
        .lines()
        .find(|line| line.contains("two"))
        .ok_or("highlighted source line missing")?;
    assert!(
        highlighted_line.contains("\u{1b}[48;2;64;64;64m"),
        "highlight style missing from selected line: {highlighted_line:?}"
    );
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.contains("mixed-mode highlighting"))
            .count(),
        1,
        "{warnings:?}"
    );
    Ok(())
}

#[test]
fn tables_honor_width_alignment_and_vertical_alignment_metadata() -> Result<(), Error> {
    let input = "[width=50%,align=right,cols=\".<,.<\"]\n|===\n|Top line +\nsecond line\n.>|Bottom\n|===\n\n[%autowidth]\n|===\n|A |B\n|===\n";
    let (output, warnings) = render_terminal(input, 60, TEXT_TERMINAL)?;

    assert!(warnings.is_empty(), "{warnings:?}");
    let plain = strip_terminal_sequences(&output);
    let border = plain
        .lines()
        .find(|line| line.contains('╭'))
        .ok_or("missing table border")?;
    assert!(
        border.starts_with(' '),
        "table was not right aligned: {output:?}"
    );
    assert!(border.chars().count() <= 60, "{border:?}");
    let bottom_line = plain
        .lines()
        .find(|line| line.contains("Bottom"))
        .ok_or("bottom-aligned cell missing")?;
    assert!(bottom_line.contains("second line"), "{plain:?}");
    let autowidth_border = plain
        .lines()
        .rev()
        .find(|line| line.contains('╭'))
        .ok_or("autowidth table border missing")?;
    assert!(autowidth_border.chars().count() < 20, "{plain:?}");
    Ok(())
}

#[test]
fn image_media_and_hidden_schemes_keep_static_link_information() -> Result<(), Error> {
    let input = ":hide-uri-scheme:\n\nimage::https://media.example/photo-file.png[Useful alt,link=https://images.example/view]\n\nInline image:https://media.example/photo-file.png[Inline alt,link=https://images.example/inline].\n\nInner link precedence: link:https://outer.example[image:https://media.example/inner.png[Inner alt,link=https://inner.example]].\n\n.Audio title\naudio::sound.ogg[start=2,end=4]\n\naudio::https://media.example/remote.ogg[]\n\n.Video title\nvideo::first.mp4,second.webm[poster=cover.jpg]\n\nvideo::https://media.example/remote.mp4[]\n\nvideo::[]\n\nlink:https://example.com[] https://example.org[] <https://example.net/path> link:https://window.example[Window link,window=_blank,role=external]\n";
    let (plain, warnings) = render_terminal(input, 80, TEXT_TERMINAL)?;

    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.contains("video has no source"))
            .count(),
        1,
        "{warnings:?}"
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    for expected in [
        "[Image: Useful alt]",
        "https://images.example/view",
        "[Image: Inline alt]",
        "https://images.example/inline",
        "[Image: Inner alt]",
        "https://inner.example",
        "[Audio: Audio title]",
        "sound.ogg#t=2,4",
        "[Audio: remote.ogg]",
        "https://media.example/remote.ogg",
        "[Video source 1/2: Video title]",
        "first.mp4",
        "[Video source 2/2: Video title]",
        "second.webm",
        "[Poster: cover.jpg]",
        "[Video: remote.mp4]",
        "https://media.example/remote.mp4",
        "example.com",
        "(https://example.com)",
        "example.org",
        "(https://example.org)",
        "example.net/path",
        "(https://example.net/path)",
        "Window link",
        "https://window.example",
    ] {
        assert!(
            plain.contains(expected),
            "missing {expected:?} in {plain:?}"
        );
    }

    let (osc8, _) = render_terminal(input, 80, OSC8_TERMINAL)?;
    assert!(
        osc8.contains("\u{1b}]8;;https://images.example/view"),
        "{osc8:?}"
    );
    assert!(
        osc8.contains("\u{1b}]8;;https://inner.example")
            && !osc8.contains("\u{1b}]8;;https://outer.example"),
        "{osc8:?}"
    );
    assert!(
        osc8.contains("\u{1b}]8;;https://media.example/remote.ogg")
            && osc8.contains("\u{1b}]8;;https://media.example/remote.mp4"),
        "{osc8:?}"
    );
    assert!(osc8.contains("example.com"), "{osc8:?}");
    Ok(())
}
