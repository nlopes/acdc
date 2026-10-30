use std::{error::Error, fs, path::PathBuf};

use acdc_parser::{
    CompatibilityMode, Options, ParseResult, SafeMode,
    ast::{BlockKind, ContentRef, ElementIdRef, InlineKind, LocationRef, StructuralRef},
    parse, parse_file,
};

type TestResult = Result<(), Box<dyn Error>>;

fn content_ids<'a>(content: ContentRef<'a>, ids: &mut Vec<ElementIdRef<'a>>) {
    for inline in content.inlines() {
        if let Some(id) = inline.id() {
            ids.push(id);
        }
        let children = match inline.kind() {
            InlineKind::Bold(span) => Some(span.content()),
            InlineKind::Italic(span) => Some(span.content()),
            InlineKind::Monospace(span) => Some(span.content()),
            InlineKind::Highlight(span) => Some(span.content()),
            InlineKind::Subscript(span) => Some(span.content()),
            InlineKind::Superscript(span) => Some(span.content()),
            InlineKind::CurvedQuotation(span) => Some(span.content()),
            InlineKind::CurvedApostrophe(span) => Some(span.content()),
            InlineKind::Footnote(note) => Some(note.content()),
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Callout(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | _ => None,
        };
        if let Some(children) = children {
            content_ids(children, ids);
        }
    }
}

fn ids(parsed: &ParseResult) -> Vec<ElementIdRef<'_>> {
    let mut ids = Vec::new();
    for node in parsed.document_ref().descendants() {
        if let StructuralRef::Block(block) = node {
            if let Some(title) = block.title() {
                content_ids(title, &mut ids);
            }
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                content_ids(paragraph.content(), &mut ids);
            }
        }
    }
    ids
}

fn assert_source_span(
    source: &str,
    location: LocationRef<'_>,
    expected: &str,
    check_bytes: bool,
) -> TestResult {
    let start = source.find(expected).ok_or("authored ID spelling")?;
    let end = start + expected.char_indices().last().ok_or("nonempty ID")?.0;
    if check_bytes {
        // Preprocessing normalizes CRLF before assigning byte offsets.
        let normalized = source.replace("\r\n", "\n");
        let byte_start = normalized.find(expected).ok_or("normalized ID spelling")?;
        let byte_end = byte_start + expected.char_indices().last().ok_or("nonempty ID")?.0;
        assert_eq!(
            (location.absolute_start(), location.absolute_end()),
            (byte_start, byte_end),
            "{expected}"
        );
    }
    for (offset, position) in [(start, location.start()), (end, location.end())] {
        let prefix = source.get(..offset).ok_or("source boundary")?;
        assert_eq!(
            position.line() as usize,
            prefix.bytes().filter(|&byte| byte == b'\n').count() + 1,
            "{expected}"
        );
        assert_eq!(
            position.column() as usize,
            prefix
                .rsplit('\n')
                .next()
                .ok_or("source line")?
                .chars()
                .count()
                + 1,
            "{expected}"
        );
    }
    Ok(())
}

#[rstest::rstest]
fn image_id_fixture_uses_original_source_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let primary = root.join("inline_image_id_locations.adoc");
    let child = root.join("includes/inline_image_id_locations.adoc");
    let source = fs::read_to_string(&primary)?;
    let included = fs::read_to_string(&child)?;
    let parsed = parse_file(
        &primary,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let ids = ids(&parsed);
    assert_eq!(ids.len(), 5);
    for id in ids {
        let spelling = if id.as_str() == "expanded-picture" {
            "{picture-id}"
        } else {
            id.as_str()
        };
        if id.as_str() == "included-picture" {
            assert_source_span(&included, id.location(), spelling, false)?;
            assert_eq!(
                parsed.resolve_location(id.location()).file,
                Some(child.canonicalize()?)
            );
        } else {
            assert_source_span(&source, id.location(), spelling, true)?;
        }
    }
    Ok(())
}

#[rstest::rstest]
fn retained_macro_ids_map_once_after_prior_blocks_and_unicode(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("\n", "\r\n")] newline: &str,
    #[values(
        "image:bird.png[Alt,id=target]",
        "icon:heart[id=target]",
        "link:guide.adoc[Guide,id=target]",
        "https://example.org[Guide,id=target]",
        "mailto:team@example.org[Team,id=target]",
        "xref:guide[Guide,id=target]",
        "indexterm2:[Term,id=target]"
    )]
    markup: &str,
) -> TestResult {
    let source = format!("Earlier.{newline}{newline}é😀 {markup} tail.{newline}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let ids = ids(&parsed);
    let [id] = ids.as_slice() else {
        return Err(format!("expected one ID in {markup}: {ids:?}").into());
    };
    assert_eq!(id.as_str(), "target");
    assert_source_span(&source, id.location(), "target", true)
}

#[rstest::rstest]
fn nested_image_ids_keep_literal_and_expanded_spelling(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("*{}*", "_{}_", "`{}`", "#{}#", "footnote:[{}]", ".{}\nParagraph.")] context: &str,
    #[values("é😀", "{picture-id}")] spelling: &str,
) -> TestResult {
    let image = format!("image:bird.png[Alt,id=\"{spelling}\"]");
    let body = context.replace("{}", &image);
    let source = format!(":picture-id: expanded\n\nEarlier.\n\n{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let ids = ids(&parsed);
    let [id] = ids.as_slice() else {
        return Err(format!("expected one ID in {body}: {ids:?}").into());
    };
    assert_eq!(
        id.as_str(),
        if spelling == "{picture-id}" {
            "expanded"
        } else {
            spelling
        }
    );
    assert_source_span(&source, id.location(), spelling, true)
}

#[rstest::rstest]
fn nested_selected_includes_keep_image_id_file_and_byte_offsets(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let dir = tempfile::tempdir()?;
    let child =
        "// Omitted\r\n\r\né😀 image:bird.png[Alt,id=selected-picture]\r\n\r\n// Omitted\r\n";
    fs::write(dir.path().join("child.adoc"), child)?;
    fs::write(
        dir.path().join("middle.adoc"),
        "include::child.adoc[lines=2..4]\n",
    )?;
    let primary = dir.path().join("root.adoc");
    fs::write(&primary, "Earlier.\n\ninclude::middle.adoc[]\n")?;
    let parsed = parse_file(
        &primary,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let ids = ids(&parsed);
    let [id] = ids.as_slice() else {
        return Err("expected included ID".into());
    };
    assert_source_span(child, id.location(), "selected-picture", true)?;
    assert_eq!(
        parsed
            .resolve_location(id.location())
            .file
            .ok_or("included source")?
            .canonicalize()?,
        dir.path().join("child.adoc").canonicalize()?
    );
    assert_eq!(
        id.location()
            .start()
            .include_chain()
            .ok_or("include chain")?
            .collect::<Vec<_>>(),
        ["middle.adoc", "child.adoc"]
    );
    Ok(())
}
