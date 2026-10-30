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
fn formatting_ids_keep_exact_authored_spans(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("\n", "\r\n")] newline: &str,
    #[values(
        "*text*",
        "**text**",
        "_text_",
        "__text__",
        "`text`",
        "``text``",
        "#text#",
        "##text##",
        "~text~",
        "^text^",
        "\"`text`\"",
        "'`text`'"
    )]
    markup: &str,
    #[values("target", "é😀", "{span-id}")] spelling: &str,
) -> TestResult {
    let source = format!(
        ":span-id: expanded{newline}{newline}Earlier.{newline}{newline}α🚀 [#discarded.role#{spelling}]{markup} tail.{newline}"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let ids = ids(&parsed);
    let [id] = ids.as_slice() else {
        return Err(format!("expected one ID: {ids:?}").into());
    };
    assert_eq!(
        id.as_str(),
        if spelling == "{span-id}" {
            "expanded"
        } else {
            spelling
        }
    );
    assert_source_span(&source, id.location(), spelling, true)
}

#[rstest::rstest]
fn nested_formatting_ids_map_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "*{}*",
        "_{}_",
        "`{}`",
        "footnote:[{}]",
        ".{}\nParagraph.",
        "[cols=a]\n|===\n|{}\n|==="
    )]
    context: &str,
    #[values("target", "{span-id}")] spelling: &str,
) -> TestResult {
    let body = context.replace("{}", &format!("[#outer]##é😀 [#{spelling}]~text~##"));
    let source = format!(":span-id: expanded\n\nEarlier.\n\n{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let ids = ids(&parsed);
    assert_eq!(ids.len(), 2, "{body}: {ids:?}");
    for id in ids {
        let spelling = if id.as_str() == "expanded" {
            "{span-id}"
        } else {
            id.as_str()
        };
        assert_source_span(&source, id.location(), spelling, true)?;
    }
    Ok(())
}

#[rstest::rstest]
fn formatting_id_fixture_keeps_include_provenance(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let primary = root.join("inline_span_id_locations.adoc");
    let child = root.join("includes/inline_span_id_locations.adoc");
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
    assert_eq!(ids.len(), 15);
    for id in ids {
        let spelling = if id.as_str() == "expanded-span" {
            "{span-id}"
        } else {
            id.as_str()
        };
        if id.as_str() == "included-span" {
            assert_source_span(&included, id.location(), spelling, false)?;
            assert_eq!(
                parsed
                    .resolve_location(id.location())
                    .file
                    .ok_or("included source")?
                    .canonicalize()?,
                child.canonicalize()?
            );
        } else {
            assert_source_span(&source, id.location(), spelling, true)?;
        }
    }
    Ok(())
}

#[rstest::rstest]
fn selected_nested_include_keeps_formatting_id_offsets(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let dir = tempfile::tempdir()?;
    let source = "// omitted\r\n\r\né😀 [#selected-span]**text**\r\n\r\n// omitted\r\n";
    fs::write(dir.path().join("child.adoc"), source)?;
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
    assert_source_span(source, id.location(), "selected-span", true)?;
    assert_eq!(
        parsed
            .resolve_location(id.location())
            .file
            .ok_or("included source")?
            .canonicalize()?,
        dir.path().join("child.adoc").canonicalize()?
    );
    Ok(())
}

#[rstest::rstest]
fn duplicate_id_spelling_uses_the_winning_token(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let source = "Earlier.\n\n[#same#same]*same*\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let ids = ids(&parsed);
    let [id] = ids.as_slice() else {
        return Err("expected one ID".into());
    };
    let start = source.find("#same]*").ok_or("winning token")? + 1;
    assert_eq!(id.as_str(), "same");
    assert_eq!(
        (id.location().absolute_start(), id.location().absolute_end()),
        (start, start + 3)
    );
    Ok(())
}

#[rstest::rstest]
fn generated_formatting_id_retains_its_reference_span(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("[#{fragment}-suffix]*text*", "{formatted}")] body: &str,
) -> TestResult {
    let source = format!(
        ":fragment: expanded\n:formatted: [#generated]*text*\n\nEarlier.\n\n[subs=\"attributes,quotes\"]\n{body}\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let ids = ids(&parsed);
    let [id] = ids.as_slice() else {
        return Err("expected one ID".into());
    };
    let spelling = if body == "{formatted}" {
        "{formatted}"
    } else {
        "{fragment}-suffix"
    };
    assert_eq!(
        id.as_str(),
        if body == "{formatted}" {
            "generated"
        } else {
            "expanded-suffix"
        }
    );
    assert_source_span(&source, id.location(), spelling, true)
}
