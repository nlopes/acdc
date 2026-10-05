use acdc_parser::{Document, Options, ParseResult, parse, parse_file};
use lopdf::{Document as PdfDocument, Object, ObjectId, decode_text_string};
use std::{
    collections::HashMap,
    fs::read_to_string,
    path::{Path, PathBuf},
};

use acdc_converters_core::{
    Converter, Diagnostics, Options as ConverterOptions, Warning, WarningSource,
};
use acdc_converters_pdf::{PdfOptions, Processor};

type Error = Box<dyn std::error::Error>;

#[test]
fn nested_styles_use_combined_pdf_fonts_and_restore_outer_styles() -> Result<(), Error> {
    let source = include_str!("fixtures/source/nested_font_styles.adoc");
    let parsed = parse(source, &Options::default())?;
    let theme_dir = tempfile::tempdir()?;
    let theme_path = theme_dir.path().join("sans.yaml");
    std::fs::write(
        &theme_path,
        include_str!("../crates/theme/assets/theme/default.yaml")
            .replace("IBM Plex Serif", "IBM Plex Sans"),
    )?;
    for (theme, family, regular) in [
        (None, "IBMPlexSerif", "IBMPlexSerif-Regular"),
        (Some(theme_path), "IBMPlexSans", "IBMPlexSans"),
    ] {
        let processor = Processor::new(ConverterOptions::default(), Options::builder())?
            .with_pdf_options(PdfOptions {
                theme,
                ..PdfOptions::default()
            });
        let mut output = Vec::new();
        let mut warnings = Vec::new();
        let warning_source = WarningSource::new("pdf");
        processor.write_to(
            parsed.document(),
            &mut output,
            None,
            None,
            &mut Diagnostics::new(&warning_source, &mut warnings),
        )?;
        assert!(warnings.is_empty(), "{warnings:?}");
        let pdf = PdfDocument::load_mem(&output)?;
        let runs = pdf_text_fonts(&pdf)?;
        for word in [
            "BothOne",
            "BothTwo",
            "BothWide",
            "LinkBoth",
            "HeadingBoth",
            "CellBoth",
            "CellReverse",
            "NoteBoth",
        ] {
            assert_text_font(&runs, word, &format!("{family}-BoldItalic"));
        }
        for word in [
            "BoldRight",
            "BoldTail",
            "BoldWideTail",
            "BoldLinkTail",
            "CellTail",
            "NoteTail",
        ] {
            assert_text_font(&runs, word, &format!("{family}-Bold"));
        }
        for word in ["ItalicRight", "ItalicTail", "CellEnd"] {
            assert_text_font(&runs, word, &format!("{family}-Italic"));
        }
        assert_text_font(&runs, "PlainEnd", regular);
        for word in ["MonoLeft", "MonoRight", "MonoLinkTail", "MonoTargetTail"] {
            assert_text_font(&runs, word, "IBMPlexMono");
        }
        for word in [
            "MonoBold",
            "MonoInBold",
            "BoldTarget",
            "TargetTail",
            "HeadingCode",
        ] {
            assert_text_font(&runs, word, "IBMPlexMono-Bold");
        }
        for word in ["MonoItalic", "MonoInItalic"] {
            assert_text_font(&runs, word, "IBMPlexMono-Italic");
        }
        for word in ["MonoBoth", "CodeLinkBoth", "BothTarget"] {
            assert_text_font(&runs, word, "IBMPlexMono-BoldItalic");
        }
        let destinations = named_destinations(&pdf)?;
        for target in ["styled", "empty"] {
            assert!(destinations.contains_key(target), "{target}");
        }
        let mut links = Vec::new();
        for page in pdf.get_pages().keys() {
            links.extend(external_link_rects(&pdf, *page)?);
        }
        for target in ["https://example.org", "https://example.org/code"] {
            assert!(links.iter().any(|(uri, _)| uri == target), "{target}");
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn nested_styles_preserve_enabled_and_disabled_code_quotes() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/subs_nested_font_styles.adoc"))?;
    let runs = pdf_text_fonts(&pdf)?;
    for word in ["AttributeBoth", "CodeBoth", "CodeReverse"] {
        assert_text_font(&runs, word, "IBMPlexMono-BoldItalic");
    }
    assert_text_font(&runs, "CodeTail", "IBMPlexMono-Bold");
    assert_text_font(&runs, "ItalicTail", "IBMPlexMono-Italic");
    assert_text_font(&runs, "CodeEnd", "IBMPlexMono");
    assert_text_font(&runs, "Literal *Bold _Italic_*.", "IBMPlexMono");
    assert_text_font(&runs, "Mono *Bold _Italic_*", "IBMPlexSerif-Regular");
    Ok(())
}

fn assert_text_font(runs: &[(String, String)], needle: &str, font: &str) {
    let matches = runs
        .iter()
        .filter(|(text, _)| {
            text.match_indices(needle).any(|(start, _)| {
                text[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|ch| !ch.is_alphanumeric())
                    && text[start + needle.len()..]
                        .chars()
                        .next()
                        .is_none_or(|ch| !ch.is_alphanumeric())
            })
        })
        .collect::<Vec<_>>();
    assert!(!matches.is_empty(), "missing {needle:?}: {runs:?}");
    for (text, actual) in matches {
        assert!(
            actual.ends_with(font),
            "{text:?}: expected {font}, got {actual}"
        );
    }
}

fn pdf_text_fonts(pdf: &PdfDocument) -> Result<Vec<(String, String)>, Error> {
    let mut runs = Vec::new();
    for page in pdf.get_pages().values() {
        let fonts = pdf
            .get_page_fonts(*page)?
            .into_iter()
            .map(|(key, font)| {
                Ok((
                    key,
                    (
                        String::from_utf8(font.get(b"BaseFont")?.as_name()?.to_vec())?,
                        font.get_font_encoding(pdf)?,
                    ),
                ))
            })
            .collect::<Result<HashMap<_, _>, Error>>()?;
        let content = lopdf::content::Content::decode(&pdf.get_page_content(*page))?;
        let mut font = None;
        let mut saved_fonts = Vec::new();
        for operation in content.operations {
            match (operation.operator.as_str(), operation.operands.as_slice()) {
                ("q", []) => saved_fonts.push(font),
                ("Q", []) => font = saved_fonts.pop().ok_or("missing saved font")?,
                ("Tf", [name, _]) => font = fonts.get(name.as_name()?),
                ("Tj", [Object::String(bytes, _)]) => {
                    let (name, encoding) = font.ok_or("missing font")?;
                    runs.push((PdfDocument::decode_text(encoding, bytes)?, name.clone()));
                }
                ("TJ", [Object::Array(parts)]) => {
                    let (name, encoding) = font.ok_or("missing font")?;
                    let mut text = String::new();
                    for part in parts {
                        if let Object::String(bytes, _) = part {
                            text.push_str(&PdfDocument::decode_text(encoding, bytes)?);
                        }
                    }
                    runs.push((text, name.clone()));
                }
                _ => {}
            }
        }
    }
    Ok(runs)
}

#[test]
fn spacing_accents_keep_pdf_positions_and_font_diagnostics() -> Result<(), Error> {
    let source = include_str!("fixtures/source/pdf_glyph_coverage.adoc");
    let parsed = parse(source, &Options::default())?;
    let (pdf, warnings) = render_document_with_warnings(parsed.document())?;
    assert_missing_glyph_codes(&warnings, &["U+4E2D", "U+65E5", "U+D55C"]);
    let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
    let text = pdf.extract_text(&pages)?;
    for expected in ["tick", "literal", "title", "cell", "`", "´"] {
        assert!(text.contains(expected), "missing {expected:?}: {text}");
    }
    let before = text_origin(&pdf, 1, "G01 A")?;
    let grave = text_origin(&pdf, 1, "`")?;
    let after = text_origin(&pdf, 1, "tick")?;
    assert!((before.1 - grave.1).abs() < 0.01, "{before:?} {grave:?}");
    assert!((after.1 - grave.1).abs() < 0.01, "{after:?} {grave:?}");
    assert!(
        before.0 < grave.0 && grave.0 < after.0,
        "{before:?} {grave:?} {after:?}"
    );
    for arrow in "⇒⇐⇔→←↔".chars() {
        assert!(text.contains(arrow), "missing {arrow}: {text}");
    }
    assert!(
        external_link_rects(&pdf, 1)?
            .iter()
            .any(|(uri, _)| uri == "https://example.org")
    );
    Ok(())
}

#[test]
fn incomplete_links_omit_pdf_annotations_for_unfinished_targets() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/incomplete_links.adoc"))?;
    let mut targets = Vec::new();
    for page in pdf.get_pages().keys() {
        targets.extend(
            external_link_rects(&pdf, *page)?
                .into_iter()
                .map(|(uri, _)| uri),
        );
    }
    for target in [
        "https://complete.example",
        "https://inner.example",
        "https://other.example",
    ] {
        assert!(targets.iter().any(|uri| uri == target), "{targets:?}");
    }
    assert!(
        targets
            .iter()
            .all(|uri| !uri.contains("unfinished.example")),
        "{targets:?}"
    );
    assert!(
        !targets.iter().any(|uri| uri == "mailto:user@example.org"),
        "{targets:?}"
    );
    Ok(())
}

#[test]
fn named_index_delimiters_keep_pdf_annotations_and_one_note() -> Result<(), Error> {
    let source = "= Index labels\n\nindexterm2:[Before https://example.org[Link] footnote:one[Only note.] anchor:inner[] after].\n\nReuse footnote:one[] and <<inner,Inner>>.\n\n<<<\n\n[index]\n== Index\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    let pdf = render_parsed(&parsed)?;
    assert_eq!(pdf.extract_text(&[1])?.matches("Only note.").count(), 1);
    let links = external_link_rects(&pdf, 1)?;
    assert!(links.iter().any(|(uri, _)| uri == "https://example.org"));
    assert!(named_destinations(&pdf)?.contains_key("inner"));
    assert!(internal_link_pages(&pdf, 2)?.contains(&1));
    Ok(())
}

#[test]
fn link_formatting_boundaries_keep_pdf_links_and_one_note() -> Result<(), Error> {
    let source = "= Links\n\n*Before https://outer.example[*Bold mailto:inner@example.org[Inner] footnote:one[Only note.] tail* after] end*.\n\nReuse footnote:one[].\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    let pdf = render_parsed(&parsed)?;
    let text = pdf.extract_text(&[1])?;
    assert_eq!(text.matches("Only note.").count(), 1, "{text}");
    assert!(!text.contains('*'), "{text}");
    assert!(!text.contains("https://outer.example"), "{text}");
    let links = external_link_rects(&pdf, 1)?;
    for target in ["https://outer.example", "mailto:inner@example.org"] {
        assert!(
            links.iter().any(|(uri, _)| uri == target),
            "missing {target}: {links:?}"
        );
    }
    Ok(())
}

#[rstest::rstest]
fn verse_keeps_pdf_line_positions_and_macro_targets(
    #[values(false, true)] delimited: bool,
) -> Result<(), Error> {
    let fence = if delimited { "____\n" } else { "" };
    let stanza = if delimited { "\n" } else { "" };
    let source = format!(
        "= Verse\n\n[verse]\n{fence}First (C).\n  anchor:verse-target[]Second (TM).\n{stanza}Third (R). link:https://example.org/verse[Link] footnote:note[Verse note.]\n{fence}\nAfter <<verse-target,Target>> and footnote:note[].\n"
    );
    let parsed = parse(&source, &Options::default())?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    let pdf = render_parsed(&parsed)?;
    let text = pdf.extract_text(&[1])?;
    for content in ["First ©.", "Second ™.", "Third ®."] {
        assert!(text.contains(content), "{text}");
    }
    assert_eq!(text.matches("Verse note.").count(), 1, "{text}");
    let first = text_origin(&pdf, 1, "First ©.")?;
    let second = text_origin(&pdf, 1, "Second ™.")?;
    let third = text_origin(&pdf, 1, "Third ®. ")?;
    assert!(second.0 > first.0 + 2.0, "{first:?} {second:?}");
    assert!((third.0 - first.0).abs() < 0.01, "{first:?} {third:?}");
    assert!(first.1 > second.1 && second.1 > third.1);
    if delimited {
        assert!(second.1 - third.1 > 1.8 * (first.1 - second.1));
    }
    let links = external_link_rects(&pdf, 1)?;
    assert!(
        links
            .iter()
            .any(|(uri, _)| uri == "https://example.org/verse")
    );
    assert!(named_destinations(&pdf)?.contains_key("verse-target"));
    Ok(())
}

#[test]
#[cfg(feature = "pre-spec-subs")]
fn title_substitutions_keep_pdf_links_and_footnotes() -> Result<(), Error> {
    let source = "= Titles\n\n.Title link:https://example.org/title[Link (C)] footnote:note[Note (R).]\n[#target]\n[subs=\"none\"]\nBody (C) footnote:[Inactive note.].\n\nReuse footnote:note[] and see <<target,Target>>.\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    let pdf = render_parsed(&parsed)?;
    let text = pdf.extract_text(&[1])?;
    assert!(text.contains("Link ©"), "{text}");
    assert!(
        text.contains("Body (C) footnote:[Inactive note.]."),
        "{text}"
    );
    assert_eq!(text.matches("Note ®.").count(), 1, "{text}");
    let links = external_link_rects(&pdf, 1)?;
    let [(uri, _)] = links.as_slice() else {
        return Err("expected one title link annotation".into());
    };
    assert_eq!(uri, "https://example.org/title");
    let targets = named_destinations(&pdf)?;
    assert!(targets.contains_key("target"));
    Ok(())
}

#[test]
fn title_footnote_copies_keep_one_body_and_native_numbering() -> Result<(), Error> {
    for toc in ["", ":toc:\n"] {
        let source = format!(
            "= Notes\n{toc}\n[[heading,Heading]]\n== Heading footnote:headingnote[Heading note.]\n\n.Titled footnote:paragraph[Paragraph note.]\n[#target]\nParagraph.\n\n.Anonymous footnote:[Anonymous note.]\n[#anonymous]\nParagraph.\n\nBody footnote:[Body note.] and reused footnote:paragraph[].\n\nSee <<heading>>, <<target>>, and <<anonymous>>.\n"
        );
        let pdf = render_input(&source)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf
            .extract_text(&pages)?
            .split_whitespace()
            .collect::<String>();
        for note in [
            "Headingnote.",
            "Paragraphnote.",
            "Anonymousnote.",
            "Bodynote.",
        ] {
            assert_eq!(text.matches(note).count(), 1, "{toc}: {text}");
        }
        assert!(text.contains("4Bodynote."), "{toc}: {text}");
        let targets = named_destinations(&pdf)?;
        for target in ["heading", "target", "anonymous"] {
            assert!(targets.contains_key(target), "missing {target}");
        }
    }
    Ok(())
}

#[test]
fn plain_table_paragraph_linebreaks_keep_pdf_text_positions() -> Result<(), Error> {
    let actual = render_input(
        "= Table\n\n[cols=\"1\"]\n|===\n|Before.\n////\nInside.\n////\nAfter.\n\nSecond.\n|===\n",
    )?;
    let control = render_input(
        "= Table\n\n[cols=\"1\"]\n|===\n|Before. //// Inside. //// After.\n\nSecond.\n|===\n",
    )?;
    for text in ["Before. //// Inside. //// After.", "Second."] {
        let actual = text_origin(&actual, 1, text)?;
        let control = text_origin(&control, 1, text)?;
        assert!(
            (actual.0 - control.0).abs() < 0.01 && (actual.1 - control.1).abs() < 0.01,
            "{text}: {actual:?}, expected {control:?}"
        );
    }
    Ok(())
}

#[test]
#[cfg(feature = "pre-spec-subs")]
fn code_typography_preserves_escapes_and_pdf_navigation() -> Result<(), Error> {
    let actual = render_input(
        "= Code\n\n[subs=\"+replacements,+macros\"]\n----\n(C) \\(R) \\*literal\\* pass:r[\\(TM)] anchor:target[]https://example.org/type[Next \\*label\\*]\n----",
    )?;
    let control = render_input(
        "= Code\n\n[subs=\"+macros\"]\n----\n© (R) \\*literal\\* pass:[(TM)] anchor:target[]https://example.org/type[Next \\*label\\*]\n----",
    )?;
    assert!(actual.extract_text(&[1])?.contains(r"\*literal\*"));
    assert!(actual.extract_text(&[1])?.contains(r"Next \*label\*"));
    let origin = text_origin(&actual, 1, r"Next \*label\*")?;
    let expected = text_origin(&control, 1, r"Next \*label\*")?;
    assert!((origin.0 - expected.0).abs() < 0.01 && (origin.1 - expected.1).abs() < 0.01);
    let links = external_link_rects(&actual, 1)?;
    let expected_links = external_link_rects(&control, 1)?;
    let ([(uri, rectangle)], [(_, expected_rectangle)]) =
        (links.as_slice(), expected_links.as_slice())
    else {
        return Err("expected one URI annotation".into());
    };
    assert_eq!(uri, "https://example.org/type");
    assert!(
        rectangle
            .iter()
            .zip(expected_rectangle)
            .all(|(actual, expected)| (actual - expected).abs() < 0.01)
    );
    let targets = named_destinations(&actual)?;
    let expected_targets = named_destinations(&control)?;
    let target = resolve_destination(
        &actual,
        &targets,
        *targets.get("target").ok_or("missing target")?,
    )?
    .as_array()?;
    let expected_target = resolve_destination(
        &control,
        &expected_targets,
        *expected_targets
            .get("target")
            .ok_or("missing control target")?,
    )?
    .as_array()?;
    for (actual, expected) in target
        .get(2..4)
        .ok_or("missing target coordinates")?
        .iter()
        .zip(
            expected_target
                .get(2..4)
                .ok_or("missing control coordinates")?,
        )
    {
        assert!((actual.as_float()? - expected.as_float()?).abs() < 0.01);
    }
    Ok(())
}

#[test]
fn deferred_typography_keeps_pdf_link_and_anchor_positions() -> Result<(), Error> {
    for body in [
        "`{value}anchor:target[]https://example.org/type[Next]`.",
        #[cfg(feature = "pre-spec-subs")]
        "[subs=\"+attributes,+macros\"]\n----\n{value}anchor:target[]https://example.org/type[Next]\n----",
    ] {
        let actual = render_input(&format!("= Type\n:value: pass:r[(C)]\n\n{body}\n"))?;
        let expected = render_input(&format!("= Type\n:value: pass:r[©]\n\n{body}\n"))?;
        let origin = text_origin(&actual, 1, "Next")?;
        let control = text_origin(&expected, 1, "Next")?;
        assert!(
            (origin.0 - control.0).abs() < 0.01 && (origin.1 - control.1).abs() < 0.01,
            "{body}: {origin:?}, expected {control:?}"
        );
        let targets = named_destinations(&actual)?;
        let control_targets = named_destinations(&expected)?;
        let target = resolve_destination(
            &actual,
            &targets,
            targets.get("target").ok_or("missing target")?,
        )?
        .as_array()?;
        let control_target = resolve_destination(
            &expected,
            &control_targets,
            control_targets
                .get("target")
                .ok_or("missing control target")?,
        )?
        .as_array()?;
        let coordinates = target.get(2..4).ok_or("missing target coordinates")?;
        let control_coordinates = control_target
            .get(2..4)
            .ok_or("missing control coordinates")?;
        for (actual, expected) in coordinates.iter().zip(control_coordinates) {
            assert!(
                (actual.as_float()? - expected.as_float()?).abs() < 0.01,
                "{body}: anchor position differs"
            );
        }
        let links = external_link_rects(&actual, 1)?;
        let control_links = external_link_rects(&expected, 1)?;
        let ([(uri, rectangle)], [(_, control_rectangle)]) =
            (links.as_slice(), control_links.as_slice())
        else {
            return Err("expected one URI annotation".into());
        };
        assert_eq!(uri, "https://example.org/type");
        assert!(
            rectangle
                .iter()
                .zip(control_rectangle)
                .all(|(actual, expected)| (actual - expected).abs() < 0.01),
            "{body}: link annotation position differs"
        );
    }
    Ok(())
}

#[test]
fn constrained_monospace_boundaries_preserve_pdf_positions_and_links() -> Result<(), Error> {
    for content in [
        "café",
        "{blank}",
        "anchor:target[]",
        "xanchor:target[]y",
        "https://example.org[Site]",
    ] {
        let marked = render_input(&format!("= Bounds\n:blank:\n\n`{content}`**End**.\n"))?;
        let control = render_input(&format!("= Bounds\n:blank:\n\n``{content}``**End**.\n"))?;
        let actual = text_origin(&marked, 1, "End")?;
        let expected = text_origin(&control, 1, "End")?;
        assert!(
            (actual.0 - expected.0).abs() < 0.01 && (actual.1 - expected.1).abs() < 0.01,
            "{content}: {actual:?}, expected {expected:?}"
        );
        if content.contains("anchor:") {
            assert!(named_destinations(&marked)?.contains_key("target"));
        }
        if content.starts_with("https:") {
            assert!(
                external_link_rects(&marked, 1)?
                    .iter()
                    .any(|(uri, _)| uri == "https://example.org")
            );
        }
    }
    Ok(())
}

#[test]
fn inline_verbatim_links_keep_pdf_uri_annotations_and_ids() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/inline_verbatim_links.adoc"))?;
    let mut uris = Vec::new();
    for page in pdf.get_pages().keys() {
        uris.extend(
            external_link_rects(&pdf, *page)?
                .into_iter()
                .map(|(uri, _)| uri),
        );
    }
    for expected in [
        "https://example.org/label",
        "https://example.org/site",
        "mailto:person@example.org",
        "mailto:person@example.org?subject=Subject&body=Body",
        "https://example.org/bare",
        "https://example.org/bracketed",
        "other.pdf#chapter",
        "https://example.org/formatted",
        "https://example.org/outer",
        "https://example.org/inner",
        "https://example.org/anchor",
        "https://example.org/empty",
        "https://example.org/one",
        "https://example.org/two",
        "https://example.org/first",
        "https://example.org/second",
        "https://example.org/item",
        "https://example.org/cell",
        "https://example.org/query?x=1&y=2",
    ] {
        assert!(
            uris.iter().any(|uri| uri == expected),
            "missing {expected}: {uris:?}"
        );
    }
    assert!(
        !uris
            .iter()
            .any(|uri| uri.ends_with("/escaped") || uri.ends_with("/raw"))
    );
    let targets = named_destinations(&pdf)?;
    for id in [
        "label-id",
        "formatted-id",
        "label-target",
        "empty-label-id",
        "duplicate",
    ] {
        assert!(targets.contains_key(id), "missing {id}");
    }
    let pages = pdf.get_pages();
    let duplicate = *targets
        .get("duplicate")
        .ok_or("missing duplicate destination")?;
    let target_page = destination_page_id(&pdf, duplicate)?;
    let page = pages
        .iter()
        .find_map(|(page, id)| (*id == target_page).then_some(*page));
    assert!(
        pdf.extract_text(&[page.ok_or("missing duplicate page")?])?
            .contains("P17")
    );
    Ok(())
}

#[test]
fn inline_verbatim_links_preserve_local_reference_navigation() -> Result<(), Error> {
    // Different fonts split the copied title into separate PDF annotations.
    for (title, expected_pages) in [
        ("Target", &[2, 2, 1, 2][..]),
        (
            "Target `https://example.org/title[Code]`",
            &[2, 2, 1, 2, 2][..],
        ),
    ] {
        let pdf = render_input(&format!(
            "= Links\n\n`xref:target[Named]` and `<<target>>` and `xref:#[Top]`.\n\nSee <<target>>.\n\n<<<\n\n[[target]]\n== {title}\n"
        ))?;
        assert_eq!(internal_link_pages(&pdf, 1)?, expected_pages);
        assert!(external_link_rects(&pdf, 1)?.is_empty());
        assert!(pdf.extract_text(&[1])?.contains("Target"));
        if title.contains("https") {
            assert!(
                external_link_rects(&pdf, 2)?
                    .iter()
                    .any(|(uri, _)| uri == "https://example.org/title")
            );
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn inline_verbatim_links_honor_context_and_copied_title_ownership() -> Result<(), Error> {
    let pdf = render_input(include_str!(
        "fixtures/source/subs_inline_verbatim_links.adoc"
    ))?;
    assert!(
        !external_link_rects(&pdf, 1)?
            .iter()
            .any(|(uri, _)| uri.ends_with("/heading")),
        "the copied title must retain the TOC's heading link"
    );
    assert!(internal_link_pages(&pdf, 1)?.iter().all(|page| *page == 2));
    let mut uris = Vec::new();
    for page in pdf.get_pages().keys() {
        uris.extend(
            external_link_rects(&pdf, *page)?
                .into_iter()
                .map(|(uri, _)| uri),
        );
    }
    for expected in [
        "https://example.org/bold",
        "https://example.org/role",
        "https://example.org/pass",
        "https://example.org/attribute",
        "https://example.org/multiline",
        "https://example.org/spaces",
        "https://example.org/quotes-off",
        "other.manual.pdf#target",
        "other.pdf#target",
        "https://example.org/heading",
        "https://example.org/surrounding",
        "https://example.org/nested-code",
        "https://example.org/hidden",
        "https://example.org/hidden-two",
    ] {
        assert!(
            uris.iter().any(|uri| uri == expected),
            "missing {expected}: {uris:?}"
        );
    }
    assert!(!uris.iter().any(|uri| uri.ends_with("/disabled")));
    let targets = named_destinations(&pdf)?;
    let heading = *targets.get("heading-id").ok_or("missing heading ID")?;
    assert_eq!(
        destination_page_id(&pdf, heading)?,
        *pdf.get_pages().get(&2).ok_or("missing body page")?
    );
    Ok(())
}

#[test]
fn inline_verbatim_links_keep_glyph_positions_and_disjoint_rectangles() -> Result<(), Error> {
    for (linked, plain) in [
        (
            "xlink:https://example.org/[Label]y".to_owned(),
            "xLabely".to_owned(),
        ),
        (
            "xlink:https://example.org/[café]y".to_owned(),
            "xcaféy".to_owned(),
        ),
        (
            "xlink:https://example.org/[cafe\u{301}]y".to_owned(),
            "xcafe\u{301}y".to_owned(),
        ),
        (
            "link:https://example.org/[First\n  second]".to_owned(),
            "First\n  second".to_owned(),
        ),
        (
            format!("link:https://example.org/[{}]", "wrapped words ".repeat(35)),
            "wrapped words ".repeat(35),
        ),
        (
            format!("xlink:https://example.org/[{}]y", "longword".repeat(20)),
            format!("x{}y", "longword".repeat(20)),
        ),
        (
            "link:https://example.org/[longword]".repeat(20),
            "longword".repeat(20),
        ),
    ] {
        for context in ["", "[cols=\"1,3\"]\n|===\n|", "[.pre-wrap]\n"] {
            let suffix = if context.contains("cols") {
                "\n|Other\n|===\n"
            } else {
                "\n"
            };
            let marked =
                render_input(&format!("= Links\n\n{context}``{linked}``**End**.{suffix}"))?;
            let control =
                render_input(&format!("= Links\n\n{context}``{plain}``**End**.{suffix}"))?;
            let actual = text_origin(&marked, 1, "End")?;
            let expected = text_origin(&control, 1, "End")?;
            assert!(
                (actual.0 - expected.0).abs() < 0.01 && (actual.1 - expected.1).abs() < 0.01,
                "{context} {linked}: {actual:?}, expected {expected:?}"
            );
            let rectangles = external_link_rects(&marked, 1)?;
            assert!(!rectangles.is_empty());
            if linked.contains("wrapped")
                || (linked.contains("longword") && context.contains("cols"))
            {
                let mut rows = rectangles
                    .iter()
                    .map(|(_, [_, bottom, _, _])| bottom.to_bits())
                    .collect::<Vec<_>>();
                rows.sort_unstable();
                rows.dedup();
                assert!(
                    rows.len() > 1,
                    "{context} {linked}: wrapped code must have clickable links on multiple lines"
                );
            }
        }
    }
    for content in [
        "`https://outer.example[Left https://inner.example[Middle] right]`",
        "https://outer.example[Left `https://inner.example[Middle]` right]",
    ] {
        let pdf = render_input(&format!("= Links\n\n{content}.\n"))?;
        let mut rectangles = external_link_rects(&pdf, 1)?;
        rectangles.sort_by(|(_, [left, ..]), (_, [right, ..])| left.total_cmp(right));
        assert_eq!(rectangles.len(), 3, "{content}: {rectangles:?}");
        for pair in rectangles.windows(2) {
            let [(_, [_, _, right, _]), (_, [left, _, _, _])] = pair else {
                return Err("missing link pair".into());
            };
            assert!(
                *right <= *left + 0.01,
                "{content}: overlapping links: {pair:?}"
            );
        }
    }
    Ok(())
}

fn external_link_rects(pdf: &PdfDocument, page: u32) -> Result<Vec<(String, [f32; 4])>, Error> {
    let page = *pdf
        .get_pages()
        .get(&page)
        .ok_or("missing annotation page")?;
    let mut links = Vec::new();
    for annotation in pdf.get_page_annotations(page)? {
        let Ok(action) = annotation.get(b"A") else {
            continue;
        };
        let (_, action) = pdf.dereference(action)?;
        let Ok(uri) = action.as_dict()?.get(b"URI") else {
            continue;
        };
        let uri = String::from_utf8(uri.as_str()?.to_vec())?;
        let [left, bottom, right, top] = annotation.get(b"Rect")?.as_array()?.as_slice() else {
            return Err("invalid link rectangle".into());
        };
        links.push((
            uri,
            [
                left.as_float()?,
                bottom.as_float()?,
                right.as_float()?,
                top.as_float()?,
            ],
        ));
    }
    Ok(links)
}

#[test]
fn named_pdf_destinations_export_rendered_source_ids() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/named_destinations.adoc"))?;
    let destinations = named_destinations(&pdf)?;
    for id in [
        "document-top",
        "_name",
        "chapter",
        "inline",
        "shorthand",
        "bold",
        "italic",
        "mono",
        "mark",
        "sub",
        "super",
        "double",
        "single",
        "empty",
        "code",
        "café",
        "topic🚀",
        "a:b",
        "id-63686170746572",
        "url-id",
        "link-id",
        "mail-id",
        "paragraph",
        "listing",
        "cell",
        "item",
        "duplicate",
        "discrete",
        "_bibliography",
        "book",
    ] {
        assert!(
            destinations.contains_key(id),
            "missing {id}: {destinations:?}"
        );
    }
    for id in ["unused-header", "escaped", "raw"] {
        assert!(!destinations.contains_key(id), "unexpected {id}");
    }
    Ok(())
}

#[test]
fn named_pdf_destinations_keep_caller_built_document_titles() -> Result<(), Error> {
    let parsed = parse(
        include_str!("fixtures/source/named_destinations.adoc"),
        &Options::default(),
    )?;
    let header = parsed
        .document()
        .header
        .as_ref()
        .ok_or("missing fixture header")?;
    let mut document = Document::default();
    document.header = Some(
        acdc_parser::Header::new(header.title.clone(), header.location.clone())
            .with_metadata(header.metadata.clone()),
    );
    document.attributes = parsed.document().attributes.clone();
    let pdf = render_document(&document)?;
    let destinations = named_destinations(&pdf)?;
    assert!(destinations.contains_key("document-top"));
    assert!(!destinations.contains_key("unused-header"));
    Ok(())
}

#[test]
fn named_pdf_destinations_preserve_positions_and_first_ownership() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/named_destinations.adoc"))?;
    let destinations = named_destinations(&pdf)?;
    let pages = pdf.get_pages();
    for (id, marker) in [
        ("duplicate", "P11 "),
        ("book", "[book] P14 Book target."),
        ("code", "P03 "),
    ] {
        let target = *destinations.get(id).ok_or("missing named destination")?;
        let page_id = destination_page_id(&pdf, target)?;
        let page = pages
            .iter()
            .find_map(|(page, candidate)| (*candidate == page_id).then_some(*page))
            .ok_or("missing target page")?;
        assert!(
            pdf.extract_text(&[page])?.contains(marker),
            "{id}: missing {marker}"
        );
        let target = resolve_destination(&pdf, &destinations, target)?.as_array()?;
        let top = target.get(3).ok_or("missing target height")?.as_float()?;
        let (_, baseline) = text_origin(&pdf, page, marker)?;
        assert!(
            (0.0..20.0).contains(&(top - baseline)),
            "{id}: target {top}, text {baseline}"
        );
    }
    let chapter = *destinations
        .get("chapter")
        .ok_or("missing chapter destination")?;
    let collision = *destinations
        .get("id-63686170746572")
        .ok_or("missing collision destination")?;
    let chapter = resolve_destination(&pdf, &destinations, chapter)?.as_array()?;
    let collision = resolve_destination(&pdf, &destinations, collision)?.as_array()?;
    let chapter = chapter.get(3).ok_or("missing chapter height")?.as_float()?;
    let collision = collision
        .get(3)
        .ok_or("missing collision height")?
        .as_float()?;
    assert!(
        chapter > collision + 20.0,
        "public source ID must take precedence over the generated heading name"
    );
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn named_pdf_destinations_omit_unrendered_targets_and_copied_titles() -> Result<(), Error> {
    let source = include_str!("fixtures/source/subs_named_destinations.adoc");
    let parsed = parse(source, &Options::default())?;
    for id in ["omitted", "_empty_index"] {
        assert!(
            parsed.document().references.contains_key(id),
            "missing catalog control {id}"
        );
    }
    let pdf = render_parsed(&parsed)?;
    let destinations = named_destinations(&pdf)?;
    for id in ["heading-target", "code-target"] {
        assert!(destinations.contains_key(id), "missing {id}");
    }
    for id in [
        "unused-value",
        "disabled",
        "not-formatted",
        "omitted",
        "_empty_index",
    ] {
        assert!(!destinations.contains_key(id), "unexpected {id}");
    }
    let target = *destinations
        .get("heading-target")
        .ok_or("missing heading destination")?;
    let heading = resolve_destination(&pdf, &destinations, target)?.as_array()?;
    let top = heading.get(3).ok_or("missing heading height")?.as_float()?;
    let page_id = destination_page_id(&pdf, target)?;
    let page = pdf
        .get_pages()
        .into_iter()
        .find_map(|(page, id)| (id == page_id).then_some(page))
        .ok_or("missing heading page")?;
    let (_, baseline) = text_origin(&pdf, page, "P02 Heading body.")?;
    assert!(
        (0.0..45.0).contains(&(top - baseline)),
        "body heading must own the target, not the TOC: {top}, {baseline}"
    );
    Ok(())
}

#[test]
fn formatted_attribute_footnotes_register_at_use() -> Result<(), Error> {
    let pdf = render_input(include_str!(
        "fixtures/source/document_attribute_formatted.adoc"
    ))?;
    let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
    let text = pdf.extract_text(&pages)?;
    assert!(text.matches("Anonymous body.").count() > 1, "{text}");
    assert_eq!(text.matches("Named body.").count(), 1, "{text}");
    assert!(!text.contains("Unused body."), "{text}");
    assert!(!text.contains("Unused"), "{text}");
    Ok(())
}

#[test]
fn unused_attribute_footnotes_do_not_add_an_empty_pdf_page() -> Result<(), Error> {
    let pdf = render_input(
        "= T\n:unused: pass:m[footnote:[Unused body.]]\n\nBody ((Term)).\n\n[index]\n== Index\n",
    )?;
    assert_eq!(pdf.get_pages().len(), 1);
    assert!(!pdf.extract_text(&[1])?.contains("Unused body."));
    assert!(internal_link_pages(&pdf, 1)?.iter().all(|page| *page == 1));
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn formatted_attributes_keep_pdf_code_formatting_and_links() -> Result<(), Error> {
    let source = include_str!("fixtures/source/subs_document_attribute_formatted.adoc");
    for source in [
        source.to_owned(),
        source.replace(":source-highlighter: syntect", ""),
    ] {
        let pdf = render_input(&source)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        assert!(!text.contains("<strong>Bold</strong>"), "{text}");
        assert!(!text.contains("*Bold*"), "{text}");
        assert_eq!(text.matches("Note body.").count(), 1, "{text}");
        let mut urls = Vec::new();
        for page in pdf.get_pages().values() {
            for annotation in pdf.get_page_annotations(*page)? {
                if let Ok(action) = annotation.get(b"A") {
                    let (_, action) = pdf.dereference(action)?;
                    if let Ok(uri) = action.as_dict()?.get(b"URI") {
                        urls.push(String::from_utf8(uri.as_str()?.to_vec())?);
                    }
                }
            }
        }
        assert_eq!(
            urls,
            [
                "https://example.org",
                "https://example.org",
                "https://example.org"
            ]
        );
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn passthrough_character_references_preserve_visible_pdf_code() -> Result<(), Error> {
    let source =
        include_str!("fixtures/source/subs_passthrough_character_references_highlighting.adoc");
    for source in [
        source.to_owned(),
        source.replace(":source-highlighter: syntect", ""),
    ] {
        let pdf = render_input(&source)?;
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf
            .extract_text(&pages)?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for expected in [
            "H01 © <tag> &",
            "H02 &#169; &lt;tag&gt; &amp;",
            r"H03 &#169; \&#169;",
            "H04 &#9; &#x9; &#0000169; &#X00A9;",
        ] {
            assert!(text.contains(expected), "missing {expected:?}: {text}");
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_quotes_use_bold_and_italic_pdf_fonts() -> Result<(), Error> {
    let pdf =
        render_input("[source,text,subs=+quotes]\n----\nRegular *Bold* _Italic_ *_Both_*\n----\n")?;
    let page = *pdf.get_pages().get(&1).ok_or("missing page")?;
    let fonts = pdf
        .get_page_fonts(page)?
        .into_values()
        .map(|font| {
            Ok(String::from_utf8(
                font.get(b"BaseFont")?.as_name()?.to_vec(),
            )?)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    for face in [
        "IBMPlexMono-Bold",
        "IBMPlexMono-Italic",
        "IBMPlexMono-BoldItalic",
    ] {
        assert!(
            fonts.iter().any(|font| font.ends_with(face)),
            "{face}: {fonts:?}"
        );
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_quotes_preserve_pdf_destinations_and_notes() -> Result<(), Error> {
    let source = include_str!("fixtures/source/subs_verbatim_quotes_nested.adoc");
    for source in [
        source.to_owned(),
        source.replace(":blank:", ":source-highlighter: rouge\n:blank:"),
    ] {
        let pdf = render_input(&source)?;
        assert_eq!(internal_link_pages(&pdf, 1)?, [2, 2, 2]);
        let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
        let text = pdf.extract_text(&pages)?;
        assert_eq!(text.matches("Code note.").count(), 1, "{text}");
        assert!(!text.contains('*'), "{text}");
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
        assert!(targets.iter().any(|target| target == "https://example.org"));
        assert!(
            targets
                .iter()
                .any(|target| target == "https://example.org/outer")
        );
        assert!(targets.iter().all(|target| {
            ["https://example.org", "https://example.org/outer"].contains(&target.as_str())
        }));
    }
    Ok(())
}

#[test]
fn inline_verbatim_targets_preserve_pdf_destinations() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/inline_verbatim_targets.adoc"))?;
    let pages = pdf
        .get_pages()
        .keys()
        .map(|page| Ok((*page, pdf.extract_text(&[*page])?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let reference_page = pages.last().ok_or("missing reference page")?.0;
    let expected = [
        "P01", "P02", "P03", "P04", "P04", "P05", "P06", "P07", "P08", "P09", "P10", "P11", "P12",
        "P13", "P14", "P15", "P16", "P17", "P18", "P20", "P20", "P22", "P22", "P22", "P22", "P22",
        "P22", "P22",
    ]
    .map(|marker| {
        pages
            .iter()
            .find(|(_, text)| text.contains(marker))
            .map(|(page, _)| *page)
            .ok_or_else(|| format!("missing target occurrence: {marker}").into())
    })
    .into_iter()
    .collect::<Result<Vec<_>, Error>>()?;
    assert_eq!(internal_link_pages(&pdf, reference_page)?, expected);
    Ok(())
}

#[test]
fn inline_verbatim_targets_keep_glyph_positions() -> Result<(), Error> {
    let long_word = "word".repeat(8);
    let table_marked = format!(
        "[cols=\"1\",width=\"20%\"]\n|===\n|``{long_word}anchor:target[]{long_word}`` **End**.\n|===\n"
    );
    let table_plain =
        format!("[cols=\"1\",width=\"20%\"]\n|===\n|``{long_word}{long_word}`` **End**.\n|===\n");
    for (marked, plain) in [
        ("`1anchor:target[]2`**End**.", "`12`**End**."),
        ("`anchor:target[]12`**End**.", "`12`**End**."),
        ("`12anchor:target[]`**End**.", "`12`**End**."),
        ("``anchor:target[]``**End**.", "**End**."),
        ("`1anchor:first[]anchor:second[]2`**End**.", "`12`**End**."),
        ("``1 anchor:target[]2``**End**.", "``1 2``**End**."),
        ("``1anchor:target[] 2``**End**.", "``1 2``**End**."),
        ("``1 anchor:target[] 2``**End**.", "``1  2``**End**."),
        (
            "`first anchor:target[]\n second`. **End**.",
            "`first second`. **End**.",
        ),
        ("`1[[target]]2`**End**.", "`12`**End**."),
        ("``1 [#target]*2*``**End**.", "``1 *2*``**End**."),
        (
            "``eanchor:target[]\u{0301}``**End**.",
            "``e\u{0301}``**End**.",
        ),
        (table_marked.as_str(), table_plain.as_str()),
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

#[cfg(feature = "pre-spec-subs")]
#[test]
fn inline_verbatim_targets_keep_duplicate_and_copied_title_ownership() -> Result<(), Error> {
    let pdf = render_input(include_str!(
        "fixtures/source/subs_inline_verbatim_targets.adoc"
    ))?;
    let pages = pdf
        .get_pages()
        .keys()
        .map(|page| Ok((*page, pdf.extract_text(&[*page])?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let reference_page = pages.last().ok_or("missing reference page")?.0;
    let expected = ["P01", "P02", "P03", "P05", "P06", "P07", "P08"]
        .map(|marker| {
            pages
                .iter()
                .find(|(_, text)| text.contains(marker))
                .map(|(page, _)| *page)
                .ok_or_else(|| format!("missing target occurrence: {marker}").into())
        })
        .into_iter()
        .collect::<Result<Vec<_>, Error>>()?;
    assert_eq!(internal_link_pages(&pdf, reference_page)?, expected);
    Ok(())
}

#[test]
fn inline_verbatim_targets_reach_the_glyph_position() -> Result<(), Error> {
    for content in ["Short".to_owned(), "wrapped words ".repeat(40)] {
        let pdf = render_input(&format!(
            "= Position\n\n``{content}anchor:target[]``**End**.\n\n<<<\n\nSee <<target,Target>>.\n"
        ))?;
        let reference_page = *pdf
            .get_pages()
            .keys()
            .last()
            .ok_or("missing reference page")?;
        assert_eq!(internal_link_pages(&pdf, reference_page)?, [1]);
        let page = *pdf
            .get_pages()
            .get(&reference_page)
            .ok_or("missing reference page")?;
        let annotations = pdf.get_page_annotations(page)?;
        let [link] = annotations.as_slice() else {
            return Err("expected one target link".into());
        };
        let destination = link
            .get(b"Dest")
            .or_else(|_| link.get(b"A")?.as_dict()?.get(b"D"))?;
        let targets = named_destinations(&pdf)?;
        let destination = resolve_destination(&pdf, &targets, destination)?;
        let [_, kind, left, top, ..] = destination.as_array()?.as_slice() else {
            return Err("incomplete target position".into());
        };
        assert_eq!(kind.as_name()?, b"XYZ");
        let (left, top) = (left.as_float()?, top.as_float()?);
        let (x, baseline) = text_origin(&pdf, 1, "End")?;
        assert!(
            (left - x).abs() < 0.01 && (0.0..20.0).contains(&(top - baseline)),
            "{content}: target ({left}, {top}), glyph ({x}, {baseline})"
        );
    }
    Ok(())
}

#[test]
fn inline_anchor_spacing_preserves_pdf_destinations() -> Result<(), Error> {
    let pdf = render_input(include_str!("fixtures/source/inline_anchor_spacing.adoc"))?;
    let pages = pdf
        .get_pages()
        .keys()
        .map(|page| Ok((*page, pdf.extract_text(&[*page])?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let link_page = pages.last().ok_or("missing reference page")?.0;
    let expected = [
        "P01", "P02", "P03", "P04", "P05", "P06", "P07", "P08", "P08", "P09", "P10", "P11", "P12",
        "P15", "P13", "P16", "P17", "P18", "P19", "F20",
    ]
    .map(|marker| {
        pages
            .iter()
            .find(|(_, text)| text.contains(marker))
            .map(|(page, _)| *page)
            .ok_or_else(|| format!("missing anchor occurrence: {marker}").into())
    })
    .into_iter()
    .collect::<Result<Vec<_>, Error>>()?;
    assert_eq!(internal_link_pages(&pdf, link_page)?, expected);
    Ok(())
}

#[test]
fn inline_anchor_spacing_keeps_glyph_positions() -> Result<(), Error> {
    // A separate bold run exposes spacing changes without cross-anchor kerning.
    for (marked, plain) in [
        ("1anchor:target[]**End**.", "1**End**."),
        ("1[[target]]**End**.", "1**End**."),
        ("1anchor:first[]anchor:second[]**End**.", "1**End**."),
        ("1 anchor:target[]**End**.", "1 **End**."),
        ("1anchor:target[] **End**.", "1 **End**."),
        // An empty passthrough keeps the two authored spaces in separate text runs.
        ("1 anchor:target[] **End**.", "1 pass:[] **End**."),
        ("anchor:target[]**End**.", "**End**."),
        ("1 [#target]**End**.", "1 **End**."),
        // Unconstrained spans keep both inputs as code after the preceding digit.
        ("1anchor:target[]``2``**End**.", "1``2``**End**."),
        (
            "https://example.org[1]anchor:target[]**End**.",
            "https://example.org[1]**End**.",
        ),
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

#[cfg(feature = "pre-spec-subs")]
#[test]
fn inline_anchor_spacing_in_code_keeps_pdf_destinations() -> Result<(), Error> {
    let source = include_str!("fixtures/source/subs_inline_anchor_spacing.adoc");
    for source in [
        source.to_owned(),
        source.replace(":manmanual:", ":source-highlighter: rouge\n:manmanual:"),
    ] {
        let pdf = render_input(&source)?;
        assert_eq!(pdf.get_pages().len(), 2);
        assert_eq!(internal_link_pages(&pdf, 2)?, [1; 7]);
    }
    Ok(())
}

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
fn index_dash_context_keeps_catalog_labels_independent_of_body_context() -> Result<(), Error> {
    for body in [
        "indexterm:[pass:r[--]]",
        "prefixindexterm:[pass:r[--]]tail",
        "indexterm2:[pass:r[--]]tail",
        "**prefix indexterm:[pass:r[--]] tail**",
        "__prefix indexterm:[pass:r[--]] tail__",
    ] {
        let pdf = render_input(&format!(
            "= Catalog context\n\n{body}\n\n<<<\n\n[index]\n== Index\n"
        ))?;
        let page = *pdf
            .get_pages()
            .keys()
            .last()
            .ok_or("missing catalog page")?;
        let catalog = pdf.extract_text(&[page])?;
        assert!(catalog.contains('—'), "{body}: {catalog}");
        assert!(!catalog.contains("--"), "{body}: {catalog}");
    }
    Ok(())
}

#[test]
#[cfg(feature = "pre-spec-subs")]
fn index_dash_context_preserves_code_destinations_and_link_positions() -> Result<(), Error> {
    let input = "= Context\n\n[source,text,subs=\"specialchars,macros,replacements\"]\n----\n\
        12anchor:before[]indexterm2:[--]anchor:after[]34 link:https://next.example/context[Next]\n----\n";
    let control = "= Context\n\n[source,text,subs=\"specialchars,macros,replacements\"]\n----\n\
        12anchor:before[]—\u{200b}anchor:after[]34 link:https://next.example/context[Next]\n----\n";
    let actual = render_input(input)?;
    let expected = render_input(control)?;
    let actual_links = external_link_rects(&actual, 1)?;
    let expected_links = external_link_rects(&expected, 1)?;
    let ([(uri, rectangle)], [(expected_uri, expected_rectangle)]) =
        (actual_links.as_slice(), expected_links.as_slice())
    else {
        return Err("expected one URI annotation per document".into());
    };
    assert_eq!(uri, expected_uri);
    for (position, control_position) in rectangle.iter().zip(expected_rectangle) {
        assert!((position - control_position).abs() < 0.01);
    }
    let destinations = named_destinations(&actual)?;
    let expected_destinations = named_destinations(&expected)?;
    for id in ["before", "after"] {
        let actual_target = *destinations.get(id).ok_or("missing destination")?;
        let expected_target = *expected_destinations
            .get(id)
            .ok_or("missing control destination")?;
        let actual_target =
            resolve_destination(&actual, &destinations, actual_target)?.as_array()?;
        let expected_target =
            resolve_destination(&expected, &expected_destinations, expected_target)?.as_array()?;
        for index in [2, 3] {
            let position = actual_target
                .get(index)
                .ok_or("missing coordinate")?
                .as_float()?;
            let control_position = expected_target
                .get(index)
                .ok_or("missing control coordinate")?
                .as_float()?;
            assert!(
                (position - control_position).abs() < 0.01,
                "{id}: {position} != {control_position}"
            );
        }
    }
    Ok(())
}

#[test]
#[cfg(feature = "pre-spec-subs")]
fn index_replacement_context_keeps_code_anchor_ownership() -> Result<(), Error> {
    for (body, control) in [
        (
            "12 anchor:before[]indexterm2:[--]anchor:after[] 34",
            "12anchor:before[]\u{2009}—\u{2009}anchor:after[]34",
        ),
        (
            "12anchor:before[]-indexterm2:[-]anchor:after[]34",
            "12anchor:before[]—\u{200b}anchor:after[]34",
        ),
        (
            "12anchor:before[]indexterm2:[-]-anchor:after[]34",
            "12anchor:before[]—\u{200b}anchor:after[]34",
        ),
        (
            "12anchor:before[]indexterm2:[']anchor:after[]s",
            "12anchor:before[]’anchor:after[]s",
        ),
    ] {
        let document = |text| {
            format!(
                "= Context\n\n[source,text,subs=\"specialchars,macros,replacements\"]\n----\n{text} link:https://next.example/ownership[Next]\n----\n"
            )
        };
        let actual = render_input(&document(body))?;
        let expected = render_input(&document(control))?;
        let actual_destinations = named_destinations(&actual)?;
        let expected_destinations = named_destinations(&expected)?;
        for id in ["before", "after"] {
            let actual_target = resolve_destination(
                &actual,
                &actual_destinations,
                actual_destinations.get(id).ok_or("missing destination")?,
            )?
            .as_array()?;
            let expected_target = resolve_destination(
                &expected,
                &expected_destinations,
                expected_destinations
                    .get(id)
                    .ok_or("missing control destination")?,
            )?
            .as_array()?;
            for coordinate in [2, 3] {
                let actual_coordinate = actual_target
                    .get(coordinate)
                    .ok_or("missing coordinate")?
                    .as_float()?;
                let expected_coordinate = expected_target
                    .get(coordinate)
                    .ok_or("missing control coordinate")?
                    .as_float()?;
                assert!(
                    (actual_coordinate - expected_coordinate).abs() < 0.01,
                    "{body}: {id}"
                );
            }
        }
        let actual_links = external_link_rects(&actual, 1)?;
        let expected_links = external_link_rects(&expected, 1)?;
        let ([(uri, rectangle)], [(control_uri, control_rectangle)]) =
            (actual_links.as_slice(), expected_links.as_slice())
        else {
            return Err("expected one URI annotation per document".into());
        };
        assert_eq!(uri, control_uri);
        for (actual, expected) in rectangle.iter().zip(control_rectangle) {
            assert!((actual - expected).abs() < 0.01, "{body}: following link");
        }
    }
    Ok(())
}

#[test]
fn index_dash_context_preserves_following_pdf_link_positions() -> Result<(), Error> {
    // Digits avoid the kerning changes caused by a zero-width index marker.
    for (marked, plain) in [
        ("12indexterm2:[--]34", "12—\u{200b}34"),
        ("12indexterm2:[--34]", "12—\u{200b}34"),
        ("indexterm2:[12--]34", "12—\u{200b}34"),
        ("12indexterm:[Hidden]indexterm2:[--]34", "12—\u{200b}34"),
        (
            "12anchor:before[]indexterm2:[--]anchor:after[]34",
            "12—\u{200b}34",
        ),
        (r"12indexterm2:[\--]34", r"12\--34"),
        ("12 indexterm2:[--] 34", "12\u{2009}—\u{2009}34"),
        ("12-indexterm2:[-]34", "12—\u{200b}34"),
        ("12indexterm2:[-]-34", "12—\u{200b}34"),
        ("12indexterm2:[']s", "12’s"),
    ] {
        let marked_pdf = render_input(&format!(
            "= Context\n\n{marked} link:https://next.example/context[Next]\n"
        ))?;
        let plain_pdf = render_input(&format!(
            "= Context\n\n{plain} link:https://next.example/context[Next]\n"
        ))?;
        let actual = external_link_rects(&marked_pdf, 1)?;
        let control = external_link_rects(&plain_pdf, 1)?;
        let ([(uri, rectangle)], [(control_uri, control_rectangle)]) =
            (actual.as_slice(), control.as_slice())
        else {
            return Err("expected one URI annotation per document".into());
        };
        assert_eq!(uri, control_uri);
        assert_eq!(uri, "https://next.example/context");
        for (position, expected) in rectangle.iter().zip(control_rectangle) {
            assert!(
                (position - expected).abs() < 0.01,
                "{marked}: {rectangle:?} != {control_rectangle:?}"
            );
        }
    }
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
fn empty_quoted_attributes_keep_pdf_anchor_destinations() -> Result<(), Error> {
    let source = "= Empty formatting\n:blank:\n\nSee <<empty,Empty target>>.\n\n<<<\n\nBefore [#empty]*{blank}* after.\n";
    let pdf = render_input(source)?;
    assert_eq!(internal_link_pages(&pdf, 1)?, [2]);
    let text = pdf
        .extract_text(&[2])?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("Before after."), "{text}");
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
    let parsed = parse(
        include_str!("fixtures/source/subs_verbatim_links.adoc"),
        &Options::default(),
    )?;
    let (pdf, warnings) = render_document_with_warnings(parsed.document())?;
    assert_missing_glyph_codes(&warnings, &["U+65E5", "U+672C", "U+8A9E"]);
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
    let parsed = parse(
        include_str!("fixtures/source/subs_verbatim_links_highlighting.adoc"),
        &Options::default(),
    )?;
    let (pdf, warnings) = render_document_with_warnings(parsed.document())?;
    assert_missing_glyph_codes(&warnings, &["U+65E5", "U+672C", "U+8A9E"]);
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
    render_document(parsed.document())
}

fn render_document(document: &Document<'_>) -> Result<PdfDocument, Error> {
    let (pdf, warnings) = render_document_with_warnings(document)?;
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(pdf)
}

fn render_document_with_warnings(
    document: &Document<'_>,
) -> Result<(PdfDocument, Vec<Warning>), Error> {
    let processor = Processor::new(
        ConverterOptions::default(),
        Options::builder().with_attributes(document.attributes.clone().into_inputs()),
    )?;
    let source = WarningSource::new("pdf");
    let mut warnings = Vec::new();
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    let mut output = Vec::new();
    processor.write_to(document, &mut output, None, None, &mut diagnostics)?;
    Ok((PdfDocument::load_mem(&output)?, warnings))
}

fn assert_missing_glyph_codes(warnings: &[Warning], codes: &[&str]) {
    assert_eq!(warnings.len(), codes.len(), "{warnings:?}");
    for code in codes {
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.message.contains("missing glyph")
                    && warning.message.contains(code))
                .count(),
            1,
            "{warnings:?}"
        );
    }
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
        let targets = named_destinations(&pdf)?;
        let target = resolve_destination(&pdf, &targets, target)?;
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

fn named_destinations(pdf: &PdfDocument) -> Result<HashMap<String, &Object>, Error> {
    fn collect<'a>(
        pdf: &'a PdfDocument,
        node: &'a Object,
        targets: &mut HashMap<String, &'a Object>,
    ) -> Result<(), Error> {
        let (_, node) = pdf.dereference(node)?;
        let node = node.as_dict()?;
        if let Ok(entries) = node.get(b"Names") {
            let entries = entries.as_array()?;
            let (pairs, remainder) = entries.as_chunks::<2>();
            assert!(remainder.is_empty(), "incomplete destination name pair");
            for [name, target] in pairs {
                // The PDF renderer stores destination keys as UTF-8 bytes, not PDF text strings.
                let name = String::from_utf8(name.as_str()?.to_vec())?;
                assert!(
                    targets.insert(name.clone(), target).is_none(),
                    "duplicate destination {name}"
                );
            }
        }
        if let Ok(children) = node.get(b"Kids") {
            for child in children.as_array()? {
                collect(pdf, child, targets)?;
            }
        }
        Ok(())
    }
    let mut targets = HashMap::new();
    if let Ok(names) = pdf.catalog()?.get(b"Names") {
        let (_, names) = pdf.dereference(names)?;
        if let Ok(destinations) = names.as_dict()?.get(b"Dests") {
            collect(pdf, destinations, &mut targets)?;
        }
    }
    Ok(targets)
}

fn resolve_destination<'a>(
    pdf: &'a PdfDocument,
    targets: &HashMap<String, &'a Object>,
    destination: &'a Object,
) -> Result<&'a Object, Error> {
    let (_, destination) = pdf.dereference(destination)?;
    match destination {
        Object::String(..) | Object::Name(_) => {
            let name = if let Object::Name(bytes) = destination {
                String::from_utf8(bytes.clone())?
            } else {
                String::from_utf8(destination.as_str()?.to_vec())?
            };
            resolve_destination(
                pdf,
                targets,
                targets.get(&name).ok_or("missing named destination")?,
            )
        }
        Object::Dictionary(dictionary) => resolve_destination(pdf, targets, dictionary.get(b"D")?),
        Object::Array(_) => Ok(destination),
        Object::Null
        | Object::Boolean(_)
        | Object::Integer(_)
        | Object::Real(_)
        | Object::Stream(_)
        | Object::Reference(_) => Err("invalid PDF destination".into()),
    }
}

fn internal_link_pages(pdf: &PdfDocument, page: u32) -> Result<Vec<u32>, Error> {
    let pages = pdf.get_pages();
    let page_id = pages.get(&page).ok_or("missing source page")?;
    let targets = named_destinations(pdf)?;
    pdf.get_page_annotations(*page_id)?
        .into_iter()
        .map(|annotation| {
            let destination = annotation
                .get(b"Dest")
                .or_else(|_| annotation.get(b"A")?.as_dict()?.get(b"D"))?;
            let destination = resolve_destination(pdf, &targets, destination)?;
            let target = destination_page_id(pdf, destination)?;
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
