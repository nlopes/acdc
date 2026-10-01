use acdc_parser::{Options, ParseResult, parse, parse_file};
use lopdf::{Document as PdfDocument, Object, ObjectId, decode_text_string};
use std::{
    collections::HashMap,
    fs::read_to_string,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Converter, Diagnostics, Options as ConverterOptions, WarningSource};
use acdc_converters_pdf::{PdfOptions, Processor};

type Error = Box<dyn std::error::Error>;

#[test]
fn index_inline_spacing_preserves_catalog_destinations() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/index_inline_spacing.adoc"))?;
    let pages = internal_link_pages(&pdf, 3)?;
    assert_eq!(pages.len(), 20);
    assert_eq!(pages.iter().filter(|page| **page == 1).count(), 18);
    assert_eq!(pages.iter().filter(|page| **page == 2).count(), 2);
    Ok(())
}

#[test]
fn index_inline_spacing_markers_keep_glyph_positions() -> Result<(), Error> {
    // Digits avoid kerning across the marker; bold End forms a separate text run.
    for (marked, plain) in [
        ("1((234))**End**.", "1234**End**."),
        ("1(((Hidden)))**End**.", "1**End**."),
        ("((12))((34))**End**.", "1234**End**."),
        ("(indexterm2:[1234])**End**.", "(1234)**End**."),
        ("1 ((234)) **End**.", "1 234 **End**."),
    ] {
        let marked_pdf = render_input(&format!("= Spacing\n\n{marked}\n"))?;
        let plain_pdf = render_input(&format!("= Spacing\n\n{plain}\n"))?;
        let marked_position = text_origin(&marked_pdf, 1, "End")?;
        let plain_position = text_origin(&plain_pdf, 1, "End")?;
        assert!(
            (marked_position.0 - plain_position.0).abs() < 0.01
                && (marked_position.1 - plain_position.1).abs() < 0.01,
            "{marked}: {marked_position:?} != {plain_position:?}"
        );
    }
    Ok(())
}

#[test]
fn outer_macro_escapes_preserve_only_active_pdf_uri_annotations() -> Result<(), Error> {
    let code = include_str!("fixtures/source/subs_outer_macro_escapes.adoc");
    let highlighted = code.replace(":manmanual:", ":source-highlighter: syntect\n:manmanual:");
    for (source, count) in [
        (include_str!("fixtures/source/outer_macro_escapes.adoc"), 4),
        (code, 1),
        (highlighted.as_str(), 1),
    ] {
        let pdf = render_input(source)?;
        let mut targets = Vec::new();
        for page in pdf.get_pages().values() {
            for annotation in pdf.get_page_annotations(*page)? {
                if let Ok(action) = annotation.get(b"A") {
                    let (_, action) = pdf.dereference(action)?;
                    if let Ok(uri) = action.as_dict()?.get(b"URI") {
                        targets.push(String::from_utf8(uri.as_str()?.to_vec())?);
                    }
                }
            }
        }
        assert_eq!(targets, vec!["https://example.org"; count]);
    }
    Ok(())
}

#[test]
fn mailto_query_values_reach_pdf_annotations() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/mailto_query.adoc"))?;
    let mut targets = Vec::new();
    for page in pdf.get_pages().values() {
        for annotation in pdf.get_page_annotations(*page)? {
            targets.push(
                annotation
                    .get(b"A")?
                    .as_dict()?
                    .get(b"URI")?
                    .as_str()?
                    .to_vec(),
            );
        }
    }
    for expected in [
        "mailto:both@example.org?subject=Test%20subject&body=Message%20body",
        "mailto:fallback@example.org?subject=Test%20subject&body=Message%20body",
        "mailto:unicode@example.org?subject=Caf%C3%A9%20%26%20tea%3F%20%2B%2050%25%20%231%20~&body=x%3Dy%20%2F%20caf%C3%A9",
        "mailto:query@example.org?cc=copy@example.org&subject=New%20subject&body=New%20body",
        "mailto:unquoted@example.org?subject=&body=Body",
        "mailto:passarg@example.org?subject=one%2Ctwo&body=%2B",
    ] {
        assert!(
            targets.iter().any(|target| target == expected.as_bytes()),
            "missing URI: {expected}"
        );
    }
    Ok(())
}

#[test]
fn nested_links_have_separate_pdf_destinations() -> Result<(), Error> {
    let pdf = render_input(
        "= Links\n\nlink:https://outer.example[Before mailto:inner@example.org[Inner] after]\n",
    )?;
    let page = *pdf.get_pages().get(&1).ok_or("missing page")?;
    let mut links = Vec::new();
    for annotation in pdf.get_page_annotations(page)? {
        let uri = annotation
            .get(b"A")?
            .as_dict()?
            .get(b"URI")?
            .as_str()?
            .to_vec();
        let [left, _, right, _] = annotation.get(b"Rect")?.as_array()?.as_slice() else {
            return Err("invalid annotation rectangle".into());
        };
        links.push((left.as_float()?, right.as_float()?, uri));
    }
    links.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(links.len(), 3);
    assert_eq!(
        links
            .iter()
            .map(|link| link.2.as_slice())
            .collect::<Vec<_>>(),
        [
            b"https://outer.example".as_slice(),
            b"mailto:inner@example.org".as_slice(),
            b"https://outer.example".as_slice()
        ]
    );
    for pair in links.windows(2) {
        let [left, right] = pair else {
            return Err("missing adjacent links".into());
        };
        assert!(
            left.1 <= right.0 + 0.01,
            "overlapping link rectangles: {links:?}"
        );
    }
    Ok(())
}

#[test]
fn link_label_quotes_keep_pdf_uri_annotations() -> Result<(), Error> {
    let source = r#"= Quoted labels

link:https://example.org["One \"quote\" two",role=test]

https://example.net["One \"quote\" two",role=test]

mailto:test@example.org["One \"quote\" two",role=test]
"#;
    let pdf = render_input(source)?;
    let page = *pdf.get_pages().get(&1).ok_or("missing page")?;
    let mut targets = Vec::new();
    for annotation in pdf.get_page_annotations(page)? {
        targets.push(
            annotation
                .get(b"A")?
                .as_dict()?
                .get(b"URI")?
                .as_str()?
                .to_vec(),
        );
    }
    targets.sort();
    assert_eq!(
        targets,
        [
            b"https://example.net".to_vec(),
            b"https://example.org".to_vec(),
            b"mailto:test@example.org".to_vec()
        ]
    );
    let text = pdf
        .extract_text(&[1])?
        .split_whitespace()
        .collect::<String>();
    assert_eq!(text.matches("One\"quote\"two").count(), 3, "{text}");
    Ok(())
}

#[test]
fn link_label_brackets_keep_pdf_uri_annotations() -> Result<(), Error> {
    let source = "= Link labels\n\nlink:https://example.org[One \\] two]\n\nhttps://example.net[One \\] two]\n\nmailto:test@example.org[One \\] two]\n";
    let pdf = render_input(source)?;
    let pages = pdf.get_pages();
    let page = pages.get(&1).ok_or("missing page")?;
    let mut targets = Vec::new();
    for annotation in pdf.get_page_annotations(*page)? {
        targets.push(
            annotation
                .get(b"A")?
                .as_dict()?
                .get(b"URI")?
                .as_str()?
                .to_vec(),
        );
    }
    targets.sort();
    assert_eq!(
        targets,
        [
            b"https://example.net".to_vec(),
            b"https://example.org".to_vec(),
            b"mailto:test@example.org".to_vec()
        ]
    );
    let text = pdf
        .extract_text(&[1])?
        .split_whitespace()
        .collect::<String>();
    assert_eq!(text.matches("One]two").count(), 3, "{text}");
    Ok(())
}

#[test]
fn passthrough_brackets_keep_pdf_link_destinations() -> Result<(), Error> {
    let source = "= Passthrough destinations\n\nSee <<target,Target>>.\n\n<<<\n\npass:m[anchor:target[Target\\]Destination.]\n\npass:m[link:https://example.org[External\\]]\n";
    let pdf = render_input(source)?;
    assert_eq!(internal_link_pages(&pdf, 1)?, [2]);
    let pages = pdf.get_pages();
    let page = pages.get(&2).ok_or("missing target page")?;
    let annotations = pdf.get_page_annotations(*page)?;
    let [link] = annotations.as_slice() else {
        return Err("expected one external link".into());
    };
    assert_eq!(
        link.get(b"A")?.as_dict()?.get(b"URI")?.as_str()?,
        b"https://example.org"
    );
    Ok(())
}

#[test]
fn anchor_macros_keep_their_pdf_destinations() -> Result<(), Error> {
    let source = "= Anchors\n\nSee <<target>>, <<empty>>.\n\n<<<\n\nanchor:target[Target]Destination.\n\n* anchor:empty[]Item.\n";
    let pdf = render_input(source)?;
    assert_eq!(internal_link_pages(&pdf, 1)?, [2, 2]);
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn anchor_macros_in_code_keep_pdf_destinations() -> Result<(), Error> {
    for highlighting in ["", ":source-highlighter: rouge\n"] {
        let source = format!(
            "= Code anchors\n{highlighting}\nSee <<start>>, <<middle>>, <<end>>, <<only>>, <<short>>, <<last>>.\n\n<<<\n\n[source,rust,linenums,subs=+macros]\n----\nanchor:start[Start]a=anchor:middle[Middle]1;anchor:end[End]\nanchor:only[Only]\n[[short,Short]]b=2;\nanchor:last[Last]\n----\n"
        );
        let pdf = render_input(&source)?;
        assert_eq!(internal_link_pages(&pdf, 1)?, [2, 2, 2, 2, 2, 2]);
        // PDF text objects can split at a zero-width target or syntax style.
        let text = pdf
            .extract_text(&[2])?
            .split_whitespace()
            .collect::<String>();
        assert!(text.contains("a=1;"), "{text}");
        assert!(text.contains("b=2;"), "{text}");
        assert!(!text.contains("anchor:"), "{text}");
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn anchor_macros_in_code_target_the_occurrence_page() -> Result<(), Error> {
    for target_line in [1, 50, 70] {
        let mut source = String::from(
            "= Code anchor pages\n:source-highlighter: rouge\n\nSee <<target>>.\n\n[source,text,linenums,subs=+macros]\n----\n",
        );
        for line in 1..=70 {
            source.push_str(if line == target_line {
                "anchor:target[Target]TargetLine\n"
            } else {
                "PaddingLine\n"
            });
        }
        source.push_str("----\n");
        let pdf = render_input(&source)?;
        let mut occurrence = None;
        for page in pdf.get_pages().keys() {
            if pdf.extract_text(&[*page])?.contains("TargetLine") {
                occurrence = Some(*page);
            }
        }
        assert_eq!(
            internal_link_pages(&pdf, 1)?,
            [occurrence.ok_or("missing code target")?]
        );
    }
    Ok(())
}

#[test]
fn standalone_callouts_keep_their_pdf_destinations() -> Result<(), Error> {
    let input = "= Standalone callout destinations\n\nSee <<notes,Notes>> and <<last,Last>>.\n\n<<<\n\n[[notes]]\n<1> First.\n<2> [[last]]Second.\n";
    let pdf = render_input(input)?;
    assert_eq!(internal_link_pages(&pdf, 1)?, [2, 2]);
    let text = pdf.extract_text(&[2])?;
    assert_eq!(text.matches("Second.").count(), 1);
    assert!(
        !text.contains("<2>"),
        "the marker must render as a list label"
    );
    Ok(())
}

#[test]
fn callout_nested_lists_keep_their_pdf_destinations() -> Result<(), Error> {
    let input = "= Callout destinations\n\nSee <<child,Child>> and <<last,Last>>.\n\n<<<\n\n----\ncode <1> <2>\n----\n<1> First.\n[[child]]\n* Nested.\n<2> [[last]]Second.\n";
    let pdf = render_input(input)?;
    assert_eq!(internal_link_pages(&pdf, 1)?, [2, 2]);
    let text = pdf.extract_text(&[2])?;
    assert_eq!(text.matches("Second.").count(), 1);
    assert!(
        !text.contains("<2>"),
        "the second callout must be a list item"
    );
    Ok(())
}

#[test]
fn xref_nested_footnotes_keep_their_own_page_destination() -> Result<(), Error> {
    for label in [
        "xref:target[Before footnote:[Nested body.] after]",
        "<<target,Before footnote:[Nested body.] after>>",
    ] {
        let input = format!("= Notes\n\n[[target]]\n== Target\n\n<<<\n\n{label}\n");
        let pdf = render_input(&input)?;
        let mut pages = internal_link_pages(&pdf, 2)?;
        pages.sort_unstable();
        assert_eq!(
            pages,
            [1, 1, 2, 2],
            "surrounding text links to page 1; the note and backlink to page 2"
        );
        assert_eq!(pdf.extract_text(&[2])?.matches("Nested body.").count(), 1);
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_xref_nested_footnotes_keep_their_own_page_destination() -> Result<(), Error> {
    for highlighter in ["", ":source-highlighter: rouge\n"] {
        let input = format!(
            "= Notes\n{highlighter}\n[[target]]\n== Target\n\n<<<\n\n[source,rust,subs=+macros]\n----\nxref:target[Before footnote:[Nested body.] after]\n----\n"
        );
        let pdf = render_input(&input)?;
        let mut pages = internal_link_pages(&pdf, 2)?;
        pages.sort_unstable();
        assert_eq!(
            pages,
            [1, 1, 2, 2],
            "surrounding code links to page 1; the note and backlink to page 2"
        );
        assert_eq!(pdf.extract_text(&[2])?.matches("Nested body.").count(), 1);
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn index_catalog_labels_do_not_create_extra_pdf_footnotes() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/subs_index_stage_labels.adoc"))?;
    let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
    let text = pdf
        .extract_text(&pages)?
        .split_whitespace()
        .collect::<String>();
    assert_eq!(text.matches("1One").count(), 1, "{text}");
    assert_eq!(text.matches("2Two").count(), 1, "{text}");
    assert_eq!(
        text.matches("3Note{name}and*literal*(C)").count(),
        1,
        "{text}"
    );
    assert_eq!(text.matches("4boldEarly{name}").count(), 1, "{text}");
    assert!(text.contains("Notefootnote:[One]"), "{text}");
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_footnotes_keep_clickable_markers_and_reuse_definitions() -> Result<(), Error> {
    for highlighter in ["", ":source-highlighter: rouge\n"] {
        for options in ["", ",linenums", ",%autofit"] {
            let input = format!(
                "= Notes\n{highlighter}\n[source,rust{options},subs=+macros]\n----\nfootnote:first[First body.]footnote:second[Second body.]\nfootnote:first[Ignored body.]\n----\n\n<<<\n\nReuse footnote:second[].\n"
            );
            let pdf = render_input(&input)?;
            let text = pdf
                .extract_text(&[1, 2])?
                .split_whitespace()
                .collect::<String>();
            assert_eq!(text.matches("Firstbody.").count(), 1, "{text}");
            assert_eq!(text.matches("Secondbody.").count(), 1, "{text}");
            assert!(!text.contains("Ignored"), "{text}");
            assert_eq!(internal_link_pages(&pdf, 1)?, [1, 1, 1, 1, 1]);
            assert_eq!(internal_link_pages(&pdf, 2)?, [1]);
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_footnotes_inside_link_labels_keep_their_own_destination() -> Result<(), Error> {
    for highlighter in ["", ":source-highlighter: rouge\n"] {
        let input = format!(
            "= Notes\n{highlighter}\n[source,rust,subs=+macros]\n----\nlink:https://example.org[Before footnote:[Nested body.] after]\n----\n"
        );
        let pdf = render_input(&input)?;
        let page = *pdf.get_pages().get(&1).ok_or("missing page")?;
        let mut internal = 0;
        let mut external = 0;
        for annotation in pdf.get_page_annotations(page)? {
            if annotation.has(b"Dest") {
                internal += 1;
            } else {
                let action = annotation.get(b"A")?.as_dict()?;
                if action.get(b"S")?.as_name()? == b"GoTo" {
                    internal += 1;
                } else {
                    assert_eq!(action.get(b"URI")?.as_str()?, b"https://example.org");
                    external += 1;
                }
            }
        }
        assert_eq!(
            internal, 2,
            "marker and backlink must each have a destination"
        );
        assert_eq!(
            external, 2,
            "link text before and after the marker stays clickable"
        );
        assert_eq!(pdf.extract_text(&[1])?.matches("Nested body.").count(), 1);
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn wrapped_verbatim_footnotes_render_once() -> Result<(), Error> {
    for prefix in 60..=100 {
        let input = format!(
            "= Notes\n:source-highlighter: rouge\n\n[source,rust,linenums,subs=+macros]\n----\n{}footnote:[Wrapped body.]{}\n----\n",
            "x".repeat(prefix),
            "y".repeat(100)
        );
        let pdf = render_input(&input)?;
        assert_eq!(pdf.extract_text(&[1])?.matches("Wrapped body.").count(), 1);
        assert_eq!(internal_link_pages(&pdf, 1)?, [1, 1]);
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn named_footnotes_do_not_insert_spaces_in_code() -> Result<(), Error> {
    for highlighter in ["", ":source-highlighter: rouge\n"] {
        let input = format!(
            "= Spacing\n{highlighter}\n[source,rust,subs=+macros]\n----\nafootnote:[Anonymous.]b\nafootnote:named[Named.]b\n----\n"
        );
        let pdf = render_input(&input)?;
        let text = pdf.extract_text(&[1])?.replace('\n', "");
        assert!(text.contains("a1b"), "{text:?}");
        assert!(text.contains("a2b"), "{text:?}");
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_links_preserve_clickable_labels_and_spacing() -> Result<(), Error> {
    for highlighter in ["", ":source-highlighter: rouge\n"] {
        let input = format!(
            "= Links\n{highlighter}\n[source,rust,subs=+macros]\n----\nprefix = link:https://example.org/?a=1&b=2[LinkedLabel];\n----\n"
        );
        let pdf = render_input(&input)?;
        let page = *pdf.get_pages().get(&1).ok_or("missing page")?;
        let annotations = pdf.get_page_annotations(page)?;
        assert_eq!(annotations.len(), 1);
        let annotation = annotations.first().ok_or("missing link")?;
        let action = annotation.get(b"A")?.as_dict()?;
        assert_eq!(
            action.get(b"URI")?.as_str()?,
            b"https://example.org/?a=1&b=2"
        );
        let rect = annotation.get(b"Rect")?.as_array()?;
        let [left, _, right, _] = rect.as_slice() else {
            return Err("invalid rectangle".into());
        };
        let (label_x, _) = text_origin(&pdf, 1, "LinkedLabel")?;
        let (suffix_x, _) = text_origin(&pdf, 1, ";")?;
        assert!((left.as_float()? - label_x).abs() < 0.1);
        assert!((right.as_float()? - suffix_x).abs() < 0.1);
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_cross_references_reach_the_target_page() -> Result<(), Error> {
    for highlighter in ["", ":source-highlighter: rouge\n"] {
        for options in ["", ",linenums", ",%autofit"] {
            let input = format!(
                "= Links\n{highlighter}\n[source,rust{options},subs=+macros]\n----\nlet a = <<destination>>;\nlet b = xref:destination[Named];\n----\n\n<<<\n\n[[destination]]\n== Destination\n"
            );
            let pdf = render_input(&input)?;
            assert_eq!(internal_link_pages(&pdf, 1)?, [2, 2]);
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_link_ids_and_wrapped_links_compile() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/subs_verbatim_links.adoc"))?;
    let text = pdf
        .extract_text(&pdf.get_pages().keys().copied().collect::<Vec<_>>())?
        .split_whitespace()
        .collect::<String>();
    assert!(text.contains("->"), "{text}");
    assert!(text.contains("<-"), "{text}");
    assert!(text.contains('→'), "{text}");

    let pdf = render_input(include_str!(
        "fixtures/source/subs_verbatim_link_labels.adoc"
    ))?;
    assert!(
        !pdf.get_page_annotations(*pdf.get_pages().get(&1).ok_or("missing page")?)?
            .is_empty()
    );
    let pdf = render_input(include_str!(
        "fixtures/source/subs_verbatim_links_highlighting.adoc"
    ))?;
    let page = *pdf.get_pages().get(&1).ok_or("missing page")?;
    let mut tops = Vec::new();
    for annotation in pdf.get_page_annotations(page)? {
        if let Ok(action) = annotation.get(b"A")
            && let Ok(uri) = action.as_dict()?.get(b"URI")
            && uri.as_str()? == b"https://example.org/wrapped"
        {
            let rect = annotation.get(b"Rect")?.as_array()?;
            let [_, _, _, top] = rect.as_slice() else {
                return Err("invalid rectangle".into());
            };
            tops.push(top.as_float()?);
        }
    }
    tops.sort_by(f32::total_cmp);
    tops.dedup();
    assert!(
        tops.len() >= 2,
        "wrapped link must remain clickable on each line: {tops:?}"
    );
    Ok(())
}

fn render_input(input: &str) -> Result<PdfDocument, Error> {
    let parsed = parse(input, &Options::default())?;
    render_parsed(&parsed)
}

fn render_parsed(parsed: &ParseResult) -> Result<PdfDocument, Error> {
    let processor = Processor::new(
        ConverterOptions::default(),
        Options::builder().with_attributes(parsed.document().attributes.clone().into_inputs()),
    )?;
    let source = WarningSource::new("pdf");
    let mut warnings = Vec::new();
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    let mut output = Vec::new();
    processor.write_to(parsed.document(), &mut output, None, None, &mut diagnostics)?;
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(PdfDocument::load_mem(&output)?)
}

#[test]
fn multiple_index_catalogs_compile_with_relationship_links() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/index_multiple_catalogs.adoc"))?;
    let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
    let text = pdf.extract_text(&pages)?;
    assert!(text.contains("First Index"), "{text}");
    assert!(text.contains("Second Index"), "{text}");
    assert!(text.contains("see"), "{text}");
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_index_locators_follow_the_rendered_code_pages() -> Result<(), Error> {
    use std::fmt::Write as _;
    for (style, options) in [("source", ""), ("source%linenums", ",highlight=2")] {
        for wrapped in [false, true] {
            let mut input = format!(
                "= Code index\n:source-highlighter: rouge\n\n[{style},text,subs=+macros{options}]\n----\n((FirstMarker))\n"
            );
            if wrapped {
                input.push_str(&"padding ".repeat(2400));
            } else {
                for line in 1..180 {
                    writeln!(&mut input, "source line {line}")?;
                }
            }
            input.push_str("((LastMarker))\n(((TailHidden)))\n----\n\n[index]\n== Index\n");
            let pdf = render_input(&input)?;
            let mut text = String::new();
            let mut first_page = None;
            let mut last_page = None;
            for page in pdf.get_pages().keys() {
                let page_text = pdf.extract_text(&[*page])?;
                if page_text.contains("FirstMarker") && first_page.is_none() {
                    first_page = Some(*page);
                }
                if page_text.contains("LastMarker") && last_page.is_none() {
                    last_page = Some(*page);
                }
                text.push_str(&page_text);
            }
            let first_page = first_page.ok_or("missing first marker")?;
            let last_page = last_page.ok_or("missing last marker")?;
            assert!(last_page > first_page, "fixture must span pages");
            let (_, catalog) = text.rsplit_once("Index").ok_or("missing index")?;
            let catalog = catalog
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .replace(" ,", ",");
            assert!(
                catalog.contains(&format!("FirstMarker, {first_page}")),
                "{catalog}"
            );
            assert!(
                catalog.contains(&format!("LastMarker, {last_page}")),
                "{catalog}"
            );
            assert!(
                catalog.contains(&format!("TailHidden, {last_page}")),
                "{catalog}"
            );
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_index_anchors_preserve_numbered_code_alignment() -> Result<(), Error> {
    for autofit in ["", ",options=autofit"] {
        for index in ["", "\n[index]\n== Index\n"] {
            let input = format!(
                "= Code layout\n:source-highlighter: rouge\n\n[source%linenums,text,subs=+macros,highlight=2{autofit}]\n----\nBeforeLine\n((MarkedLine))(((Hidden)))\nAfterLine\n----\n{index}"
            );
            let pdf = render_input(&input)?;
            let before = text_origin(&pdf, 1, "BeforeLine")?;
            let marked = text_origin(&pdf, 1, "MarkedLine")?;
            let after = text_origin(&pdf, 1, "AfterLine")?;
            let number = text_origin(&pdf, 1, "2")?;
            assert!((before.0 - marked.0).abs() < 0.01, "{before:?} {marked:?}");
            assert!((after.0 - marked.0).abs() < 0.01, "{after:?} {marked:?}");
            assert!((number.1 - marked.1).abs() < 0.01, "{number:?} {marked:?}");
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_index_locators_match_occurrences_at_page_boundaries() -> Result<(), Error> {
    use std::fmt::Write as _;
    for marker_line in 49..59 {
        let mut input = String::from(
            "= Page boundary\n:source-highlighter: rouge\n\n[source%linenums,text,subs=+macros]\n----\n",
        );
        for line in 1..70 {
            if line == marker_line {
                input.push_str("((BoundaryMarker))\n");
            } else {
                writeln!(input, "padding line {line}")?;
            }
        }
        input.push_str("----\n\n[index]\n== Index\n");
        let pdf = render_input(&input)?;
        let mut occurrence_page = None;
        let mut catalog_page = None;
        for page in pdf.get_pages().keys() {
            let text = pdf.extract_text(&[*page])?;
            if text.contains("BoundaryMarker") && occurrence_page.is_none() {
                occurrence_page = Some(*page);
            }
            if text.contains("Index") {
                catalog_page = Some(*page);
            }
        }
        let occurrence_page = occurrence_page.ok_or("missing code term")?;
        let catalog_page = catalog_page.ok_or("missing index")?;
        assert_eq!(
            internal_link_pages(&pdf, catalog_page)?,
            [occurrence_page],
            "source line {marker_line}"
        );
    }
    Ok(())
}

fn text_origin(pdf: &PdfDocument, page: u32, needle: &str) -> Result<(f32, f32), Error> {
    let page_id = *pdf.get_pages().get(&page).ok_or("missing page")?;
    let encodings = pdf
        .get_page_fonts(page_id)?
        .into_iter()
        .map(|(name, font)| Ok((name, font.get_font_encoding(pdf)?)))
        .collect::<Result<HashMap<_, _>, Error>>()?;
    let content = lopdf::content::Content::decode(&pdf.get_page_content(page_id))?;
    let mut encoding = None;
    let mut origin = (0.0, 0.0);
    let mut transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut transforms = Vec::new();
    for operation in content.operations {
        match (operation.operator.as_str(), operation.operands.as_slice()) {
            ("q", []) => transforms.push(transform),
            ("Q", []) => transform = transforms.pop().ok_or("missing saved transform")?,
            ("cm", [xx, yx, xy, yy, tx, ty]) => {
                let [xx, yx, xy, yy, tx, ty] = [xx, yx, xy, yy, tx, ty].map(Object::as_float);
                let [xx, yx, xy, yy, tx, ty] = [xx?, yx?, xy?, yy?, tx?, ty?];
                transform = [
                    transform[0] * xx + transform[2] * yx,
                    transform[1] * xx + transform[3] * yx,
                    transform[0] * xy + transform[2] * yy,
                    transform[1] * xy + transform[3] * yy,
                    transform[0] * tx + transform[2] * ty + transform[4],
                    transform[1] * tx + transform[3] * ty + transform[5],
                ];
            }
            ("Tf", [font, _]) => encoding = encodings.get(font.as_name()?),
            ("Tm", [_, _, _, _, x, y]) => {
                let (x, y) = (x.as_float()?, y.as_float()?);
                let [xx, yx, xy, yy, tx, ty] = transform;
                origin = (xx * x + xy * y + tx, yx * x + yy * y + ty);
            }
            ("Tj", [Object::String(text, _)]) => {
                if PdfDocument::decode_text(encoding.ok_or("missing font")?, text)? == needle {
                    return Ok(origin);
                }
            }
            ("TJ", [Object::Array(parts)]) => {
                let mut text = String::new();
                for part in parts {
                    if let Object::String(bytes, _) = part {
                        text.push_str(&PdfDocument::decode_text(
                            encoding.ok_or("missing font")?,
                            bytes,
                        )?);
                    }
                }
                if text == needle {
                    return Ok(origin);
                }
            }
            _ => {}
        }
    }
    Err(format!("missing text {needle:?}").into())
}

#[test]
fn colon_cross_references_use_local_pdf_destinations() -> Result<(), Error> {
    let pdf = render_input(
        "= Colon links\n\n<<:colon,Short>> xref::colon[Macro] xref:#a-b.c:d[Fragment] <<a-b.c:d,Mixed>>\n\n<<<\n\nanchor::colon[Colon]Target.\n\n[[a-b.c:d,Mixed]]\nOther target.\n\nxref:a-b.c:d[External]\n",
    )?;
    assert_eq!(internal_link_pages(&pdf, 1)?, [2, 2, 2, 2]);
    let page = *pdf.get_pages().get(&2).ok_or("missing second page")?;
    let mut uris = Vec::new();
    for annotation in pdf.get_page_annotations(page)? {
        if let Ok(action) = annotation.get(b"A") {
            let (_, action) = pdf.dereference(action)?;
            if let Ok(uri) = action.as_dict()?.get(b"URI") {
                uris.push(String::from_utf8(uri.as_str()?.to_vec())?);
            }
        }
    }
    assert_eq!(uris, ["a-b.c:d"]);
    Ok(())
}

#[test]
fn included_source_references_create_internal_pdf_destinations() -> Result<(), Error> {
    let parsed = parse_file(
        "tests/fixtures/source/xref_included_sources.adoc",
        &Options::default(),
    )?;
    let pdf = render_parsed(&parsed)?;
    let mut internal = 0;
    let mut external = Vec::new();
    for page_id in pdf.get_pages().values() {
        for annotation in pdf.get_page_annotations(*page_id)? {
            if annotation.has(b"Dest") {
                internal += 1;
            } else if let Ok(action) = annotation.get(b"A") {
                let (_, action) = pdf.dereference(action)?;
                let action = action.as_dict()?;
                if let Ok(uri) = action.get(b"URI") {
                    external.push(String::from_utf8(uri.as_str()?.to_vec())?);
                } else if action.get(b"S")?.as_name()? == b"GoTo" {
                    internal += 1;
                }
            }
        }
    }
    // The explicit label spans regular and bold text, producing two annotations.
    assert_eq!(internal, 5);
    assert_eq!(
        external,
        [
            "other.pdf#included-target",
            "https://example.org/other.pdf#included-target"
        ]
    );
    let text = pdf
        .extract_text(&[1])?
        .split_whitespace()
        .collect::<String>();
    assert!(text.contains("Forward:Section1andSection1."), "{text}");
    assert!(text.contains("[missing.id]"), "{text}");
    Ok(())
}

#[test]
fn included_document_top_links_target_the_first_pdf_page() -> Result<(), Error> {
    for fixture in ["xref_document_top", "xref_document_top_untitled"] {
        let parsed = parse_file(
            format!("tests/fixtures/source/{fixture}.adoc"),
            &Options::default(),
        )?;
        let pdf = render_parsed(&parsed)?;
        assert_eq!(internal_link_pages(&pdf, 1)?, [1, 1, 1]);
        let text = pdf
            .extract_text(&[1])?
            .split_whitespace()
            .collect::<String>();
        assert!(
            text.contains("Top:[^top]and[^top].Explicit:Start."),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn horizontal_description_term_anchors_target_visible_content() -> Result<(), Error> {
    let pdf = render_input(
        "= Horizontal anchor\n\nSee <<rule>>.\n\n<<<\n\n[horizontal]\n[[rule,Rule 1]]Rule 1:: Description.\n",
    )?;
    assert_eq!(pdf.get_pages().len(), 2);
    assert_eq!(internal_link_pages(&pdf, 1)?, [2]);
    Ok(())
}

#[test]
fn standalone_anchors_target_the_following_paragraph() -> Result<(), Error> {
    for before in [
        "Before.",
        "Term:: Before.",
        "Term:: Principal.\n+\nBefore.",
        "* Principal.\n+\nBefore.",
    ] {
        let input = format!(
            "= Anchor ownership\n\nSee <<target>>.\n\n<<<\n\n{before}\n[[target,Target]]\nFollowing.\n"
        );
        let pdf = render_input(&input)?;
        assert_eq!(internal_link_pages(&pdf, 1)?, [2]);
        let page = *pdf.get_pages().get(&1).ok_or("missing link page")?;
        let annotations = pdf.get_page_annotations(page)?;
        let [link] = annotations.as_slice() else {
            return Err("expected one paragraph link".into());
        };
        let target = link
            .get(b"Dest")
            .or_else(|_| link.get(b"A")?.as_dict()?.get(b"D"))?;
        let (_, target) = pdf.dereference(target)?;
        let [_, kind, _, top, ..] = target.as_array()?.as_slice() else {
            return Err("incomplete paragraph destination".into());
        };
        assert_eq!(kind.as_name()?, b"XYZ");
        let top = top.as_float()?;
        let (_, baseline) = text_origin(&pdf, 2, "Following.")?;
        assert!(
            (0.0..20.0).contains(&(top - baseline)),
            "{before}: destination {top}, paragraph {baseline}"
        );
    }
    Ok(())
}

fn destination_page_id(pdf: &PdfDocument, destination: &Object) -> Result<ObjectId, Error> {
    let (_, destination) = pdf.dereference(destination)?;
    let destination = destination
        .as_dict()
        .and_then(|dict| dict.get(b"D"))
        .unwrap_or(destination);
    let (_, destination) = pdf.dereference(destination)?;
    Ok(destination
        .as_array()?
        .first()
        .ok_or("empty destination")?
        .as_reference()?)
}

fn internal_link_pages(pdf: &PdfDocument, page: u32) -> Result<Vec<u32>, Error> {
    let pages = pdf.get_pages();
    let page_id = pages.get(&page).ok_or("missing source page")?;
    let mut targets = HashMap::new();
    if let Ok(names) = pdf.catalog()?.get(b"Names") {
        let (_, names) = pdf.dereference(names)?;
        if let Ok(destinations) = names.as_dict()?.get(b"Dests") {
            let (_, destinations) = pdf.dereference(destinations)?;
            let entries = destinations.as_dict()?.get(b"Names")?.as_array()?;
            for [name, target] in entries.as_chunks::<2>().0 {
                targets.insert(name.as_str()?, destination_page_id(pdf, target)?);
            }
        }
    }
    pdf.get_page_annotations(*page_id)?
        .into_iter()
        .map(|annotation| {
            let destination = annotation
                .get(b"Dest")
                .or_else(|_| annotation.get(b"A")?.as_dict()?.get(b"D"))?;
            let (_, destination) = pdf.dereference(destination)?;
            let target = if let Ok(name) = destination.as_str() {
                *targets.get(name).ok_or("missing named destination")?
            } else {
                destination_page_id(pdf, destination)?
            };
            pages
                .iter()
                .find_map(|(number, id)| (*id == target).then_some(*number))
                .ok_or_else(|| "missing destination page".into())
        })
        .collect()
}

#[test]
fn duplicate_section_ids_keep_distinct_toc_links_and_first_reference() -> Result<(), Error> {
    let source = read_to_string("tests/fixtures/source/duplicate_section_ids.adoc")?;
    let pdf = render_input(&source)?;
    assert_eq!(pdf.get_pages().len(), 3);
    assert_eq!(internal_link_pages(&pdf, 1)?, [2, 3]);
    assert_eq!(internal_link_pages(&pdf, 3)?, [2]);
    let text = pdf.extract_text(&[3])?;
    assert!(
        text.split_whitespace()
            .collect::<String>()
            .contains("SeeFirst."),
        "{text}"
    );
    Ok(())
}

#[test]
fn duplicate_section_id_keeps_an_earlier_block_destination() -> Result<(), Error> {
    let source = read_to_string("tests/fixtures/source/duplicate_anchor_ids.adoc")?;
    let pdf = render_input(&source)?;
    assert_eq!(internal_link_pages(&pdf, 1)?.first(), Some(&3));
    assert_eq!(internal_link_pages(&pdf, 3)?.first(), Some(&2));
    Ok(())
}

#[test]
fn anchors_in_copied_titles_still_target_the_original_definition() -> Result<(), Error> {
    let source = read_to_string("tests/fixtures/source/copied_title_anchors.adoc")?;
    let pdf = render_input(&source)?;
    assert_eq!(pdf.get_pages().len(), 2);
    assert_eq!(internal_link_pages(&pdf, 1)?, [2]);
    assert!(internal_link_pages(&pdf, 2)?.iter().all(|page| *page == 2));
    Ok(())
}

#[test]
fn anchors_in_repeated_table_headers_resolve_to_the_first_page() -> Result<(), Error> {
    let source = format!(
        "= Repeated header\n:pdf-page-size: A5\n\n[options=header]\n|===\n|[[header]]Repeated heading\n{}|===\n\nSee <<header>>.\n",
        "|Body row\n".repeat(100)
    );
    let pdf = render_input(&source)?;
    let pages = pdf.get_pages();
    assert!(pages.len() > 1);
    let last = *pages.last_key_value().ok_or("missing last page")?.0;
    assert_eq!(internal_link_pages(&pdf, last)?, [1]);
    Ok(())
}

fn fixture_theme(doc: &acdc_parser::Document<'_>) -> Option<PathBuf> {
    doc.attributes
        .get("acdc-pdf-test-theme")
        .and_then(|value| value.text())
        .map(|name| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/themes")
                .join(name)
                .with_extension("yaml")
        })
}

fn assert_repeated_table_header_index(pdf: &[u8]) -> Result<(), Error> {
    let rendered = PdfDocument::load_mem(pdf)?;
    let pages = rendered.get_pages().keys().copied().collect::<Vec<_>>();
    let mut repeated_header_pages = Vec::new();
    for page in &pages {
        let text = rendered.extract_text(&[*page])?;
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.contains("Description with visible header") {
            repeated_header_pages.push(*page);
        }
    }
    assert!(
        repeated_header_pages.len() >= 2,
        "expected a repeated table header, found it on pages {repeated_header_pages:?}",
    );
    let last_header_page = repeated_header_pages
        .last()
        .copied()
        .ok_or("repeated table header page not found")?;
    let text = rendered.extract_text(&pages)?;
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" , ", ", ");
    for term in ["Related header", "shared term", "visible header"] {
        let expected = format!("{term}, {last_header_page}");
        assert!(
            normalized.contains(&expected),
            "expected index entry `{expected}` in PDF text:\n{text}",
        );
    }
    Ok(())
}

fn run_typst_fixture(path: &Path) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("invalid fixture file name")?;

    #[cfg(not(feature = "pre-spec-subs"))]
    if file_name.contains("subs") {
        return Ok(());
    }

    let expected_path = Path::new("tests/fixtures/expected")
        .join(file_name)
        .with_extension("typ");
    let bootstrap = Processor::new(ConverterOptions::default(), Options::builder())?;
    let parsed = parse_file(path, bootstrap.parser_options())?;
    let output_dir = tempfile::tempdir()?;
    let typst_path = output_dir.path().join("actual.typ");
    let processor = Processor::new(
        ConverterOptions::default(),
        Options::builder().with_attributes(parsed.document().attributes.clone().into_inputs()),
    )?
    .with_pdf_options(PdfOptions {
        emit_typst: Some(typst_path.clone()),
        theme: fixture_theme(parsed.document()),
        ..PdfOptions::default()
    });
    let mut pdf = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("pdf");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        parsed.document(),
        &mut pdf,
        Some(path),
        None,
        &mut diagnostics,
    )?;

    if file_name == "silent_metadata" {
        assert_eq!(
            warnings
                .iter()
                .map(|warning| warning.message.as_ref())
                .collect::<Vec<_>>(),
            [
                "inline image `fit=none` page-height sizing is not supported by the PDF backend; rendering with normal intrinsic sizing",
                "PHP source block mixed-mode highlighting is not supported by the PDF backend; rendering with Typst's normal PHP highlighter",
                "page-break layout changes are not supported by the PDF backend; keeping the document page layout",
            ]
        );
        assert!(warnings.iter().all(|warning| warning.advice.is_some()));
        assert_eq!(
            warnings
                .iter()
                .filter_map(|warning| warning.source_location())
                .map(|location| location.location.start.line)
                .collect::<Vec<_>>(),
            [97, 101, 118]
        );
    }

    if file_name == "parity_kitchen_sink" {
        assert!(warnings.is_empty(), "{warnings:?}");
        let rendered = PdfDocument::load_mem(&pdf)?;
        let (_, info) = rendered.dereference(rendered.trailer.get(b"Info")?)?;
        let info = info.as_dict()?;
        for (key, expected) in [
            (b"Title".as_slice(), "PDF Parity Kitchen Sink: API Coverage"),
            (b"Author".as_slice(), "Ada Lovelace, Grace B. Hopper"),
            (
                b"Subject".as_slice(),
                "Representative PDF converter coverage",
            ),
            (b"Keywords".as_slice(), "parity, PDF, converter API"),
        ] {
            assert_eq!(decode_text_string(info.get(key)?)?, expected);
        }
    }

    if file_name.starts_with("index_terms_repeated_table_header") {
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_repeated_table_header_index(&pdf)?;
    }

    assert!(pdf.starts_with(b"%PDF-"));
    let minimum_pages_path = expected_path.with_extension("min-pages");
    if minimum_pages_path.exists() {
        let minimum_pages = read_to_string(&minimum_pages_path)?
            .trim()
            .parse::<usize>()?;
        let rendered = PdfDocument::load_mem(&pdf)?;
        let actual_pages = rendered.get_pages().len();
        assert!(
            actual_pages >= minimum_pages,
            "PDF page count for {file_name} is {actual_pages}; expected at least {minimum_pages}",
        );
    }
    let expected = read_to_string(expected_path)?;
    let actual = read_to_string(typst_path)?;
    pretty_assertions::assert_eq!(
        expected,
        actual,
        "Typst output mismatch for fixture: {file_name}",
    );
    Ok(())
}

#[rstest::rstest]
fn typst_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    run_typst_fixture(&path)
}

#[test]
fn link_macro_ids_are_named_pdf_destinations() -> Result<(), Error> {
    let path = Path::new("tests/fixtures/source/link_macro_ids.adoc");
    let bootstrap = Processor::new(ConverterOptions::default(), Options::builder())?;
    let parsed = parse_file(path, bootstrap.parser_options())?;
    let processor = Processor::new(
        ConverterOptions::default(),
        Options::builder().with_attributes(parsed.document().attributes.clone().into_inputs()),
    )?;
    let mut pdf = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("pdf");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        parsed.document(),
        &mut pdf,
        Some(path),
        None,
        &mut diagnostics,
    )?;

    let rendered = PdfDocument::load_mem(&pdf)?;
    let (_, names) = rendered.dereference(rendered.catalog()?.get(b"Names")?)?;
    let (_, destinations) = rendered.dereference(names.as_dict()?.get(b"Dests")?)?;
    let (_, entries) = rendered.dereference(destinations.as_dict()?.get(b"Names")?)?;
    let mut names = entries
        .as_array()?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[name, _]| decode_text_string(name))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();

    assert_eq!(
        names,
        [
            "bare-link-id",
            "bare-mailto-id",
            "duplicate-id",
            "link-id",
            "mailto-id",
            "url-id",
        ]
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(())
}

#[test]
fn image_alt_text_reaches_pdf_structure() -> Result<(), Error> {
    let path = Path::new("tests/fixtures/source/image_accessibility_alt_text.adoc");
    let bootstrap = Processor::new(ConverterOptions::default(), Options::builder())?;
    let parsed = parse_file(path, bootstrap.parser_options())?;
    let processor = Processor::new(
        ConverterOptions::default(),
        Options::builder().with_attributes(parsed.document().attributes.clone().into_inputs()),
    )?;
    let mut pdf = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("pdf");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        parsed.document(),
        &mut pdf,
        Some(path),
        None,
        &mut diagnostics,
    )?;

    let rendered = PdfDocument::load_mem(&pdf)?;
    let mut descriptions = rendered
        .objects
        .values()
        .filter_map(|object| {
            let dictionary = object.as_dict().ok()?;
            let alt = dictionary.get(b"Alt").ok()?;
            decode_text_string(alt).ok()
        })
        .collect::<Vec<_>>();
    descriptions.sort();

    assert_eq!(
        descriptions,
        [
            "Explicit block description",
            "Explicit inline description",
            "Linked description",
            "Positioned description",
            "inline image dimensions",
            "inline image dimensions",
        ]
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(())
}
