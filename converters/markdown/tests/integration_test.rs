use std::path::{Path, PathBuf};

use acdc_converters_core::{
    Converter, GeneratorMetadata, Options as ConverterOptions, visitor::Visitor,
};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_html::Processor as HtmlProcessor;
use acdc_converters_markdown::{MarkdownVariant, MarkdownVisitor, Processor};
use acdc_parser::{DocumentAttributes, Options as ParserOptions};
use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, Options as MarkdownParserOptions, Parser as MarkdownParser,
    Tag, TagEnd,
};

type Error = Box<dyn std::error::Error>;

const ACCEPTANCE_SEMANTICS: &str = include_str!("fixtures/source/acceptance_semantics.adoc");
const ACCEPTANCE_DESTINATIONS: &[&str] = &[
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
];

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

fn convert_html_str(input: &str) -> Result<(String, Vec<acdc_converters_core::Warning>), Error> {
    let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
    let parsed = acdc_parser::parse(input, &parser_options)?;
    let doc = parsed.document();

    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .embedded(false)
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = HtmlProcessor::new(converter_options, doc.attributes.clone());
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("html");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;

    Ok((String::from_utf8(output)?, warnings))
}

#[test]
fn document_title_uses_last_stacked_anchor_as_destination() -> Result<(), Error> {
    let (output, warnings) = convert_str(
        "[[unused-header-anchor]]\n[[document-header]]\n= Main *Title*: Subtitle _Details_\n\nSee <<document-header>>.\n",
    )?;

    assert!(
        output.starts_with("<a id=\"document-header\"></a>\n"),
        "{output}"
    );
    assert!(!output.contains("id=\"unused-header-anchor\""), "{output}");
    assert!(
        output.contains("[Main **Title**: Subtitle *Details*](#document-header)"),
        "{output}"
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(())
}

#[test]
fn book_chapter_signifier_labels_headings_and_toc_entries() -> Result<(), Error> {
    let (output, warnings) = convert_str(
        "= Book Title\n:doctype: book\n:toc:\n:sectnums:\n:chapter-signifier: Unit\n\n== First Chapter\n\n=== Detail\n",
    )?;

    assert_eq!(
        output.matches("Unit 1. First Chapter").count(),
        2,
        "{output}"
    );
    assert_eq!(output.matches("1.1. Detail").count(), 2, "{output}");
    assert!(!output.contains("Unit 1.1. Detail"), "{output}");
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(())
}

fn markdown_parser_options(variant: MarkdownVariant) -> MarkdownParserOptions {
    let mut options = MarkdownParserOptions::empty();
    if variant == MarkdownVariant::GitHubFlavored {
        options.insert(MarkdownParserOptions::ENABLE_GFM);
        options.insert(MarkdownParserOptions::ENABLE_TABLES);
        options.insert(MarkdownParserOptions::ENABLE_FOOTNOTES);
        options.insert(MarkdownParserOptions::ENABLE_STRIKETHROUGH);
        options.insert(MarkdownParserOptions::ENABLE_TASKLISTS);
    }
    options
}

fn parse_markdown(output: &str, variant: MarkdownVariant) -> Vec<Event<'_>> {
    MarkdownParser::new_ext(output, markdown_parser_options(variant)).collect()
}

fn parsed_text(events: &[Event<'_>]) -> String {
    let mut text = String::new();
    for event in events {
        match event {
            Event::Text(content) | Event::Code(content) => text.push_str(content),
            Event::End(_) | Event::SoftBreak | Event::HardBreak => text.push(' '),
            Event::Start(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::Rule
            | Event::TaskListMarker(_) => {}
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn has_parsed_link(events: &[Event<'_>], destination: &str) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            Event::Start(Tag::Link { dest_url, .. }) if dest_url.as_ref() == destination
        )
    })
}

fn has_parsed_anchor(events: &[Event<'_>], id: &str) -> bool {
    let attribute = format!("id=\"{id}\"");
    events.iter().any(|event| {
        matches!(event, Event::Html(html) | Event::InlineHtml(html) if html.contains(&attribute))
    })
}

fn maximum_list_depth(events: &[Event<'_>]) -> usize {
    let mut depth: usize = 0;
    let mut maximum = 0;
    for event in events {
        match event {
            Event::Start(Tag::List(_)) => {
                depth += 1;
                maximum = maximum.max(depth);
            }
            Event::End(TagEnd::List(_)) => depth = depth.saturating_sub(1),
            Event::Start(_)
            | Event::End(_)
            | Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_) => {}
        }
    }
    maximum
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
    for variant in [MarkdownVariant::GitHubFlavored, MarkdownVariant::CommonMark] {
        let (output, _warnings) = convert_str_with_variant(ACCEPTANCE_SEMANTICS, variant)?;
        let events = parse_markdown(&output, variant);

        for id in ACCEPTANCE_DESTINATIONS {
            let anchor = format!(r#"<a id="{id}"></a>"#);
            assert_eq!(
                output.matches(&anchor).count(),
                1,
                "expected one {anchor:?} in {output}"
            );
            assert!(
                has_parsed_anchor(&events, id),
                "Markdown parser did not retain anchor {id:?}: {events:?}"
            );
            let destination = format!("#{id}");
            assert!(
                has_parsed_link(&events, &destination),
                "Markdown parser did not retain link to {destination:?}: {events:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn gfm_output_parses_with_expected_extended_structure() -> Result<(), Error> {
    let (table_output, _warnings) =
        convert_str(include_str!("fixtures/source/table_fallbacks.adoc"))?;
    let table_events = parse_markdown(&table_output, MarkdownVariant::GitHubFlavored);
    assert!(
        table_events.iter().any(|event| {
            matches!(
                event,
                Event::Start(Tag::Table(alignments))
                    if alignments == &[Alignment::Left, Alignment::Center, Alignment::Right]
            )
        }),
        "missing parsed aligned table: {table_events:?}"
    );
    assert!(
        table_events
            .iter()
            .filter(|event| matches!(event, Event::Start(Tag::TableCell)))
            .count()
            >= 15,
        "missing parsed table cells: {table_events:?}"
    );
    let table_text = parsed_text(&table_events);
    for text in [
        "Table 1. Aligned records",
        "Nested one",
        "Second paragraph.",
    ] {
        assert!(
            table_text.contains(text),
            "missing {text:?}: {table_events:?}"
        );
    }

    let (inline_output, _warnings) =
        convert_str(include_str!("fixtures/source/inline_meaning_links.adoc"))?;
    let inline_events = parse_markdown(&inline_output, MarkdownVariant::GitHubFlavored);
    assert_eq!(
        inline_events
            .iter()
            .filter(|event| {
                matches!(event, Event::FootnoteReference(label) if label.as_ref() == "named")
            })
            .count(),
        2,
        "missing parsed footnote references: {inline_events:?}"
    );
    assert_eq!(
        inline_events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Start(Tag::FootnoteDefinition(label)) if label.as_ref() == "named"
                )
            })
            .count(),
        1,
        "missing parsed footnote definition: {inline_events:?}"
    );
    assert!(
        inline_events
            .iter()
            .any(|event| matches!(event, Event::Code(code) if code.as_ref() == "tick ` inside")),
        "hostile code delimiters did not parse: {inline_events:?}"
    );
    assert!(
        has_parsed_link(&inline_events, "https://example.com/a_(b)"),
        "hostile link destination did not parse: {inline_events:?}"
    );

    let (list_output, _warnings) =
        convert_str(include_str!("fixtures/source/checklist_contexts.adoc"))?;
    let list_events = parse_markdown(&list_output, MarkdownVariant::GitHubFlavored);
    assert!(
        maximum_list_depth(&list_events) >= 3,
        "nested GFM lists did not parse: {list_events:?}"
    );
    assert!(
        list_events
            .iter()
            .any(|event| matches!(event, Event::TaskListMarker(true))),
        "GFM task-list state did not parse: {list_events:?}"
    );
    Ok(())
}

#[test]
fn commonmark_output_parses_with_expected_portable_structure() -> Result<(), Error> {
    let (inline_output, _warnings) = convert_str_with_variant(
        include_str!("fixtures/source/commonmark_inline_meaning_links.adoc"),
        MarkdownVariant::CommonMark,
    )?;
    let inline_events = parse_markdown(&inline_output, MarkdownVariant::CommonMark);
    assert_eq!(
        inline_events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Start(Tag::Link { dest_url, .. })
                        if dest_url.as_ref() == "#_footnote_named"
                )
            })
            .count(),
        2,
        "CommonMark footnote links did not parse: {inline_events:?}"
    );
    assert!(
        inline_events
            .iter()
            .any(|event| matches!(event, Event::Code(code) if code.as_ref() == "tick ` inside")),
        "hostile code delimiters did not parse: {inline_events:?}"
    );
    assert!(
        has_parsed_link(&inline_events, "https://example.com/a_(b)"),
        "hostile link destination did not parse: {inline_events:?}"
    );
    assert!(
        parsed_text(&inline_events).contains("Body with bold and code"),
        "CommonMark footnote body was not retained: {inline_events:?}"
    );

    let (list_output, _warnings) = convert_str_with_variant(
        include_str!("fixtures/source/commonmark_checklist_contexts.adoc"),
        MarkdownVariant::CommonMark,
    )?;
    let list_events = parse_markdown(&list_output, MarkdownVariant::CommonMark);
    assert!(
        maximum_list_depth(&list_events) >= 3,
        "nested CommonMark lists did not parse: {list_events:?}"
    );
    assert!(
        !list_events
            .iter()
            .any(|event| matches!(event, Event::TaskListMarker(_))),
        "CommonMark unexpectedly parsed GFM task-list syntax: {list_events:?}"
    );
    assert!(
        parsed_text(&list_events).contains("[x] Checked sibling"),
        "CommonMark checklist state was not visible: {list_events:?}"
    );

    let (caption_output, _warnings) = convert_str_with_variant(
        include_str!("fixtures/source/header_titles_captions.adoc"),
        MarkdownVariant::CommonMark,
    )?;
    let caption_events = parse_markdown(&caption_output, MarkdownVariant::CommonMark);
    let caption_text = parsed_text(&caption_events);
    for text in [
        "Figure 1. Image Title",
        "Table 1. Table Title",
        "Example 1. Example Title",
        "Listing 1. Listing Title",
    ] {
        assert!(
            caption_text.contains(text),
            "missing {text:?}: {caption_events:?}"
        );
    }

    let (table_output, warnings) = convert_str_with_variant(
        include_str!("fixtures/source/commonmark_no_tables.adoc"),
        MarkdownVariant::CommonMark,
    )?;
    let table_events = parse_markdown(&table_output, MarkdownVariant::CommonMark);
    assert!(
        !table_events
            .iter()
            .any(|event| matches!(event, Event::Start(Tag::Table(_)))),
        "CommonMark unexpectedly parsed a table: {table_events:?}"
    );
    assert!(
        parsed_text(&table_events).contains("Table 1. Skipped Table Title"),
        "CommonMark lost the table caption: {table_events:?}"
    );
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.message.contains("tables not natively supported"))
            .count(),
        1,
        "unexpected CommonMark table diagnostics: {warnings:?}"
    );
    Ok(())
}

#[test]
fn markdown_and_html_preserve_shared_source_meaning() -> Result<(), Error> {
    let (markdown, markdown_warnings) = convert_str(ACCEPTANCE_SEMANTICS)?;
    let (html, html_warnings) = convert_html_str(ACCEPTANCE_SEMANTICS)?;

    for visible in [
        "Acceptance Semantics",
        "Generated Section",
        "Explicit Section",
        "Overview image",
        "Scenic view",
        "Records",
        "Name",
        "Value",
        "Ada",
        "Parent",
        "Child",
        "Footnote body.",
        "Concept",
        "Details",
    ] {
        assert!(
            markdown.contains(visible),
            "Markdown lost {visible:?}: {markdown}"
        );
        assert!(html.contains(visible), "HTML lost {visible:?}: {html}");
    }
    for id in ["_generated_section", "image-id", "table-id", "_index"] {
        assert!(markdown.contains(&format!("id=\"{id}\"")), "{markdown}");
        assert!(html.contains(&format!("id=\"{id}\"")), "{html}");
    }
    for destination in ["#image-id", "#table-id"] {
        assert!(
            markdown.contains(&format!("]({destination})")),
            "{markdown}"
        );
        assert!(html.contains(&format!("href=\"{destination}\"")), "{html}");
    }

    assert!(markdown.contains("![Scenic view](photo.png)"), "{markdown}");
    assert!(
        html.contains("<img src=\"photo.png\" alt=\"Scenic view\""),
        "{html}"
    );
    assert!(markdown.contains("| Name | Value |"), "{markdown}");
    assert!(html.contains("<table"), "{html}");
    assert!(markdown.contains("[^1]"), "{markdown}");
    assert!(html.contains("class=\"footnote\""), "{html}");
    assert!(markdown.contains("_indexterm_0"), "{markdown}");
    assert!(html.contains("_indexterm_0"), "{html}");
    for message in [
        "audio and video playback are not supported",
        "page breaks not natively supported",
    ] {
        assert!(
            markdown_warnings
                .iter()
                .any(|warning| warning.message.contains(message)),
            "missing {message:?}: {markdown_warnings:?}"
        );
    }
    assert!(
        markdown_warnings
            .iter()
            .all(|warning| warning.advice().is_some()),
        "{markdown_warnings:?}"
    );
    assert!(html_warnings.is_empty(), "{html_warnings:?}");
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
        "Target macro: [[Target raw Title]](#Target%20raw%20Title).",
        "Target plus: [[Target raw Title]](#Target%20raw%20Title).",
        "Missing macro: [[Missing raw Title]](#Missing%20raw%20Title).",
        "Missing plus: [[Missing raw Title]](#Missing%20raw%20Title).",
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

#[test]
fn gfm_table_fallbacks_report_each_unsupported_capability() -> Result<(), Error> {
    let input = include_str!("fixtures/source/table_fallbacks.adoc");
    let (_output, warnings) = convert_str(input)?;

    for message in [
        "headerless tables are not supported",
        "table footers are not supported",
        "table cell spans are not supported",
        "non-default table cell styles are not fully supported",
        "nested table cell blocks are not supported",
        "table width metadata is not supported",
        "table-level and per-cell alignment are not supported",
    ] {
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.message.contains(message))
                .count(),
            1,
            "expected one {message:?} warning, got: {warnings:?}"
        );
    }
    assert_eq!(warnings.len(), 7, "unexpected warnings: {warnings:?}");
    Ok(())
}

#[test]
fn static_media_playback_warning_is_deduplicated() -> Result<(), Error> {
    let input = include_str!("fixtures/source/static_media_fallbacks.adoc");
    let (_output, warnings) = convert_str(input)?;

    assert_eq!(warnings.len(), 1, "unexpected warnings: {warnings:?}");
    let warning = warnings.first().ok_or("missing playback warning")?;
    assert!(
        warning
            .message
            .contains("audio and video playback are not supported"),
        "unexpected warning: {warnings:?}"
    );
    assert!(warning.advice().is_some(), "missing advice: {warnings:?}");
    Ok(())
}

#[test]
fn stem_fixtures_preserve_expressions_and_structured_warnings() -> Result<(), Error> {
    for variant in [MarkdownVariant::GitHubFlavored, MarkdownVariant::CommonMark] {
        let (inline_output, inline_warnings) = convert_str_with_variant(
            include_str!("fixtures/source/inline_meaning_links.adoc"),
            variant,
        )?;
        let inline_stem_warnings = inline_warnings
            .iter()
            .filter(|warning| warning.message.contains("inline STEM is not supported"))
            .collect::<Vec<_>>();
        assert_eq!(
            inline_stem_warnings.len(),
            1,
            "{variant}: {inline_warnings:?}"
        );
        assert!(
            inline_stem_warnings
                .iter()
                .all(|warning| warning.advice().is_some()),
            "{variant}: {inline_warnings:?}"
        );
        for expected in [
            "<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>",
            "**[Save]**",
            "File > Open > Recent",
            "[Back]",
            "`x < y`",
            "`a_b`",
        ] {
            assert!(
                inline_output.contains(expected),
                "expected {expected:?} for {variant} in {inline_output}"
            );
        }

        let (block_output, block_warnings) = convert_str_with_variant(
            include_str!("fixtures/source/block_attribute_parity.adoc"),
            variant,
        )?;
        let block_stem_warnings = block_warnings
            .iter()
            .filter(|warning| warning.message.contains("block STEM is not supported"))
            .collect::<Vec<_>>();
        assert_eq!(
            block_stem_warnings.len(),
            1,
            "{variant}: {block_warnings:?}"
        );
        assert!(
            block_stem_warnings
                .iter()
                .all(|warning| warning.advice().is_some()),
            "{variant}: {block_warnings:?}"
        );
        assert!(
            block_output.contains("sqrt(4) = 2"),
            "{variant}: {block_output}"
        );

        let events = parse_markdown(&block_output, variant);
        assert!(
            events.iter().any(|event| {
                matches!(
                    event,
                    Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                        if info.as_ref() == "asciimath"
                )
            }),
            "missing parsed asciimath block for {variant}: {events:?}"
        );
    }
    Ok(())
}

#[test]
fn linked_index_catalog_requires_both_opt_ins() -> Result<(), Error> {
    let body = "== Terms\n\nVisible ((Cats)) and concealed (((Animals, Mammals))).\n";
    let index = "\n[index]\n== Index\n";

    let (section_only, _) = convert_str(&format!("= Doc\n\n{body}{index}"))?;
    let (attribute_only, _) = convert_str(&format!("= Doc\n:acdc-index:\n\n{body}"))?;
    for output in [&section_only, &attribute_only] {
        assert!(!output.contains("_indexterm_"), "{output}");
        assert!(!output.contains("### A"), "{output}");
    }

    for variant in [MarkdownVariant::GitHubFlavored, MarkdownVariant::CommonMark] {
        let (enabled, _) =
            convert_str_with_variant(&format!("= Doc\n:acdc-index:\n\n{body}{index}"), variant)?;
        for expected in [
            "<a id=\"_indexterm_0\"></a>Cats",
            "<a id=\"_indexterm_1\"></a>",
            "### A",
            "- Animals",
            "  - Mammals — [Terms](#_indexterm_1)",
            "### C",
            "- Cats — [Terms](#_indexterm_0)",
        ] {
            assert!(
                enabled.contains(expected),
                "missing {expected:?} for {variant}: {enabled}"
            );
        }
    }
    Ok(())
}

#[test]
fn document_wide_fallback_warnings_are_deduplicated_across_subvisitors() -> Result<(), Error> {
    let input = "stem:[body]\n\nFootnotes footnote:[stem:[first]] and footnote:[stem:[second]].\n\n<<<\n\n<<<\n\n[upperalpha]\n. One\n\nParagraph.\n\n[lowerroman]\n. Two\n";
    let (_output, warnings) = convert_str(input)?;

    for message in [
        "inline STEM is not supported",
        "page breaks not natively supported",
        "non-numeric ordered list numbering styles not natively supported",
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
        warnings.iter().all(|warning| warning.advice().is_some()),
        "{warnings:?}"
    );
    Ok(())
}

#[test]
fn unhandled_blocks_emit_a_structured_warning_with_source_context() -> Result<(), Error> {
    let parsed = acdc_parser::parse("Paragraph.\n", &ParserOptions::default())?;
    let doc = parsed.document();
    let block = doc.blocks.first().ok_or("missing test block")?;
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("markdown");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    {
        let mut visitor = MarkdownVisitor::new(&mut output, processor, diagnostics.reborrow());
        visitor.visit_unhandled_block(block)?;
    }

    assert!(output.is_empty());
    let warning = warnings.first().ok_or("missing fallback warning")?;
    assert!(
        warning.message.contains("unknown parser block feature"),
        "{warnings:?}"
    );
    assert!(warning.advice().is_some(), "{warnings:?}");
    assert!(warning.source_location().is_some(), "{warnings:?}");
    Ok(())
}
