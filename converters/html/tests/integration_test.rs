use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    error::Error as StdError,
    fs::read_to_string,
    path::{Path, PathBuf},
};

use acdc_converters_core::{
    Converter, Diagnostics, GeneratorMetadata, Options as ConverterOptions, TraversalContext,
    WarningSource, visitor::Visitor,
};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_html::{HtmlVariant, HtmlVisitor, Processor, RenderOptions};
use acdc_parser::{AttributeValue, Options as ParserOptions, SafeMode, parse, parse_file};

mod support;

type Error = Box<dyn StdError>;

#[rstest::rstest]
#[case::absent("= T\n\n", false, false)]
#[case::named("= T\n:source-highlighter: syntect\n\n", true, false)]
#[case::empty("= T\n:source-highlighter:\n\n", true, false)]
#[case::unset(
    "= T\n:source-highlighter: rouge\n:source-highlighter!:\n\n",
    false,
    false
)]
#[case::caller_unset("= T\n:source-highlighter: syntect\n\n", false, true)]
#[case::verbatim("= T\n\n----\n:source-highlighter: syntect\n----\n\n", false, false)]
#[case::comment(
    "= T\n\nParagraph.\n\n////\n:source-highlighter: syntect\n////\n\n",
    false,
    false
)]
#[case::body("= T\n\nParagraph.\n\n:source-highlighter: syntect\n\n", false, false)]
fn fixture_expectations_follow_effective_header_highlighter(
    #[case] prefix: &str,
    #[case] highlighter: bool,
    #[case] caller_unset: bool,
) -> Result<(), Error> {
    let input = format!("{prefix}[source,rust]\n----\nfn main() {{}}\n----\n");
    let mut options = ParserOptions::builder();
    if caller_unset {
        options = options.with_attribute("source-highlighter", false);
    }
    let parsed = parse(&input, &options.build()?)?;
    let expected = if !cfg!(feature = "highlighting") && highlighter {
        "example.no-highlighting.html"
    } else {
        "example.html"
    };
    assert_eq!(
        support::expected_fixture_path(Path::new("expected"), "example", parsed.document()),
        Path::new("expected").join(expected)
    );
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = render_fixture_document(parsed.document(), variant, true)?;
        assert_eq!(
            output.contains("<span style=\"color:"),
            cfg!(feature = "highlighting") && highlighter,
            "{output}"
        );
    }
    Ok(())
}

#[cfg(not(feature = "highlighting"))]
#[test]
fn plain_source_keeps_text_callouts_and_wrap_options() -> Result<(), Error> {
    for (directory, variant) in [
        ("html", HtmlVariant::Standard),
        ("html5s", HtmlVariant::Semantic),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/source/{directory}/embedded/source_line_options.adoc"
        ));
        let output = render_fixture(&path, variant, true)?;
        assert!(!output.contains("<span style=\"color:"), "{output}");
        assert!(!output.contains("syntax-"), "{output}");
        assert!(!output.contains("<table"), "{output}");
        assert_eq!(output.matches("<pre").count(), 14, "{output}");
        for text in [
            "fn ten() {}\nfn eleven() {}",
            "fn invalid_start() {}",
            "plain one\nplain two",
            "fn paragraph_one() {}\nfn paragraph_two() {}",
            "fn callout_one() {} <b class=\"conum\">(1)</b>",
            "First callout.",
            "Second callout.",
            "fn after_unset() {}",
        ] {
            assert!(output.contains(text), "missing {text}: {output}");
        }
        assert_eq!(output.matches("class=\"highlight nowrap\"").count(), 2);
    }
    Ok(())
}

#[cfg(all(feature = "pre-spec-subs", not(feature = "highlighting")))]
#[test]
fn plain_code_keeps_enabled_macros_and_literal_controls() -> Result<(), Error> {
    for (directory, variant) in [
        ("html", HtmlVariant::Standard),
        ("html5s", HtmlVariant::Semantic),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/source/{directory}/embedded/subs_highlight_attributes.adoc"
        ));
        let output = render_fixture(&path, variant, true)?;
        check_link_structure(&output, "plain_highlight_attributes")?;
        assert!(!output.contains("syntax-"), "{output}");
        assert!(!output.contains("<span style=\"color:"), "{output}");
        assert!(!output.contains("<table"), "{output}");
        for text in [
            "href=\"https://example.org/highlight\"",
            "href=\"#target\"",
            "href=\"#_indexterm_0\"",
            "id=\"_indexterm_1\"",
            "Visible &#174;",
            "H04 &#169;",
            "<b class=\"conum\">(1)</b>",
            "Attached callout.",
            "Shared note.",
            "H06 footnote:[Disabled note.] indexterm2:[Disabled term]",
        ] {
            assert!(output.contains(text), "missing {text}: {output}");
        }
        assert!(!output.contains("href=\"#_indexterm_2\""), "{output}");
    }
    Ok(())
}

#[test]
fn formatted_attribute_footnotes_keep_unique_link_targets() -> Result<(), Error> {
    check_link_label_footnote_targets("document_attribute_formatted")?;
    check_link_label_footnote_targets("document_attribute_profile_edges")?;
    #[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
    check_link_label_footnote_targets("subs_document_attribute_formatted")?;
    Ok(())
}

#[cfg(all(feature = "highlighting", feature = "pre-spec-subs"))]
#[test]
fn document_attribute_pass_raw_tags_remain_nested_under_highlighting() -> Result<(), Error> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/source/html/embedded/subs_document_attribute_pass_highlighting.adoc");
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = render_fixture(&path, variant, true)?;
        let mut open = Vec::new();
        for fragment in output.split('<').skip(1) {
            let Some((tag, _)) = fragment.split_once('>') else {
                continue;
            };
            if tag.starts_with("span ") || tag == "span" {
                open.push("span");
            } else if tag == "em" || tag.starts_with("em ") {
                open.push("em");
            } else if let Some(close) = tag.strip_prefix('/')
                && matches!(close, "em" | "span")
            {
                assert_eq!(open.pop(), Some(close), "{output}");
            }
        }
        assert!(open.is_empty(), "{output}");
        assert!(output.contains("<em>"), "{output}");
        assert!(output.contains("Generated {name}"), "{output}");
        assert!(output.contains("<em title=\"a > b\">"), "{output}");
        assert!(output.contains("<em title='x > y'>"), "{output}");
        assert!(output.contains("<!-- a > b -->"), "{output}");
    }
    Ok(())
}

#[test]
fn nested_links_keep_separate_anchors_and_targets() -> Result<(), Error> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/source/html/embedded/nested_links.adoc");
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        for embedded in [true, false] {
            let output = render_fixture(&path, variant, embedded)?;
            check_link_structure(&output, "nested_links")?;
            for target in [
                "mailto:first@example.org",
                "mailto:only@example.org",
                "https://image.example",
                "https://middle.example",
            ] {
                assert!(
                    output.contains(&format!("href=\"{target}\"")),
                    "missing target {target}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn link_label_quotes_keep_footnote_links_separate() -> Result<(), Error> {
    check_link_label_footnote_targets("link_label_quotes")
}

#[test]
fn link_label_brackets_keep_footnote_links_separate() -> Result<(), Error> {
    check_link_label_footnote_targets("link_label_brackets")
}

#[test]
fn passthrough_brackets_keep_unique_link_targets() -> Result<(), Error> {
    check_link_label_footnote_targets("passthrough_brackets")?;
    #[cfg(feature = "pre-spec-subs")]
    check_link_label_footnote_targets("subs_passthrough_brackets")?;
    Ok(())
}

#[test]
fn anchor_macros_keep_unique_targets_without_nested_links() -> Result<(), Error> {
    check_link_label_footnote_targets("anchor_macro")?;
    #[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
    check_link_label_footnote_targets("subs_anchor_macro_highlighting")?;
    Ok(())
}

#[test]
fn standalone_callouts_keep_unique_link_targets() -> Result<(), Error> {
    check_link_label_footnote_targets("standalone_callouts")
}

#[test]
fn callout_nested_lists_keep_unique_link_targets() -> Result<(), Error> {
    check_link_label_footnote_targets("callout_nested_lists")
}

#[test]
fn xref_nested_footnotes_have_separate_unique_targets() -> Result<(), Error> {
    check_link_label_footnote_targets("xref_nested_footnotes")?;
    #[cfg(feature = "pre-spec-subs")]
    check_link_label_footnote_targets("subs_xref_nested_footnotes")?;
    #[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
    check_link_label_footnote_targets("subs_xref_nested_footnotes_highlighting")?;
    Ok(())
}

#[test]
fn footnotes_in_link_labels_have_separate_unique_targets() -> Result<(), Error> {
    check_link_label_footnote_targets("footnotes_in_link_labels")
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_footnotes_in_link_labels_have_separate_unique_targets() -> Result<(), Error> {
    check_link_label_footnote_targets("subs_footnotes_in_link_labels")
}

#[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
#[test]
fn highlighted_footnotes_in_link_labels_have_separate_unique_targets() -> Result<(), Error> {
    check_link_label_footnote_targets("subs_footnotes_in_link_labels_highlighting")
}

fn check_link_label_footnote_targets(name: &str) -> Result<(), Error> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/source/html/embedded/{name}.adoc"));
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = render_fixture(&path, variant, true)?;
        check_link_structure(&output, name)?;
    }
    Ok(())
}

fn check_link_structure(output: &str, name: &str) -> Result<(), Error> {
    let mut inside_anchor = false;
    let mut footnote_links = 0;
    let mut ids = HashSet::new();
    for part in output.split('<').skip(1) {
        let tag = part.split_once('>').ok_or("unterminated HTML tag")?.0;
        if tag.starts_with("a ") {
            assert!(!inside_anchor, "nested anchor in {name}: <{tag}>");
            inside_anchor = true;
            footnote_links += usize::from(tag.contains("href=\"#_footnote"));
        } else if tag == "/a" {
            assert!(inside_anchor, "unmatched closing anchor in {name}");
            inside_anchor = false;
        }
        if let Some((_, tail)) = tag.split_once(" id=\"") {
            let id = tail.split_once('"').ok_or("unterminated ID")?.0;
            assert!(ids.insert(id), "duplicate ID {id} in {name}");
        }
    }
    assert!(!inside_anchor, "unclosed anchor in {name}");
    assert!(footnote_links > 0, "no footnote links in {name}");
    for tail in output.split("href=\"#").skip(1) {
        let id = tail.split_once('"').ok_or("unterminated link")?.0;
        assert!(ids.contains(id), "missing target {id} in {name}");
    }
    Ok(())
}

#[test]
fn index_relationships_target_their_own_catalog() -> Result<(), Error> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/source/html/embedded/index_multiple_catalogs.adoc");
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = render_fixture(&path, variant, true)?;
        let mut links = 0;
        for tail in output.split("href=\"#_indextermdef_").skip(1) {
            let (suffix, _) = tail.split_once('"').ok_or("missing link terminator")?;
            assert_eq!(
                output
                    .matches(&format!("id=\"_indextermdef_{suffix}\""))
                    .count(),
                1
            );
            links += 1;
        }
        assert_eq!(links, 5);
    }
    Ok(())
}

#[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
#[test]
fn highlighted_index_links_resolve_inside_code() -> Result<(), Error> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/source/html/embedded/subs_verbatim_index_highlighting.adoc");
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = render_fixture(&path, variant, true)?;
        let mut count = 0;
        for tail in output.split("href=\"#_indexterm_").skip(1) {
            let (number, _) = tail.split_once('"').ok_or("missing link terminator")?;
            let anchor = format!("id=\"_indexterm_{number}\"");
            assert_eq!(output.matches(&anchor).count(), 1);
            let (prefix, _) = output.split_once(&anchor).ok_or("missing anchor")?;
            assert!(
                prefix
                    .rfind("<pre")
                    .is_some_and(|start| { prefix.rfind("</pre>").is_none_or(|end| start > end) })
            );
            count += 1;
        }
        assert_eq!(count, 3);
    }
    Ok(())
}

#[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
#[test]
fn index_dash_context_keeps_highlighted_link_offsets_and_catalog_targets() -> Result<(), Error> {
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        for mode in ["inline", "class", "fallback"] {
            let input = ":acdc-index:\n:source-highlighter: syntect\n\n\
                [source,text,subs=\"specialchars,macros,replacements\"]\n----\n\
                prefixindexterm:[Hidden]indexterm2:[--]tail link:https://next.example/context[Next]\n\
                prefixindexterm2:[--]tail link:https://next.example/second[Second]\n\
                prefixindexterm2:[--]link:https://next.example/tail[Tail]\n\
                link:https://next.example/before[Before]indexterm2:[--]tail\n\
                prefix indexterm2:[--] tail link:https://next.example/spaced[Spaced]\n\
                prefix-indexterm2:[-]tail link:https://next.example/split[Split]\n\
                indexterm2:[Sam']s link:https://next.example/apostrophe[Apostrophe]\n\
                ----\n\n[index]\n== Index\n";
            let attributes = match mode {
                "class" => vec![("highlight-css", AttributeValue::String("class".into()))],
                "fallback" => vec![(
                    "highlight-style",
                    AttributeValue::String("missing-theme".into()),
                )],
                _ => Vec::new(),
            };
            let output = convert_string_with_variant(input, &attributes, variant)?;
            if mode == "class" {
                assert!(output.contains("class=\"syntax-"), "{output}");
            }
            for (target, label) in [
                ("context", "Next"),
                ("second", "Second"),
                ("tail", "Tail"),
                ("before", "Before"),
                ("spaced", "Spaced"),
                ("split", "Split"),
                ("apostrophe", "Apostrophe"),
            ] {
                let link = format!("<a href=\"https://next.example/{target}\">{label}</a>");
                assert_eq!(output.matches(&link).count(), 1, "{mode}: {output}");
                assert_eq!(output.matches(label).count(), 1, "{mode}: {output}");
            }
            assert!(output.contains("—\u{200b}"), "{mode}: {output}");
            let (_, catalog) = output
                .split_once("class=\"indexterms\"")
                .ok_or("missing index catalog")?;
            assert!(catalog.contains(">--\n"), "{mode}: {output}");
            let mut in_tag = false;
            let visible = output
                .chars()
                .filter(|character| match character {
                    '<' => {
                        in_tag = true;
                        false
                    }
                    '>' => {
                        in_tag = false;
                        false
                    }
                    _ => !in_tag,
                })
                .collect::<String>();
            assert!(visible.contains("prefix--Tail"), "{mode}: {output}");
            assert!(visible.contains("Before--tail"), "{mode}: {output}");
            assert!(
                visible.contains("prefix\u{2009}—\u{2009}tail Spaced"),
                "{mode}: {output}"
            );
            assert!(
                visible.contains("prefix—\u{200b}tail Split"),
                "{mode}: {output}"
            );
            assert!(visible.contains("Sam’s Apostrophe"), "{mode}: {output}");
            for number in 0..8 {
                assert_eq!(
                    output
                        .matches(&format!("id=\"_indexterm_{number}\""))
                        .count(),
                    1,
                    "{mode}: {output}"
                );
                assert_eq!(
                    catalog
                        .matches(&format!("href=\"#_indexterm_{number}\""))
                        .count(),
                    1,
                    "{mode}: {output}"
                );
            }
            assert!(!output.contains("_indexterm_8"), "{mode}: {output}");
        }
    }
    Ok(())
}

#[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
#[rstest::rstest]
#[case::link_labels("subs_footnotes_in_link_labels_highlighting")]
#[case::cross_references("subs_xref_nested_footnotes_highlighting")]
#[case::verbatim_footnotes("subs_verbatim_footnotes_highlighting")]
#[case::visible_indexes("subs_visible_index_typography_class")]
fn class_highlighting_fixtures_select_class_spans(#[case] stem: &str) -> Result<(), Error> {
    for (directory, variant) in [
        ("html", HtmlVariant::Standard),
        ("html5s", HtmlVariant::Semantic),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/source/{directory}/embedded/{stem}.adoc"
        ));
        let output = render_fixture(&path, variant, true)?;
        assert!(
            output.contains("class=\"syntax-"),
            "{directory}/{stem}: {output}"
        );
        assert!(
            !output.contains("style=\"color:"),
            "{directory}/{stem}: {output}"
        );
    }
    Ok(())
}

#[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
#[rstest::rstest]
#[case::primary("subs_highlight_attributes", true)]
#[case::deprecated("subs_highlight_attributes_legacy", true)]
#[case::unset("subs_highlight_attributes_unset", false)]
fn highlight_attribute_fixtures_preserve_mode_and_targets(
    #[case] stem: &str,
    #[case] class_mode: bool,
) -> Result<(), Error> {
    for (directory, variant) in [
        ("html", HtmlVariant::Standard),
        ("html5s", HtmlVariant::Semantic),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/source/{directory}/embedded/{stem}.adoc"
        ));
        let output = render_fixture(&path, variant, true)?;
        assert_eq!(
            output.contains("class=\"syntax-"),
            class_mode,
            "{stem}: {output}"
        );
        assert_eq!(
            output.contains("style=\"color:"),
            !class_mode,
            "{stem}: {output}"
        );
        check_link_structure(&output, stem)?;
    }
    Ok(())
}

#[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
#[test]
fn visible_index_typography_keeps_following_links_and_unique_targets() -> Result<(), Error> {
    for stem in [
        "subs_visible_index_typography_highlighting",
        "subs_visible_index_typography_class",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/source/html/embedded/{stem}.adoc"));
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let output = render_fixture(&path, variant, true)?;
            for number in (4..=23).chain([28]) {
                let label = format!("Next{number:02}");
                let link = format!("<a href=\"https://next.example/{number:02}\">{label}</a>");
                assert_eq!(output.matches(&link).count(), 1, "{stem}: {output}");
                assert_eq!(output.matches(&label).count(), 1, "{stem}: {output}");
            }
            let mut occurrences = HashSet::new();
            for tail in output.split("href=\"#_indexterm_").skip(1) {
                let (number, _) = tail
                    .split_once('"')
                    .ok_or("missing index link terminator")?;
                assert!(
                    occurrences.insert(number),
                    "duplicate index entry: {output}"
                );
                let anchor = format!("id=\"_indexterm_{number}\"");
                assert_eq!(output.matches(&anchor).count(), 1, "{stem}: {output}");
            }
            assert!(!occurrences.is_empty());
        }
    }
    Ok(())
}

#[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
#[test]
fn visible_index_typography_keeps_registration_labels_and_source_order() -> Result<(), Error> {
    let input = ":source-highlighter: syntect\n:acdc-index:\n\n\
        [source,rust,subs=\"+macros,+replacements\"]\n----\n\
        (((Hidden first)))indexterm2:[One (C)] https://next.example[Next]\n\
        indexterm2:[Two (R)](((Hidden last)))\n----\n\n[index]\n== Index\n";
    for mode in ["inline", "class"] {
        let output = convert_string(
            input,
            &[("highlight-css", AttributeValue::String(mode.into()))],
        )?;
        assert_eq!(
            output.contains("class=\"syntax-"),
            mode == "class",
            "{mode}: {output}"
        );
        assert_eq!(
            output.contains("style=\"color:"),
            mode == "inline",
            "{mode}: {output}"
        );
        let (_, catalog) = output
            .split_once("class=\"indexterms\"")
            .ok_or("missing index catalog")?;
        for (number, label) in [
            (0, "Hidden first"),
            (1, "One (C)"),
            (2, "Two (R)"),
            (3, "Hidden last"),
        ] {
            let id = format!("id=\"_indexterm_{number}\"");
            assert_eq!(output.matches(&id).count(), 1, "{output}");
            let (_, entry) = catalog
                .split_once(label)
                .ok_or("missing registration label")?;
            let (entry, _) = entry.split_once("</dt>").ok_or("missing index entry end")?;
            assert!(
                entry.contains(&format!("href=\"#_indexterm_{number}\"")),
                "{output}"
            );
        }
        assert!(
            !output.contains("_indexterm_4"),
            "unexpected registration: {output}"
        );
    }
    Ok(())
}

#[cfg(all(feature = "pre-spec-subs", feature = "highlighting"))]
#[test]
fn visible_index_typography_survives_missing_theme_fallback() -> Result<(), Error> {
    let input = read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "tests/fixtures/source/html/embedded/subs_visible_index_typography_highlighting.adoc",
    ))?;
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = convert_string_with_variant(
            &input,
            &[(
                "highlight-style",
                AttributeValue::String("missing-theme".into()),
            )],
            variant,
        )?;
        for line in [
            "I04 Code © ® ™ After <a href=\"https://next.example/04\">Next04</a>",
            "I08 (C) Escaped After <a href=\"https://next.example/08\">Next08</a>",
            "I12 © Decoded After <a href=\"https://next.example/12\">Next12</a>",
            "I22 © Own enabled After <a href=\"https://next.example/22\">Next22</a>",
        ] {
            assert!(output.contains(line), "{variant:?}: {output}");
        }
        assert!(
            output.contains("class=\"conum\" data-value=\"1\""),
            "{output}"
        );
    }
    Ok(())
}

#[test]
fn index_catalog_links_resolve_to_unique_occurrences() -> Result<(), Error> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/source/html/embedded/index_placement.adoc");
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = render_fixture(&path, variant, true)?;
        let mut links = 0;
        for tail in output.split("href=\"#_indexterm_").skip(1) {
            let (number, _) = tail.split_once('"').ok_or("missing link terminator")?;
            let anchor = format!("id=\"_indexterm_{number}\"");
            assert_eq!(output.matches(&anchor).count(), 1, "{output}");
            links += 1;
        }
        assert!(links > 0, "the fixture must contain catalog links");
    }
    Ok(())
}

#[test]
fn bibliography_conversion_keeps_parent_safe_mode() -> Result<(), Error> {
    for mode in [
        SafeMode::Unsafe,
        SafeMode::Safe,
        SafeMode::Server,
        SafeMode::Secure,
    ] {
        let parsed = parse(
            "[bibliography]\n* [[[ref,pass:a[{safe-mode-name}; {safe-mode-level}]]]] Entry.\n* [[[home,pass:a[{user-home}]]]] Home.\n",
            &ParserOptions::builder().with_safe_mode(mode).build()?,
        )?;
        let processor = Processor::new_with_variant(
            ConverterOptions::builder().safe_mode(mode).build(),
            ParserOptions::builder(),
            HtmlVariant::Standard,
        )?;
        let mut output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.convert_to_writer(
            parsed.document(),
            &mut output,
            &RenderOptions {
                embedded: true,
                ..RenderOptions::default()
            },
            &mut diagnostics,
        )?;
        let html = String::from_utf8(output)?;
        assert!(
            html.contains(&format!("[{}; {}] Entry.", mode.name(), mode.level())),
            "{html}"
        );
        if mode >= SafeMode::Server {
            assert!(html.contains("[.] Home."), "{html}");
        }
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    Ok(())
}

#[test]
fn oversized_source_indent_is_a_bounded_structured_warning() -> Result<(), Error> {
    for highlighter in [false, true] {
        for assignment in [
            ":source-indent: 18446744073709551615\n\n[source]",
            "\n[source,indent=18446744073709551615]",
        ] {
            let input = format!("= T\n{assignment}\n----\n    text\n----\n");
            let options = ParserOptions::builder()
                .with_attribute("source-highlighter", highlighter)
                .build()?;
            let parsed = parse(&input, &options)?;
            let attributes = parsed.document().attributes.clone();
            let processor = Processor::new_with_variant(
                ConverterOptions::default(),
                ParserOptions::builder().with_attributes(attributes.into_inputs()),
                HtmlVariant::Standard,
            )?;
            let mut output = Vec::new();
            let mut warnings = Vec::new();
            let source = WarningSource::new("html");
            let mut diagnostics = Diagnostics::new(&source, &mut warnings);
            processor.convert_to_writer(
                parsed.document(),
                &mut output,
                &RenderOptions {
                    embedded: true,
                    ..RenderOptions::default()
                },
                &mut diagnostics,
            )?;
            assert!(output.len() < 2048);
            assert!(warnings.iter().any(|warning| {
                warning.message.contains("unsupported source indentation")
                    && warning.advice().is_some()
            }));
        }
    }
    Ok(())
}

fn temp_output_path(name: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!("acdc-{name}-{}.{extension}", std::process::id()))
}

fn run_fixture_test(
    path: &Path,
    expected_dir: &Path,
    variant: HtmlVariant,
    embedded: bool,
) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;

    if support::skip_fixture(file_name) {
        return Ok(());
    }

    let parsed = parse_file(path, &ParserOptions::default())?;
    // Require both baselines even in highlighting-enabled CI, so new inputs
    // cannot omit the plain rendering coverage.
    if support::has_highlighter(parsed.document()) {
        let plain_path = expected_dir
            .join(file_name)
            .with_extension("no-highlighting.html");
        assert!(plain_path.is_file(), "missing {}", plain_path.display());
    }
    let expected_path = support::expected_fixture_path(expected_dir, file_name, parsed.document());

    let actual = render_fixture_document(parsed.document(), variant, embedded)?;
    let expected = read_to_string(&expected_path)?;
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "HTML output mismatch for fixture: {file_name}",
    );
    Ok(())
}

fn render_fixture(path: &Path, variant: HtmlVariant, embedded: bool) -> Result<String, Error> {
    let parser_options = ParserOptions::default();
    let parsed = parse_file(path, &parser_options)?;
    render_fixture_document(parsed.document(), variant, embedded)
}

fn render_fixture_document(
    doc: &acdc_parser::Document<'_>,
    variant: HtmlVariant,
    embedded: bool,
) -> Result<String, Error> {
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = Processor::new_with_variant(
        converter_options,
        ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
        variant,
    )?;
    let render_options = RenderOptions {
        embedded,
        ..RenderOptions::default()
    };

    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("html");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.convert_to_writer(doc, &mut output, &render_options, &mut diagnostics)?;

    Ok(String::from_utf8(output)?)
}

#[rstest::rstest]
#[case::bibliography_recognition(
    r"= References
:label: a[b]c

Citations: <<brackets>>, <<note>>, <<escaped>>, <<multiline>>.

[bibliography]
* [[[brackets,{label}]]] Bracketed attribute.
* [[[note,footnote:[A note.] +]]] Footnote label.
* [[[escaped,\pass:[raw]]]] Escaped macro.
* [[[multiline,pass:a[first
second]]]] Multiline passthrough.
",
    &[
        r##"Citations: <a href="#brackets">[{label}]</a>, <a href="#note">[footnote:[A note.] +]</a>, <a href="#escaped">[\pass:[raw]</a>, <a href="#multiline">[multiline]</a>."##,
        r#"<a id="brackets"></a>[a[b]c] Bracketed attribute."#,
        r##"<a id="note"></a>[<sup class="footnote">[<a id="_footnoteref_1" class="footnote" href="#_footnotedef_1" title="View footnote.">1</a>]</sup> +] Footnote label."##,
        r#"<a id="escaped"></a>[pass:[raw]] Escaped macro."#,
        r#"<a id="multiline"></a>[first
second] Multiline passthrough."#,
    ],
    &[
        r##"
<a href="#_footnoteref_1">1</a>. A note."##,
    ],
)]
#[case::toc_attribute_normalization(
    r#"= TOC attributes
:toc: left

Root: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].

[cols="2*a",options="noheader"]
|===
|
:toc: macro

Child: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
|
Sibling: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
|===

Parent: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
"#,
    &[
        r"Root: [] [left] [auto] [toc2].",
        r"Child: [] [content] [macro] [toc2].",
        r"Sibling: [{toc}] [{toc-position}] [auto] [toc2].",
        r"Parent: [] [left] [auto] [toc2].",
    ],
    &[],
)]
#[case::toc_nested_body(
    r"= T
:toc: left

[cols=a]
|===
|
Before.

:toc: right

Text: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
|===
",
    &[
        r"Before.",
        r"Text: [right] [{toc-position}] [auto] [toc2].",
    ],
    &[],
)]
#[case::toc_nested_class(
    r"= T

[cols=a]
|===
|
:toc: left
:toc-class: child

Child: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
|===

Parent: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
",
    &[
        r"Child: [] [left] [auto] [child].",
        r"Parent: [{toc}] [{toc-position}] [auto] [{toc-class}].",
    ],
    &[],
)]
#[case::toc_nested_comments(
    r"= T
:toc: left

[cols=a]
|===
|
// Comment
:toc: right

Text: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
|===
",
    &[
        r"Text: [] [right] [auto] [toc2].",
    ],
    &[],
)]
#[case::toc_nested_gap(
    r"= T
:toc: left

[cols=a]
|===
|
// Comment

:toc: right

Text: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
|===
",
    &[
        r"Text: [] [right] [auto] [toc2].",
    ],
    &[],
)]
#[case::toc_nested_header(
    r"= T
:toc: left

[cols=a]
|===
|
:toc: right

Text: [{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].
|===
",
    &[
        r"Text: [] [right] [auto] [toc2].",
    ],
    &[],
)]
fn inline_html_preserves_attribute_context(
    #[case] input: &str,
    #[case] paragraphs: &[&str],
    #[case] footnotes: &[&str],
) -> Result<(), Error> {
    let html = convert_string(input, &[])?;
    for (open, close, expected) in [
        ("<p", "</p>", paragraphs),
        ("<div class=\"footnote\"", "</div>", footnotes),
    ] {
        let actual: Vec<_> = html
            .split(open)
            .skip(1)
            .filter_map(|part| {
                let (attributes, rest) = part.split_once('>')?;
                if !attributes.is_empty() && !attributes.starts_with([' ', '>']) {
                    return None;
                }
                let content = rest.split_once(close)?.0;
                Some(if open == "<p" {
                    content
                } else {
                    content.trim_end()
                })
            })
            .collect();
        assert_eq!(actual, expected, "{input}");
    }
    Ok(())
}

#[rstest::rstest]
#[tracing_test::traced_test]
fn test_with_fixtures(
    #[files("tests/fixtures/source/html/embedded/*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    run_fixture_test(
        &path,
        Path::new("tests/fixtures/expected/html/embedded"),
        HtmlVariant::Standard,
        true,
    )
}

#[rstest::rstest]
#[tracing_test::traced_test]
fn test_standalone_with_fixtures(
    #[files("tests/fixtures/source/html/standalone/*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    run_fixture_test(
        &path,
        Path::new("tests/fixtures/expected/html/standalone"),
        HtmlVariant::Standard,
        false,
    )
}

#[rstest::rstest]
#[tracing_test::traced_test]
fn test_html5s_with_fixtures(
    #[files("tests/fixtures/source/html5s/embedded/*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    run_fixture_test(
        &path,
        Path::new("tests/fixtures/expected/html5s/embedded"),
        HtmlVariant::Semantic,
        true,
    )
}

#[rstest::rstest]
#[tracing_test::traced_test]
fn test_html5s_standalone_with_fixtures(
    #[files("tests/fixtures/source/html5s/standalone/*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    run_fixture_test(
        &path,
        Path::new("tests/fixtures/expected/html5s/standalone"),
        HtmlVariant::Semantic,
        false,
    )
}

/// Helper: convert an `AsciiDoc` string to full-page HTML with custom attributes.
fn convert_string(input: &str, extra_attrs: &[(&str, AttributeValue)]) -> Result<String, Error> {
    convert_string_with_variant(input, extra_attrs, HtmlVariant::Standard)
}

fn convert_string_with_variant(
    input: &str,
    extra_attrs: &[(&str, AttributeValue)],
    variant: HtmlVariant,
) -> Result<String, Error> {
    let mut attrs = HashMap::<Cow<'_, str>, AttributeValue<'_>>::new();
    for (k, v) in extra_attrs {
        attrs.insert((*k).into(), v.clone());
    }
    let parser_options = ParserOptions::with_attributes(attrs)?;
    let parsed = parse(input, &parser_options)?;
    let doc = parsed.document();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = Processor::new_with_variant(
        converter_options,
        ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
        variant,
    )?;
    let render_options = RenderOptions::default();
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("html");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.convert_to_writer(doc, &mut output, &render_options, &mut diagnostics)?;
    Ok(String::from_utf8(output)?)
}

#[test]
fn standalone_description_and_keywords_are_escaped_when_present() -> Result<(), Error> {
    let populated = "= Metadata Test\n:description: Five < six & \"quoted\"\n:keywords: alpha, <beta>, gamma & delta\n\nBody.\n";
    let empty = "= Empty Metadata\n:description:\n:keywords:\n\nBody.\n";
    let unset = "= Unset Metadata\n\nBody.\n";

    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let html = convert_string_with_variant(populated, &[], variant)?;
        assert!(
            html.contains(
                "<meta name=\"description\" content=\"Five &lt; six &amp; &quot;quoted&quot;\">"
            ),
            "{variant}: {html}"
        );
        assert!(
            html.contains(
                "<meta name=\"keywords\" content=\"alpha, &lt;beta&gt;, gamma &amp; delta\">"
            ),
            "{variant}: {html}"
        );

        let html = convert_string_with_variant(empty, &[], variant)?;
        assert!(
            html.contains("<meta name=\"description\" content=\"\">")
                && html.contains("<meta name=\"keywords\" content=\"\">"),
            "{variant}: {html}"
        );

        let html = convert_string_with_variant(unset, &[], variant)?;
        assert!(
            !html.contains("<meta name=\"description\""),
            "{variant}: {html}"
        );
        assert!(
            !html.contains("<meta name=\"keywords\""),
            "{variant}: {html}"
        );
    }
    Ok(())
}

#[test]
fn unhandled_parser_block_warning_is_structured() -> Result<(), Error> {
    let parsed = parse("text", &ParserOptions::default())?;
    let block = parsed
        .document()
        .blocks
        .first()
        .ok_or("missing parsed block")?;
    let processor = Processor::new_with_variant(
        ConverterOptions::default(),
        ParserOptions::builder()
            .with_attributes(parsed.document().attributes.clone().into_inputs()),
        HtmlVariant::Standard,
    )?;
    let source = WarningSource::new("html").with_variant("standard");
    let mut warnings = Vec::new();
    {
        let diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header =
            acdc_converters_core::Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            std::rc::Rc::new(processor),
            RenderOptions::default(),
            diagnostics,
        );
        visitor.visit_unhandled_block(&mut traversal, block)?;
    }

    let warning = warnings.first().ok_or("missing unhandled block warning")?;
    assert_eq!(warning.source.converter, "html");
    assert_eq!(warning.source.variant.as_deref(), Some("standard"));
    assert!(warning.message.contains("omitted from HTML output"));
    assert!(warning.advice().is_some());
    Ok(())
}

#[test]
fn deprecated_role_warning_is_returned_in_conversion_result() -> Result<(), Error> {
    let parser_options = ParserOptions::default();
    let parsed = parse("[big]#large#\n", &parser_options)?;
    let doc = parsed.document();
    let converter_options = ConverterOptions::builder().embedded(true).build();
    let processor = Processor::new_with_variant(
        converter_options,
        ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
        HtmlVariant::Standard,
    )?;
    let output_path = temp_output_path("html-warning", "html");

    let result = processor.convert_to_file(doc, None, &output_path)?;
    let _ = std::fs::remove_file(&output_path);

    assert!(result.warnings().iter().any(|warning| {
        warning.source.converter == "html"
            && warning.source.variant.as_deref() == Some("standard")
            && warning.message.contains("deprecated role `big`")
    }));
    Ok(())
}

#[rstest::rstest]
#[case::primary(
    "= T\n:source-highlighter: syntect\n:highlight-css: class\n:highlight-style: InspiredGitHub\n\nLiteral syntect-css and syntect-style.\n",
    false,
    &[]
)]
#[case::unrelated("= T\n:syntect-custom: value\n\nParagraph.\n", false, &[])]
#[case::css("= T\n:syntect-css: class\n\nParagraph.\n", false, &[("syntect-css", "highlight-css")])]
#[case::style("= T\n:syntect-style:\n\nParagraph.\n", false, &[("syntect-style", "highlight-style")])]
#[case::overridden(
    include_str!("fixtures/source/html/embedded/subs_highlight_attributes.adoc"),
    false,
    &[("syntect-css", "highlight-css"), ("syntect-style", "highlight-style")]
)]
#[case::unsets(
    "= T\n:syntect-css!:\n:!syntect-style:\n\nParagraph.\n",
    false,
    &[("syntect-css", "highlight-css"), ("syntect-style", "highlight-style")]
)]
#[case::body(
    include_str!("fixtures/source/html/embedded/highlight_attribute_deprecations.adoc"),
    false,
    &[("syntect-css", "highlight-css"), ("syntect-style", "highlight-style")]
)]
#[case::nested_cells(
    include_str!("fixtures/source/html/embedded/highlight_attribute_deprecations_nested.adoc"),
    false,
    &[("syntect-css", "highlight-css"), ("syntect-style", "highlight-style")]
)]
#[case::caller(
    "= T\n\nParagraph.\n",
    true,
    &[("syntect-css", "highlight-css"), ("syntect-style", "highlight-style")]
)]
fn deprecated_highlight_attributes_warn_once_per_conversion(
    #[case] input: &str,
    #[case] caller_aliases: bool,
    #[case] expected: &[(&str, &str)],
) -> Result<(), Error> {
    let mut options = ParserOptions::builder();
    if caller_aliases {
        options = options
            .with_attribute("syntect-css", "class")
            .with_attribute("syntect-style", "InspiredGitHub");
    }
    let parsed = parse(input, &options.build()?)?;
    let doc = parsed.document();
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let processor = Processor::new_with_variant(
            ConverterOptions::default(),
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            variant,
        )?;
        let source = processor.warning_source();
        for embedded in [false, true] {
            let mut warnings = Vec::new();
            for conversion in 1..=2 {
                let mut output = Vec::new();
                let mut diagnostics = Diagnostics::new(&source, &mut warnings);
                processor.convert_to_writer(
                    doc,
                    &mut output,
                    &RenderOptions {
                        embedded,
                        ..RenderOptions::default()
                    },
                    &mut diagnostics,
                )?;
                assert_eq!(warnings.len(), expected.len() * conversion, "{warnings:?}");
                for (warning, (alias, primary)) in warnings.iter().zip(expected.iter().cycle()) {
                    assert_eq!(warning.source, source);
                    assert_eq!(
                        warning.message,
                        format!("attribute `{alias}` is deprecated")
                    );
                    assert_eq!(
                        warning.advice(),
                        Some(format!("Use `{primary}` instead.").as_str())
                    );
                }
                assert!(!String::from_utf8(output)?.contains("is deprecated"));
            }
        }
    }
    Ok(())
}

#[test]
fn deprecated_highlight_warnings_are_returned_in_conversion_result() -> Result<(), Error> {
    let parsed = parse(
        include_str!("fixtures/source/html/embedded/highlight_attribute_deprecations.adoc"),
        &ParserOptions::default(),
    )?;
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let temp = tempfile::tempdir()?;
        let processor = Processor::new_with_variant(
            ConverterOptions::builder().embedded(true).build(),
            ParserOptions::builder()
                .with_attributes(parsed.document().attributes.clone().into_inputs()),
            variant,
        )?;
        let result = processor.convert_to_file(
            parsed.document(),
            None,
            &temp.path().join("document.html"),
        )?;
        assert_eq!(result.warnings().len(), 2);
        assert!(result.warnings().iter().all(|warning| {
            warning.source == processor.warning_source()
                && warning.message.contains("is deprecated")
                && warning.advice().is_some()
        }));
    }
    Ok(())
}

#[test]
fn id_only_highlight_renders_as_a_plain_target_span() -> Result<(), Error> {
    let html = convert_string("[#mark-id]#marked text#\n", &[])?;

    assert!(
        html.contains("<span id=\"mark-id\">marked text</span>"),
        "{html}"
    );
    assert!(!html.contains("<mark id=\"mark-id\">"), "{html}");
    Ok(())
}

#[test]
fn reference_label_is_rendered_through_the_inline_pipeline() -> Result<(), Error> {
    // A label is inline content: its markup renders and its characters are
    // escaped, exactly as asciidoctor does.
    let html = convert_string(
        "Some [[bold,*Bold* label]]text.\n\n\
         Some [[unsafe,<script>alert(1)</script>]]text.\n\n\
         Some [[chars,A & B <tag> \"q\"]]text.\n\n\
         Refs: <<bold>>, <<unsafe>>, <<chars>>.\n",
        &[],
    )?;

    assert!(
        html.contains("<a href=\"#bold\"><strong>Bold</strong> label</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"#unsafe\">&lt;script&gt;alert(1)&lt;/script&gt;</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"#chars\">A &amp; B &lt;tag&gt; \"q\"</a>"),
        "{html}"
    );
    Ok(())
}

#[test]
fn a_cross_reference_inside_reference_text_is_not_a_nested_link() -> Result<(), Error> {
    // An `<a>` cannot nest, and the resolution must terminate.
    let html = convert_string(
        "[[a]]\n.See <<a>> again\n====\nbody\n====\n\nRef: <<a>>.\n",
        &[],
    )?;

    assert!(
        html.contains("Ref: <a href=\"#a\">See [a] again</a>."),
        "{html}"
    );
    Ok(())
}

#[test]
fn interdocument_xref_macros_do_not_link_to_matching_local_titles() -> Result<(), Error> {
    let input = "Empty: xref:Other.adoc[].\n\nExplicit: xref:Other.adoc[Other].\n\nShorthand: <<Other.adoc>>.\n\nFragment: xref:Foo#Bar[].\n\n== Other.adoc\n\n== Foo#Bar\n";

    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let parsed = parse(input, &ParserOptions::default())?;
        let doc = parsed.document();
        let processor = Processor::new_with_variant(
            ConverterOptions::default(),
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            variant,
        )?;
        let mut output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.convert_to_writer(
            doc,
            &mut output,
            &RenderOptions::default(),
            &mut diagnostics,
        )?;
        let html = String::from_utf8(output)?;

        for expected in [
            "Empty: <a href=\"Other.html\">Other.html</a>.",
            "Explicit: <a href=\"Other.html\">Other</a>.",
            "Shorthand: <a href=\"#_other_adoc\">Other.adoc</a>.",
            "Fragment: <a href=\"Foo.html#Bar\">Foo.html</a>.",
        ] {
            assert!(html.contains(expected), "expected {expected:?} in {html}");
        }
    }
    Ok(())
}

#[test]
fn xref_macro_role_becomes_the_link_class() -> Result<(), Error> {
    let input = "Auto: xref:fig[role=r].\n\nText: xref:fig[Text,role=\"a b\"].\n\n\
                 Styled: xref:fig[xrefstyle=short,role=r].\n\nEmpty: xref:fig[role=].\n\n\
                 External: xref:other.adoc#sec[role=r].\n\nExternal text: xref:other.adoc#sec[Other,role=r].\n\n\
                 Missing: xref:missing[role=r].\n\nShorthand: <<fig,role=r>>.\n\n\
                 [[fig]]\n.A figure\nimage::f.png[]\n";

    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let html = convert_string_with_variant(input, &[], variant)?;
        for expected in [
            "Auto: <a href=\"#fig\" class=\"r\">A figure</a>.",
            "Text: <a href=\"#fig\" class=\"a b\">Text</a>.",
            "Styled: <a href=\"#fig\" class=\"r\">Figure 1</a>.",
            // asciidoctor writes `class=""`; an empty class is left out here,
            // as it is for a `link:` macro, which renders the same.
            "Empty: <a href=\"#fig\">A figure</a>.",
            "External: <a href=\"other.html#sec\" class=\"r\">other.html</a>.",
            "External text: <a href=\"other.html#sec\" class=\"r\">Other</a>.",
            "Missing: <a href=\"#missing\" class=\"r\">[missing]</a>.",
            "Shorthand: <a href=\"#fig\">role=r</a>.",
        ] {
            assert!(html.contains(expected), "expected {expected:?} in {html}");
        }
    }
    Ok(())
}

#[test]
fn passthroughs_are_restored_before_natural_xref_resolution() -> Result<(), Error> {
    let input = "Title macro: <<Pass raw Title>>.\nTitle plus: <<Plus raw Title>>.\nTarget macro: <<Target pass:[raw] Title>>.\nTarget plus: <<Target +raw+ Title>>.\nMissing macro: <<Missing pass:[raw] Title>>.\nMissing plus: <<Missing +raw+ Title>>.\nControl: <<Control Title>>.\n\n== Pass pass:[raw] Title\n\n== Plus +raw+ Title\n\n== Target raw Title\n\n== Control Title\n";

    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let parsed = parse(input, &ParserOptions::default())?;
        let doc = parsed.document();
        let processor = Processor::new_with_variant(
            ConverterOptions::default(),
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            variant,
        )?;
        let mut output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.convert_to_writer(
            doc,
            &mut output,
            &RenderOptions::default(),
            &mut diagnostics,
        )?;
        let html = String::from_utf8(output)?;

        for expected in [
            "Title macro: <a href=\"#_pass_raw_title\">Pass raw Title</a>.",
            "Title plus: <a href=\"#_plus_raw_title\">Plus raw Title</a>.",
            "Target macro: <a href=\"#Target raw Title\">[Target raw Title]</a>.",
            "Target plus: <a href=\"#Target raw Title\">[Target raw Title]</a>.",
            "Missing macro: <a href=\"#Missing raw Title\">[Missing raw Title]</a>.",
            "Missing plus: <a href=\"#Missing raw Title\">[Missing raw Title]</a>.",
            "Control: <a href=\"#_control_title\">Control Title</a>.",
        ] {
            assert!(html.contains(expected), "expected {expected:?} in {html}");
        }
        assert!(!html.contains('\u{fffd}'), "{html}");
    }
    Ok(())
}

#[test]
fn captioned_cross_references_honor_source_order_xrefstyle() -> Result<(), Error> {
    let html = convert_string(
        ":figure-caption: BeforeFigure\n:table-caption: BeforeTable\n:xrefstyle: short\n\nForward short: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: full\n\nForward full: <<figure-target>> and <<table-target>>.\n\n:figure-caption: TargetFigure\n:table-caption: TargetTable\n\n[[figure-target]]\n.A figure title\nimage::figure.svg[]\n\n[[table-target]]\n.A table title\n|===\n|Cell\n|===\n\n:figure-caption: AfterFigure\n:table-caption: AfterTable\n:xrefstyle: short\n\nBackward short: <<figure-target>> and <<table-target>>.\n\n:xrefstyle: full\n\nBackward full: <<figure-target>> and <<table-target>>.\n",
        &[],
    )?;

    for expected in [
        "Forward short: <a href=\"#figure-target\">TargetFigure 1</a> and <a href=\"#table-target\">BeforeTable 1</a>",
        "Forward full: <a href=\"#figure-target\">TargetFigure 1, &#8220;A figure title&#8221;</a> and <a href=\"#table-target\">BeforeTable 1, &#8220;A table title&#8221;</a>",
        "Backward short: <a href=\"#figure-target\">TargetFigure 1</a> and <a href=\"#table-target\">AfterTable 1</a>",
        "Backward full: <a href=\"#figure-target\">TargetFigure 1, &#8220;A figure title&#8221;</a> and <a href=\"#table-target\">AfterTable 1, &#8220;A table title&#8221;</a>",
    ] {
        assert!(html.contains(expected), "expected {expected:?} in {html}");
    }
    Ok(())
}

#[test]
fn explicit_ordered_list_numbering_styles() -> Result<(), Error> {
    // An explicit `[<style>]` on an ordered list sets the CSS class (and `<ol type>`
    // where applicable), overriding the depth-derived default. Matches asciidoctor.
    let cases = [
        ("arabic", "<ol class=\"arabic\">"),
        ("decimal", "<ol class=\"decimal\">"),
        ("loweralpha", "<ol class=\"loweralpha\" type=\"a\">"),
        ("upperalpha", "<ol class=\"upperalpha\" type=\"A\">"),
        ("lowerroman", "<ol class=\"lowerroman\" type=\"i\">"),
        ("upperroman", "<ol class=\"upperroman\" type=\"I\">"),
        ("lowergreek", "<ol class=\"lowergreek\">"),
    ];
    for (style, expected_ol) in cases {
        let html = convert_string(&format!("[{style}]\n. one\n. two\n"), &[])?;
        assert!(
            html.contains(expected_ol),
            "style `{style}` should render `{expected_ol}`:\n{html}"
        );
        assert!(
            html.contains(&format!("<div class=\"olist {style}\">")),
            "style `{style}` should render `<div class=\"olist {style}\">`:\n{html}"
        );
    }
    Ok(())
}

#[test]
fn none_ordered_list_style_suppresses_marker() -> Result<(), Error> {
    let html = convert_string(". numbered\n\n[none]\n. unmarked\n", &[])?;

    assert!(html.contains("<div class=\"olist none\">"), "{html}");
    assert!(html.contains("<ol class=\"none\">"), "{html}");
    Ok(())
}

#[test]
fn markerless_ordered_list_styles_preserve_their_classes() -> Result<(), Error> {
    for style in ["no-bullet", "unstyled", "unnumbered"] {
        let html = convert_string(&format!("[{style}]\n. unmarked\n"), &[])?;

        assert!(
            html.contains(&format!("<div class=\"olist {style}\">")),
            "{html}"
        );
        assert!(html.contains(&format!("<ol class=\"{style}\">")), "{html}");
    }
    Ok(())
}

#[test]
fn unordered_list_styles_preserve_their_classes() -> Result<(), Error> {
    for style in [
        "none",
        "no-bullet",
        "unstyled",
        "unnumbered",
        "disc",
        "circle",
        "square",
    ] {
        let html = convert_string(&format!("[{style}]\n* item\n"), &[])?;

        assert!(
            html.contains(&format!("<div class=\"ulist {style}\">")),
            "{html}"
        );
        assert!(html.contains(&format!("<ul class=\"{style}\">")), "{html}");
    }
    Ok(())
}

#[test]
fn checklist_styles_stay_on_the_wrapper() -> Result<(), Error> {
    for style in ["none", "no-bullet", "unstyled"] {
        let html = convert_string(&format!("[{style}]\n* [ ] task\n"), &[])?;

        assert!(
            html.contains(&format!("<div class=\"ulist checklist {style}\">")),
            "{html}"
        );
        assert!(html.contains("<ul class=\"checklist\">"), "{html}");
    }
    Ok(())
}

#[test]
fn preface_subsections_are_not_numbered() -> Result<(), Error> {
    // Issue #408: with :sectnums:, subsections of a [preface] must stay
    // unnumbered (asciidoctor does not emit "0.1." before them), while a normal
    // chapter after the preface still numbers from 1.
    let input = "= Title\n:sectnums:\n\n[preface]\n== Introduction\n\nintro\n\n=== Features\n\nfeatures\n\n== Real Chapter\n\ntext\n";
    let html = convert_string(input, &[])?;

    assert!(
        html.contains("<h3 id=\"_features\">Features</h3>"),
        "preface subsection should be unnumbered:\n{html}"
    );
    assert!(
        html.contains("<h2 id=\"_introduction\">Introduction</h2>"),
        "preface itself should be unnumbered:\n{html}"
    );
    assert!(
        html.contains("<h2 id=\"_real_chapter\">1. Real Chapter</h2>"),
        "normal chapter after preface should still number from 1:\n{html}"
    );
    Ok(())
}

#[cfg(feature = "highlighting")]
mod syntax_highlighting {
    use super::*;

    const SOURCE_BLOCK: &str = r#":source-highlighter: syntect

[source,rust]
----
fn main() {
    println!("hello");
}
----
"#;

    #[rstest::rstest]
    #[case::primary("highlight-css", "highlight-style")]
    #[case::deprecated("syntect-css", "syntect-style")]
    #[case::mixed_css("highlight-css", "syntect-style")]
    #[case::mixed_style("syntect-css", "highlight-style")]
    fn highlight_aliases_preserve_rendering_and_written_stylesheets(
        #[case] css: &str,
        #[case] style: &str,
    ) -> Result<(), Error> {
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let attributes = [
                (css, AttributeValue::String("class".into())),
                (style, AttributeValue::String("Solarized (dark)".into())),
            ];
            let canonical = convert_string_with_variant(
                SOURCE_BLOCK,
                &[
                    ("highlight-css", AttributeValue::String("class".into())),
                    (
                        "highlight-style",
                        AttributeValue::String("Solarized (dark)".into()),
                    ),
                ],
                variant,
            )?;
            let output = convert_string_with_variant(SOURCE_BLOCK, &attributes, variant)?;
            assert_eq!(output, canonical, "{css}/{style}: {variant}");
            assert!(output.contains("class=\"syntax-"), "{output}");

            let temp = tempfile::tempdir()?;
            let path = temp.path().join("document.html");
            let parser_options = ParserOptions::builder()
                .with_attributes(attributes.into_iter())
                .with_attribute("linkcss", true)
                .with_attribute("stylesdir", "css")
                .build()?;
            let parsed = parse(SOURCE_BLOCK, &parser_options)?;
            let doc = parsed.document();
            let processor = Processor::new_with_variant(
                ConverterOptions::default(),
                ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
                variant,
            )?;
            let mut linked = Vec::new();
            let mut warnings = Vec::new();
            let source = WarningSource::new("html");
            let mut diagnostics = Diagnostics::new(&source, &mut warnings);
            processor.write_to(doc, &mut linked, None, Some(&path), &mut diagnostics)?;
            let linked = String::from_utf8(linked)?;
            assert!(
                linked.contains("href=\"css/acdc-highlight.css\""),
                "{linked}"
            );
            assert!(!linked.contains("acdc-syntect.css"), "{linked}");
            assert!(!temp.path().join("css/acdc-syntect.css").exists());
            let written_css = read_to_string(temp.path().join("css/acdc-highlight.css"))?;
            assert!(
                canonical.contains(&written_css),
                "written theme differs from embedded theme"
            );
            assert_eq!(
                warnings.len(),
                [css, style]
                    .iter()
                    .filter(|name| name.starts_with("syntect-"))
                    .count(),
                "{warnings:?}"
            );
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::primary_values(
        ":highlight-css: inline\n:syntect-css: class\n:highlight-style: InspiredGitHub\n:syntect-style: Solarized (dark)\n",
        "inline",
        "InspiredGitHub"
    )]
    #[case::primary_empty(
        ":highlight-css:\n:syntect-css: class\n:highlight-style:\n:syntect-style: Solarized (dark)\n",
        "inline",
        "InspiredGitHub"
    )]
    #[case::primary_unsets(
        ":highlight-css!:\n:syntect-css: class\n:highlight-style!:\n:syntect-style: Solarized (dark)\n",
        "inline",
        "InspiredGitHub"
    )]
    #[case::primary_prefix_unsets(
        ":!highlight-css:\n:syntect-css: class\n:!highlight-style:\n:syntect-style: Solarized (dark)\n",
        "inline",
        "InspiredGitHub"
    )]
    #[case::dark_unset_theme(
        ":highlight-css!:\n:syntect-css: class\n:highlight-style!:\n:syntect-style: Solarized (dark)\n:dark-mode:\n",
        "inline",
        "base16-eighties.dark"
    )]
    #[case::unknown_primary_mode(
        ":highlight-css: unknown\n:syntect-css: class\n:highlight-style: InspiredGitHub\n:syntect-style: Solarized (dark)\n",
        "inline",
        "InspiredGitHub"
    )]
    #[case::primary_class_over_alias_inline(
        ":highlight-css: class\n:syntect-css: inline\n:highlight-style: Solarized (dark)\n:syntect-style: InspiredGitHub\n",
        "class",
        "Solarized (dark)"
    )]
    fn primary_highlight_assignments_override_aliases(
        #[case] declarations: &str,
        #[case] mode: &str,
        #[case] theme: &str,
    ) -> Result<(), Error> {
        let input = format!("{declarations}{SOURCE_BLOCK}");
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let expected = convert_string_with_variant(
                &input,
                &[
                    ("highlight-css", AttributeValue::String(mode.into())),
                    ("highlight-style", AttributeValue::String(theme.into())),
                ],
                variant,
            )?;
            let actual = convert_string_with_variant(&input, &[], variant)?;
            assert_eq!(actual, expected, "{declarations}: {variant}");
            assert_eq!(actual.contains("class=\"syntax-"), mode == "class");
        }
        Ok(())
    }

    #[test]
    fn caller_highlight_unsets_suppress_legacy_stylesheet_writes() -> Result<(), Error> {
        let input =
            format!(":syntect-css: class\n:syntect-style: Solarized (dark)\n{SOURCE_BLOCK}");
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let temp = tempfile::tempdir()?;
            let path = temp.path().join("document.html");
            let options = ParserOptions::builder()
                .with_attribute("highlight-css", false)
                .with_attribute("highlight-style", false)
                .with_attribute("linkcss", true)
                .build()?;
            let parsed = parse(&input, &options)?;
            let doc = parsed.document();
            let processor = Processor::new_with_variant(
                ConverterOptions::default(),
                ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
                variant,
            )?;
            let mut output = Vec::new();
            let mut warnings = Vec::new();
            let source = WarningSource::new("html");
            let mut diagnostics = Diagnostics::new(&source, &mut warnings);
            processor.write_to(doc, &mut output, None, Some(&path), &mut diagnostics)?;
            let output = String::from_utf8(output)?;
            assert!(!output.contains("class=\"syntax-"), "{output}");
            assert!(!output.contains("acdc-highlight.css"), "{output}");
            assert!(!temp.path().join("acdc-highlight.css").exists());
            assert_eq!(warnings.len(), 2, "{warnings:?}");
        }
        Ok(())
    }

    #[test]
    fn class_mode_produces_class_spans() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[("highlight-css", AttributeValue::String("class".into()))],
        )?;
        assert!(
            html.contains("class=\"syntax-"),
            "Should contain class=\"syntax-\" spans:\n{html}"
        );
        assert!(
            !html.contains("style=\"color:"),
            "Should not contain inline style= color:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn class_mode_embeds_css_in_head() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[("highlight-css", AttributeValue::String("class".into()))],
        )?;
        assert!(
            html.contains(".syntax-"),
            "Head should contain .syntax- CSS rules:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn inline_mode_uses_style_attributes() -> Result<(), Error> {
        let html = convert_string(SOURCE_BLOCK, &[])?;
        assert!(
            html.contains("style=\""),
            "Inline mode should use style= attributes:\n{html}"
        );
        assert!(
            !html.contains("class=\"syntax-"),
            "Inline mode should not contain syntax- classes:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn highlight_style_overrides_theme() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[(
                "highlight-style",
                AttributeValue::String("base16-ocean.dark".into()),
            )],
        )?;
        assert!(
            html.contains("<span"),
            "Should produce highlighted spans with custom theme:\n{html}"
        );
        assert_ne!(html, convert_string(SOURCE_BLOCK, &[])?);
        Ok(())
    }

    #[test]
    fn class_mode_with_custom_theme_embeds_that_theme_css() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[
                ("highlight-css", AttributeValue::String("class".into())),
                (
                    "highlight-style",
                    AttributeValue::String("Solarized (dark)".into()),
                ),
            ],
        )?;
        // The CSS should be present and the code should have class spans
        assert!(html.contains(".syntax-"), "CSS rules should be in head");
        assert!(
            html.contains("class=\"syntax-"),
            "Code should have class= spans"
        );
        Ok(())
    }

    #[test]
    fn class_mode_with_linkcss_links_stylesheet() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[
                ("highlight-css", AttributeValue::String("class".into())),
                ("linkcss", AttributeValue::Bool(true)),
            ],
        )?;
        // Should link to the external stylesheet, not embed it
        assert!(
            html.contains(r#"<link rel="stylesheet" href="./acdc-highlight.css">"#),
            "Should link to acdc-highlight.css:\n{html}"
        );
        // Should NOT embed the CSS rules in the page
        assert!(
            !html.contains("<style>\n.syntax-"),
            "Should not embed syntax CSS when linkcss is set:\n{html}"
        );
        // Code should still have class-based spans
        assert!(
            html.contains("class=\"syntax-"),
            "Code should still have class= spans:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn class_mode_with_linkcss_and_stylesdir() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[
                ("highlight-css", AttributeValue::String("class".into())),
                ("linkcss", AttributeValue::Bool(true)),
                ("stylesdir", AttributeValue::String("css".into())),
            ],
        )?;
        assert!(
            html.contains(r#"<link rel="stylesheet" href="css/acdc-highlight.css">"#),
            "Should link to css/acdc-highlight.css:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn inline_mode_with_linkcss_no_syntax_link() -> Result<(), Error> {
        let html = convert_string(SOURCE_BLOCK, &[("linkcss", AttributeValue::Bool(true))])?;
        assert!(
            !html.contains("acdc-highlight.css"),
            "Inline mode should not reference acdc-highlight.css:\n{html}"
        );
        Ok(())
    }

    #[test]
    #[cfg(feature = "pre-spec-subs")]
    fn attribute_substitution_applied_with_highlighting() -> Result<(), Error> {
        let input = r#":source-highlighter: syntect
:version: 1.0

[source,ruby,subs="+attributes"]
----
puts "Version: {version}"
----
"#;
        let html = convert_string(input, &[])?;
        assert!(
            html.contains("Version: 1.0"),
            "Attribute references should be expanded in highlighted code:\n{html}"
        );
        assert!(
            !html.contains("{version}"),
            "Unexpanded attribute reference should not appear in output:\n{html}"
        );
        Ok(())
    }
}

mod stylesheet_modes {
    use super::*;

    const BASIC_DOC: &str = "= Title\n\nHello world.\n";

    #[test]
    fn no_stylesheet_mode_suppresses_css_and_fonts() -> Result<(), Error> {
        let html = convert_string(":!stylesheet:\n\nHello world.\n", &[])?;
        // No embedded <style> for the main stylesheet
        assert!(
            !html.contains("<style>"),
            "no-stylesheet mode should not contain <style>:\n{html}"
        );
        // No linked stylesheet
        assert!(
            !html.contains(r#"<link rel="stylesheet""#),
            "no-stylesheet mode should not contain stylesheet <link>:\n{html}"
        );
        // No Google Fonts link
        assert!(
            !html.contains("fonts.googleapis.com"),
            "no-stylesheet mode should not contain Google Fonts link:\n{html}"
        );
        // Body content should still be present
        assert!(
            html.contains("Hello world."),
            "content should still be rendered"
        );
        Ok(())
    }

    #[test]
    fn no_stylesheet_mode_preserves_mathjax() -> Result<(), Error> {
        let html = convert_string(":!stylesheet:\n:stem:\n\nHello world.\n", &[])?;
        assert!(
            html.contains("MathJax"),
            "no-stylesheet mode should still include MathJax when :stem: is set:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn no_stylesheet_mode_preserves_font_awesome() -> Result<(), Error> {
        let html = convert_string(":!stylesheet:\n:icons: font\n\nHello world.\n", &[])?;
        assert!(
            html.contains("fontawesome"),
            "no-stylesheet mode should still include Font Awesome when :icons: font is set:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn asciidoc_cell_attributes_do_not_add_head_resources() -> Result<(), Error> {
        let html = convert_string(
            "= T\n:csp:\n\n[cols=\"1*a\"]\n|===\n|\n:icons: font\n:stem:\n\nNOTE: Nested\n|===\n",
            &[],
        )?;

        assert!(
            !html.contains("fontawesome"),
            "unexpected Font Awesome:\n{html}"
        );
        assert!(!html.contains("MathJax"), "unexpected MathJax:\n{html}");
        assert!(
            !html.contains("cdn.jsdelivr.net"),
            "unexpected Font Awesome CSP source:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn default_mode_includes_embedded_css() -> Result<(), Error> {
        let html = convert_string(BASIC_DOC, &[])?;
        assert!(
            html.contains("<style>"),
            "default mode should embed CSS in <style>:\n{html}"
        );
        assert!(
            html.contains("fonts.googleapis.com"),
            "default mode should include Google Fonts link:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn linkcss_mode_links_stylesheet() -> Result<(), Error> {
        let html = convert_string(BASIC_DOC, &[("linkcss", AttributeValue::Bool(true))])?;
        assert!(
            html.contains(r#"<link rel="stylesheet" href="./"#),
            "linkcss mode should link to stylesheet:\n{html}"
        );
        // Should still have supplementary stem styles
        assert!(
            html.contains(".stemblock .content"),
            "linkcss mode should include supplementary stem styles:\n{html}"
        );
        Ok(())
    }
}

mod webfonts {
    use super::*;

    #[test]
    fn default_includes_google_fonts() -> Result<(), Error> {
        let html = convert_string("= Title\n\nHello.\n", &[])?;
        assert!(
            html.contains("fonts.googleapis.com/css?family=Open+Sans"),
            "default should include Open Sans font link:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn webfonts_disabled_suppresses_font_link() -> Result<(), Error> {
        let html = convert_string(":!webfonts:\n\nHello.\n", &[])?;
        // No Google Fonts <link> tag (the CSS content itself may mention fonts in comments)
        assert!(
            !html.contains(r#"<link rel="stylesheet" href="https://fonts.googleapis.com"#),
            ":!webfonts: should suppress Google Fonts <link> tag:\n{html}"
        );
        // Should still have stylesheet
        assert!(
            html.contains("<style>"),
            "disabling webfonts should not affect stylesheet:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn webfonts_custom_value_uses_custom_url() -> Result<(), Error> {
        let html = convert_string(":webfonts: Roboto:400,700\n\nHello.\n", &[])?;
        assert!(
            html.contains(r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Roboto:400,700">"#),
            "custom :webfonts: value should appear in font <link> tag:\n{html}"
        );
        // The default Open Sans font <link> should not be present
        assert!(
            !html.contains(
                r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Open+Sans"#
            ),
            "custom :webfonts: should replace default Open Sans <link> tag:\n{html}"
        );
        Ok(())
    }
}

mod copycss {
    use super::*;
    use acdc_converters_core::Converter;
    use std::fs::write;
    use tempfile::tempdir;

    #[test]
    fn linkcss_with_default_stylesheet_writes_builtin_css() -> Result<(), Error> {
        let tmp = tempdir()?;
        let html_path = tmp.path().join("output.html");

        let input = "= Title\n:linkcss:\n\nHello.\n";
        let mut attrs = HashMap::<Cow<'_, str>, AttributeValue<'_>>::new();
        attrs.insert("linkcss".into(), AttributeValue::Bool(true));
        attrs.insert("copycss".into(), AttributeValue::String(Cow::Borrowed("")));

        let parser_options = ParserOptions::with_attributes(attrs)?;
        let parsed = parse(input, &parser_options)?;
        let doc = parsed.document();

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            HtmlVariant::Standard,
        )?;

        // Run a full file conversion: this writes HTML and copies CSS as
        // companion artifacts in one step.
        let mut html_output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.write_to(
            doc,
            &mut html_output,
            None,
            Some(&html_path),
            &mut diagnostics,
        )?;
        write(&html_path, &html_output)?;

        // The built-in stylesheet should have been written to disk
        let css_path = tmp.path().join("asciidoctor-light-mode.css");
        assert!(
            css_path.exists(),
            "built-in stylesheet should be written to disk at {}",
            css_path.display()
        );

        let css_content = read_to_string(&css_path)?;
        assert!(
            !css_content.is_empty(),
            "written CSS file should not be empty"
        );

        Ok(())
    }

    #[test]
    fn copycss_value_used_as_source_path() -> Result<(), Error> {
        let tmp = tempdir()?;
        let html_path = tmp.path().join("output.html");

        // Create a custom CSS file to be used as copycss source
        let custom_css_path = tmp.path().join("my-custom.css");
        write(&custom_css_path, "body { color: red; }")?;

        let input = "= Title\n:linkcss:\n:stylesheet: target.css\n\nHello.\n";
        let mut attrs = HashMap::<Cow<'_, str>, AttributeValue<'_>>::new();
        attrs.insert("linkcss".into(), AttributeValue::Bool(true));
        attrs.insert(
            "copycss".into(),
            AttributeValue::String(custom_css_path.to_string_lossy()),
        );
        attrs.insert(
            "stylesheet".into(),
            AttributeValue::String("target.css".into()),
        );

        let parser_options = ParserOptions::with_attributes(attrs)?;
        let parsed = parse(input, &parser_options)?;
        let doc = parsed.document();

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            HtmlVariant::Standard,
        )?;

        // Run a full file conversion: this writes HTML and copies CSS as
        // companion artifacts in one step.
        let mut html_output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.write_to(
            doc,
            &mut html_output,
            None,
            Some(&html_path),
            &mut diagnostics,
        )?;
        write(&html_path, &html_output)?;

        // The custom CSS should have been copied to target.css
        let target_path = tmp.path().join("target.css");
        assert!(
            target_path.exists(),
            "copycss source should be copied to target path at {}",
            target_path.display()
        );

        let content = read_to_string(&target_path)?;
        assert_eq!(
            content, "body { color: red; }",
            "copied file should have the custom CSS content"
        );

        Ok(())
    }

    #[test]
    fn no_stylesheet_mode_skips_copycss() -> Result<(), Error> {
        let tmp = tempdir()?;
        let html_path = tmp.path().join("output.html");

        let input = ":!stylesheet:\n:linkcss:\n\nHello.\n";
        let mut attrs = HashMap::<Cow<'_, str>, AttributeValue<'_>>::new();
        attrs.insert("stylesheet".into(), AttributeValue::Bool(false));
        attrs.insert("linkcss".into(), AttributeValue::Bool(true));
        attrs.insert("copycss".into(), AttributeValue::String(Cow::Borrowed("")));

        let parser_options = ParserOptions::with_attributes(attrs)?;
        let parsed = parse(input, &parser_options)?;
        let doc = parsed.document();

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            HtmlVariant::Standard,
        )?;

        let mut html_output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.write_to(
            doc,
            &mut html_output,
            None,
            Some(&html_path),
            &mut diagnostics,
        )?;

        // No CSS files should be written
        let css_files: Vec<_> = std::fs::read_dir(tmp.path())?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "css"))
            .collect();
        assert!(
            css_files.is_empty(),
            "no CSS files should be written in no-stylesheet mode"
        );

        Ok(())
    }

    #[test]
    fn embedded_mode_skips_copycss() -> Result<(), Error> {
        let tmp = tempdir()?;
        let html_path = tmp.path().join("output.html");

        let input = "= Title\n:linkcss:\n\nHello.\n";
        let mut attrs = HashMap::<Cow<'_, str>, AttributeValue<'_>>::new();
        attrs.insert("linkcss".into(), AttributeValue::Bool(true));
        attrs.insert("copycss".into(), AttributeValue::String(Cow::Borrowed("")));

        let parser_options = ParserOptions::with_attributes(attrs)?;
        let parsed = parse(input, &parser_options)?;
        let doc = parsed.document();

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .embedded(true)
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            HtmlVariant::Standard,
        )?;

        let mut html_output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.write_to(
            doc,
            &mut html_output,
            None,
            Some(&html_path),
            &mut diagnostics,
        )?;

        // No CSS files should be written in embedded mode
        let css_files: Vec<_> = std::fs::read_dir(tmp.path())?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "css"))
            .collect();
        assert!(
            css_files.is_empty(),
            "no CSS files should be written in embedded mode, found: {:?}",
            css_files
                .iter()
                .map(std::fs::DirEntry::path)
                .collect::<Vec<_>>()
        );

        Ok(())
    }
}

mod docinfo {
    use super::*;
    use std::fs::write;
    use tempfile::tempdir;

    /// Helper: create a temp dir with an `.adoc` source file and optional docinfo files,
    /// parse it, and return the converted HTML string.
    fn convert_with_docinfo(
        adoc_content: &str,
        docinfo_files: &[(&str, &str)],
        embedded: bool,
        safe_mode: SafeMode,
    ) -> Result<String, Box<dyn StdError>> {
        let tmp = tempdir()?;
        let adoc_path = tmp.path().join("mydoc.adoc");
        write(&adoc_path, adoc_content)?;

        for (name, content) in docinfo_files {
            write(tmp.path().join(name), content)?;
        }

        let parser_options = ParserOptions::default();
        let parsed = parse_file(&adoc_path, &parser_options)?;
        let doc = parsed.document();

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .safe_mode(safe_mode)
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            HtmlVariant::Standard,
        )?;
        let render_options = RenderOptions {
            embedded,
            source_dir: Some(tmp.path().to_path_buf()),
            docname: Some("mydoc".to_string()),
            ..RenderOptions::default()
        };

        let html = processor.convert_to_string(doc, &render_options)?;
        Ok(html)
    }

    #[test]
    fn shared_head_docinfo_injected() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[("docinfo.html", "<style>.custom { color: red; }</style>")],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<style>.custom { color: red; }</style>"),
            "shared head docinfo content should be in output"
        );
        // Content should appear before </head>
        let before_head_close = html.split("</head>").next().unwrap_or("");
        assert!(
            before_head_close.contains("<style>.custom { color: red; }</style>"),
            "docinfo head content should appear before </head>"
        );
        Ok(())
    }

    #[test]
    fn private_head_docinfo_injected() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: private\n\nHello.\n",
            &[(
                "mydoc-docinfo.html",
                "<meta name=\"custom\" content=\"value\">",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<meta name=\"custom\" content=\"value\">"),
            "private head docinfo content should be in output"
        );
        Ok(())
    }

    #[test]
    fn shared_header_docinfo_injected() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[(
                "docinfo-header.html",
                "<div id=\"custom-banner\">Banner</div>",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<div id=\"custom-banner\">Banner</div>"),
            "shared header docinfo content should be in output"
        );
        // Content should appear after <body...>
        let after_body_open = html.split("<body").nth(1).unwrap_or("");
        assert!(
            after_body_open.contains("<div id=\"custom-banner\">Banner</div>"),
            "docinfo header content should appear after <body>"
        );
        Ok(())
    }

    #[test]
    fn private_header_docinfo_injected() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: private\n\nHello.\n",
            &[(
                "mydoc-docinfo-header.html",
                "<nav id=\"private-nav\">Nav</nav>",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<nav id=\"private-nav\">Nav</nav>"),
            "private header docinfo content should be in output"
        );
        Ok(())
    }

    #[test]
    fn shared_footer_docinfo_injected() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[(
                "docinfo-footer.html",
                "<script>console.log('analytics');</script>",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<script>console.log('analytics');</script>"),
            "shared footer docinfo content should be in output"
        );
        // Content should appear before </body>
        let before_body_close = html.split("</body>").next().unwrap_or("");
        assert!(
            before_body_close.contains("<script>console.log('analytics');</script>"),
            "docinfo footer content should appear before </body>"
        );
        Ok(())
    }

    #[test]
    fn private_footer_docinfo_injected() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: private\n\nHello.\n",
            &[(
                "mydoc-docinfo-footer.html",
                "<div id=\"private-footer\">PF</div>",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<div id=\"private-footer\">PF</div>"),
            "private footer docinfo content should be in output"
        );
        Ok(())
    }

    #[test]
    fn combined_shared_and_private() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared,private\n\nHello.\n",
            &[
                ("docinfo.html", "<!-- shared-head -->"),
                ("mydoc-docinfo.html", "<!-- private-head -->"),
                ("docinfo-header.html", "<!-- shared-header -->"),
                ("mydoc-docinfo-header.html", "<!-- private-header -->"),
                ("docinfo-footer.html", "<!-- shared-footer -->"),
                ("mydoc-docinfo-footer.html", "<!-- private-footer -->"),
            ],
            false,
            SafeMode::Unsafe,
        )?;

        // All six should be present
        assert!(html.contains("<!-- shared-head -->"), "shared head missing");
        assert!(
            html.contains("<!-- private-head -->"),
            "private head missing"
        );
        assert!(
            html.contains("<!-- shared-header -->"),
            "shared header missing"
        );
        assert!(
            html.contains("<!-- private-header -->"),
            "private header missing"
        );
        assert!(
            html.contains("<!-- shared-footer -->"),
            "shared footer missing"
        );
        assert!(
            html.contains("<!-- private-footer -->"),
            "private footer missing"
        );

        // Private should appear before shared (within each position)
        let before_shared = html.split("<!-- shared-head -->").next().unwrap_or("");
        assert!(
            before_shared.contains("<!-- private-head -->"),
            "private head should come before shared head"
        );

        Ok(())
    }

    #[test]
    fn embedded_mode_skips_docinfo() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[
                ("docinfo.html", "<!-- should-not-appear -->"),
                ("docinfo-footer.html", "<!-- also-hidden -->"),
            ],
            true,
            SafeMode::Unsafe,
        )?;

        assert!(
            !html.contains("<!-- should-not-appear -->"),
            "docinfo head should not appear in embedded mode"
        );
        assert!(
            !html.contains("<!-- also-hidden -->"),
            "docinfo footer should not appear in embedded mode"
        );
        Ok(())
    }

    #[test]
    fn missing_file_no_error() -> Result<(), Box<dyn StdError>> {
        // No docinfo files present, should not error
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[],
            false,
            SafeMode::Unsafe,
        )?;

        // Should still produce valid HTML
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
        Ok(())
    }

    #[test]
    fn docinfodir_overrides_source_dir() -> Result<(), Box<dyn StdError>> {
        let tmp = tempdir()?;

        // Create source file in root
        let adoc_path = tmp.path().join("mydoc.adoc");
        write(
            &adoc_path,
            "= Title\n:docinfo: shared\n:docinfodir: custom-docinfo\n\nHello.\n",
        )?;

        // Create docinfo in a subdirectory
        let docinfo_dir = tmp.path().join("custom-docinfo");
        std::fs::create_dir(&docinfo_dir)?;
        write(docinfo_dir.join("docinfo.html"), "<!-- from-custom-dir -->")?;

        // Also create one in source dir (should NOT be picked up)
        write(tmp.path().join("docinfo.html"), "<!-- from-source-dir -->")?;

        let parser_options = ParserOptions::default();
        let parsed = parse_file(&adoc_path, &parser_options)?;
        let doc = parsed.document();

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            HtmlVariant::Standard,
        )?;
        let render_options = RenderOptions {
            source_dir: Some(tmp.path().to_path_buf()),
            docname: Some("mydoc".to_string()),
            ..RenderOptions::default()
        };

        let html = processor.convert_to_string(doc, &render_options)?;

        assert!(
            html.contains("<!-- from-custom-dir -->"),
            "should use docinfo from docinfodir"
        );
        assert!(
            !html.contains("<!-- from-source-dir -->"),
            "should not use docinfo from source dir when docinfodir is set"
        );
        Ok(())
    }

    #[test]
    fn secure_safe_mode_disables_docinfo() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[("docinfo.html", "<!-- secret-content -->")],
            false,
            SafeMode::Secure,
        )?;

        assert!(
            !html.contains("<!-- secret-content -->"),
            "docinfo should not appear in secure safe mode"
        );
        Ok(())
    }

    #[test]
    fn attribute_substitution_in_docinfo() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n:my-custom-attr: replaced-value\n\nHello.\n",
            &[(
                "docinfo.html",
                "<meta name=\"custom\" content=\"{my-custom-attr}\">",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<meta name=\"custom\" content=\"replaced-value\">"),
            "attribute references in docinfo should be substituted"
        );
        assert!(
            !html.contains("{my-custom-attr}"),
            "raw attribute reference should not remain"
        );
        Ok(())
    }

    #[test]
    fn docinfo_bare_attribute_defaults_to_private() -> Result<(), Box<dyn StdError>> {
        // `:docinfo:` with no value should default to "private"
        let html = convert_with_docinfo(
            "= Title\n:docinfo:\n\nHello.\n",
            &[("mydoc-docinfo.html", "<!-- private-default -->")],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<!-- private-default -->"),
            "bare :docinfo: should default to private scope"
        );
        Ok(())
    }

    #[test]
    fn granular_shared_head_only() -> Result<(), Box<dyn StdError>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared-head\n\nHello.\n",
            &[
                ("docinfo.html", "<!-- head-content -->"),
                ("docinfo-footer.html", "<!-- footer-should-not-appear -->"),
            ],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<!-- head-content -->"),
            "shared-head docinfo should appear"
        );
        assert!(
            !html.contains("<!-- footer-should-not-appear -->"),
            "footer docinfo should not appear when only shared-head is set"
        );
        Ok(())
    }
}

mod toc_footnote {
    use super::*;

    /// Helper: convert an `AsciiDoc` string to embedded HTML (mirrors WASM editor path).
    fn convert_embedded(input: &str) -> Result<String, Error> {
        let parser_options = ParserOptions::default();
        let parsed = parse(input, &parser_options)?;
        let doc = parsed.document();
        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            ParserOptions::builder().with_attributes(doc.attributes.clone().into_inputs()),
            HtmlVariant::Standard,
        )?;
        let render_options = RenderOptions {
            embedded: true,
            ..RenderOptions::default()
        };
        let mut output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.convert_to_writer(doc, &mut output, &render_options, &mut diagnostics)?;
        Ok(String::from_utf8(output)?)
    }

    #[test]
    fn toc_footnote_id_not_duplicated() -> Result<(), Error> {
        // Use a named footnote so the id="_footnote_{name}" path is exercised
        let input = "= Document\n:toc:\n\n== Section with footnotefootnote:fn1[A note]\n\nBody.\n";
        let html = convert_embedded(input)?;

        // TOC should render in embedded mode
        assert!(
            html.contains("id=\"toc\""),
            "TOC should render in embedded mode:\n{html}"
        );

        // The footnote id should appear exactly once (on the heading, not in the TOC)
        let count = html.matches("id=\"_footnote_fn1\"").count();
        assert_eq!(
            count, 1,
            "id=\"_footnote_fn1\" should appear exactly once, found {count}:\n{html}"
        );

        // TOC entry should not contain nested <a> tags inside the TOC link
        let toc_section = html
            .split("id=\"toc\"")
            .nth(1)
            .and_then(|s| s.split("</div>\n</div>").next())
            .unwrap_or("");
        assert!(
            !toc_section.contains("<a class=\"footnote-ref\""),
            "TOC should not contain footnote anchor links:\n{toc_section}"
        );

        // Footnotes section should still render correctly
        assert!(
            html.contains("id=\"footnotes\""),
            "Footnotes section should render:\n{html}"
        );

        Ok(())
    }
}
#[test]
fn table_paragraphs_keep_boundaries_in_both_variants() -> Result<(), Error> {
    let source = "= Table\n\n[cols=\"1,1a\"]\n|===\n|Before.\n////\nInside.\n////\nAfter.\n\nSecond.\n|First nested.\n\nSecond nested.\n|===\n";
    let parsed = acdc_parser::parse(source, &acdc_parser::Options::default())?;
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = render_fixture_document(parsed.document(), variant, true)?;
        assert!(!output.contains("After.Second."), "{output}");
        assert!(!output.contains("First nested.Second nested."), "{output}");
        assert!(output.contains("After.</p>"), "{output}");
        assert!(output.contains("Second.</p>"), "{output}");
        assert!(output.contains("First nested.</p>"), "{output}");
        assert!(output.contains("Second nested.</p>"), "{output}");
    }
    let parsed = acdc_parser::parse(
        "[cols=\"1a\"]\n|===\n|\n:value: cell\n\nSingle.\n|===\n",
        &acdc_parser::Options::default(),
    )?;
    let output = render_fixture_document(parsed.document(), HtmlVariant::Semantic, true)?;
    assert!(output.contains("<p>Single.</p>"), "{output}");
    let parsed = acdc_parser::parse(
        "[cols=\"1\",options=header]\n|===\n|First.\n\nSecond.\n|Body.\n|===\n",
        &acdc_parser::Options::default(),
    )?;
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let output = render_fixture_document(parsed.document(), variant, true)?;
        assert!(output.contains(">First.\nSecond.</th>"), "{output}");
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn asciidoc_table_paragraphs_keep_unique_link_targets() -> Result<(), Error> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/source/html/embedded/subs_table_asciidoc_paragraphs.adoc");
    let parsed = parse_file(&path, &ParserOptions::default())?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        for embedded in [true, false] {
            let output = render_fixture_document(parsed.document(), variant, embedded)?;
            check_link_structure(&output, "asciidoc table paragraphs")?;
        }
    }
    Ok(())
}

#[test]
fn title_footnotes_keep_unique_link_targets() -> Result<(), Error> {
    for name in ["title_macro_registration", "title_macro_contexts"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/source/html/embedded/{name}.adoc"));
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            for embedded in [true, false] {
                let output = render_fixture(&path, variant, embedded)?;
                check_link_structure(&output, name)?;
            }
        }
    }
    Ok(())
}

#[test]
fn heading_and_title_footnotes_keep_unique_link_targets() -> Result<(), Error> {
    let source = "= T\n:toc:\n\n.Unused footnote:[Unused note.]\n== Heading footnote:[Heading note.]\n\n.Title footnote:named[Title note.]\n[#target]\nParagraph.\n\n[discrete]\n=== Discrete footnote:[Discrete note.]\n\nBody footnote:[Body note.], footnote:named[], and <<target>>.\n";
    let parsed = parse(source, &ParserOptions::default())?;
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        for embedded in [true, false] {
            let output = render_fixture_document(parsed.document(), variant, embedded)?;
            check_link_structure(&output, "heading_and_title_macro_registration")?;
        }
    }
    Ok(())
}
