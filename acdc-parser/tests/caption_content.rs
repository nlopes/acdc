use acdc_parser::{
    CaptionKind, CompatibilityMode, Options, SafeMode,
    ast::{
        AttributeValueRef, AttributesRef, BlockRef, CaptionRef, ContentRef, FootnoteDefinitionRef,
        InlineKind, InlineRef,
    },
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn text(node: InlineRef<'_>) -> Result<&str, Error> {
    if let InlineKind::Plain(plain) = node.kind() {
        Ok(plain.text())
    } else if let InlineKind::Raw(raw) = node.kind() {
        Ok(raw.text())
    } else {
        Err(format!("expected text, got {node:?}").into())
    }
}

fn caption_content(attributes: AttributesRef<'_>) -> Option<ContentRef<'_>> {
    if let Some(AttributeValueRef::Content(content)) = attributes.get("caption") {
        Some(content)
    } else {
        None
    }
}

#[rstest::rstest]
fn nested_caption_references_use_the_declaration_snapshot(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":other: Before\n:word: \\{other}\n\n.Title\n[caption='{word}']\n:other: After\n====\nBody.\n====";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let node = parsed
        .document()
        .blocks()
        .filter_map(BlockRef::attributes)
        .find_map(|metadata| caption_content(metadata).and_then(|content| content.inlines().next()))
        .ok_or("missing caption")?;
    assert_eq!(
        text(node)?,
        if mode == CompatibilityMode::Asciidoctor {
            "Before"
        } else {
            "{other}"
        }
    );
    Ok(())
}

#[rstest::rstest]
fn introduced_caption_markup_is_parsed_independently_of_body_substitutions(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":prefix: *Prefix*\n\n.Title\n[caption='{prefix}',subs=none]\n====\n*Body*.\n====";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let node = parsed
        .document()
        .blocks()
        .next()
        .and_then(BlockRef::attributes)
        .and_then(caption_content)
        .and_then(|content| content.inlines().next())
        .ok_or("missing caption")?;
    let InlineKind::Bold(bold) = node.kind() else {
        return Err(format!("expected bold caption: {node:?}").into());
    };
    assert_eq!(
        text(bold.content().inlines().next().ok_or("missing bold text")?)?,
        "Prefix"
    );
    Ok(())
}

#[rstest::rstest]
fn a_single_character_caption_keeps_its_authored_byte_span(
    #[values("x", "λ")] value: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(".Title\n[caption='{value}']\n====\nBody.\n====");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let node = parsed
        .document()
        .blocks()
        .next()
        .and_then(BlockRef::attributes)
        .and_then(caption_content)
        .and_then(|content| content.inlines().next())
        .ok_or("missing caption")?;
    let start = source.find(value).ok_or("missing authored character")?;
    assert_eq!(node.location().absolute_start(), start);
    assert_eq!(node.location().absolute_end(), start);
    assert_eq!(node.location().start(), node.location().end());
    Ok(())
}

#[rstest::rstest]
fn included_caption_nodes_keep_the_authored_reference_and_file(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        root.join("compatibility_caption_locations.adoc"),
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let node = parsed
        .document()
        .blocks()
        .filter_map(BlockRef::attributes)
        .filter_map(caption_content)
        .filter_map(|content| content.inlines().next())
        .find(|node| {
            parsed
                .resolve_location(node.location())
                .file
                .as_ref()
                .is_some_and(|file| file.ends_with("includes/compatibility_caption_locations.adoc"))
        })
        .ok_or("missing included caption")?;
    let InlineKind::Bold(bold) = node.kind() else {
        return Err("expected bold included caption".into());
    };
    for node in std::iter::once(node).chain(bold.content().inlines()) {
        let source = parsed.resolve_location(node.location());
        assert_eq!(
            source.file,
            Some(root.join("includes/compatibility_caption_locations.adoc"))
        );
        assert_eq!(
            (source.location.start.line, source.location.end.line),
            (3, 3)
        );
        assert_eq!(
            (source.location.start.column, source.location.end.column),
            (11, 18)
        );
    }
    assert_eq!(parsed.document().footnotes().count(), 2);
    Ok(())
}

#[rstest::rstest]
fn selected_caption_and_alt_content_registers_footnotes_in_source_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        root.join("compatibility_caption_snapshots.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let expected = if mode == CompatibilityMode::Draft {
        vec![
            Some("selected"),
            Some("body"),
            Some("caption_first"),
            Some("caption_second"),
        ]
    } else {
        vec![
            Some("selected"),
            Some("body"),
            Some("caption_first"),
            Some("alt_second"),
            Some("alt_first"),
            Some("caption_second"),
        ]
    };
    assert_eq!(
        parsed
            .document()
            .footnotes()
            .map(FootnoteDefinitionRef::id)
            .collect::<Vec<_>>(),
        expected
    );
    Ok(())
}

#[rstest::rstest]
fn custom_and_empty_captions_leave_each_kind_numbering_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("example", "figure", "table", "listing")] kind: &str,
) -> Result<(), Error> {
    let (body, expected) = match kind {
        "example" => ("====\nBody.\n====", CaptionKind::Example),
        "figure" => ("image::photo.png[Photo]", CaptionKind::Figure),
        "table" => ("|===\n|Cell\n|===", CaptionKind::Table),
        "listing" => ("----\ncode\n----", CaptionKind::Listing),
        _ => return Err("unexpected caption kind".into()),
    };
    let source = format!(
        ":{kind}-caption: Label\n\n.Custom\n[caption='*Prefix*: ']\n{body}\n\n.Empty\n[caption='']\n{body}\n\n.Numbered\n{body}\n\n.Next\n{body}"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    let captions = parsed
        .document()
        .blocks()
        .filter_map(BlockRef::attributes)
        .filter_map(|metadata| CaptionRef::resolve(metadata, document.attributes(), None))
        .collect::<Vec<_>>();
    let [custom, empty, first, second] = captions.as_slice() else {
        return Err("expected four captions".into());
    };
    assert!(matches!(custom, CaptionRef::CustomContent(content) if !content.is_empty()));
    assert!(matches!(empty, CaptionRef::CustomContent(content) if content.is_empty()));
    for (caption, ordinal) in [(first, 1), (second, 2)] {
        assert!(
            matches!(caption, CaptionRef::Numbered {kind, number:Some(number), ..} if *kind == expected && number.get() == ordinal)
        );
    }
    assert_eq!(parsed.document().highest_caption_number(expected), 2);
    Ok(())
}

#[rstest::rstest]
fn cross_reference_targets_retain_the_selected_caption_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":prefix: *Prefix*\n\n[#target]\n.Title\n[caption='{prefix}: ']\n====\nBody.\n====\n\nSee <<target>>.";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    let metadata = document
        .blocks()
        .next()
        .and_then(BlockRef::attributes)
        .ok_or("missing block")?;
    let reference = document
        .references()
        .get("target")
        .ok_or("missing reference target")?;
    assert_eq!(
        reference.caption(),
        CaptionRef::resolve(metadata, document.attributes(), None)
    );
    let Some(CaptionRef::CustomContent(content)) = reference.caption() else {
        return Err("missing custom reference caption".into());
    };
    assert!(matches!(
        content.inlines().next().map(InlineRef::kind),
        Some(InlineKind::Bold(_))
    ));
    assert!(
        serde_json::to_value(document)?
            .pointer("/blocks/0/metadata/attributes/caption")
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
fn expanded_caption_and_alt_keep_attribute_order_when_source_spans_coincide(
    #[values(false, true)] caption_first: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let caption = "caption='Caption footnote:caption[Caption note]: '";
    let alt = "alt='Alt footnote:alt[Alt note]'";
    let entries = if caption_first {
        format!("{caption},{alt}")
    } else {
        format!("{alt},{caption}")
    };
    let source = format!(":media: {entries}\n\n.Title\n[{{media}}]\nimage::photo.png[]");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let expected = if mode == CompatibilityMode::Draft {
        vec![Some("caption")]
    } else if caption_first {
        vec![Some("caption"), Some("alt")]
    } else {
        vec![Some("alt"), Some("caption")]
    };
    assert_eq!(
        parsed
            .document()
            .footnotes()
            .map(FootnoteDefinitionRef::id)
            .collect::<Vec<_>>(),
        expected
    );
    let content = parsed
        .document()
        .blocks()
        .next()
        .and_then(BlockRef::attributes)
        .and_then(caption_content)
        .ok_or("missing caption")?;
    let start = source
        .find("{media}")
        .ok_or("missing attribute reference")?;
    for node in content.inlines() {
        assert_eq!(
            (
                node.location().absolute_start(),
                node.location().absolute_end()
            ),
            (start, start + "{media}".len() - 1)
        );
    }
    Ok(())
}

#[rstest::rstest]
fn custom_caption_reference_content_stays_structured_in_exports(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("example", "figure", "table", "listing")] kind: &str,
) -> Result<(), Error> {
    let body = match kind {
        "example" => "====\nBody.\n====",
        "figure" => "image::photo.png[Photo]",
        "table" => "|===\n|Cell\n|===",
        "listing" => "----\ncode\n----",
        _ => return Err("unexpected caption kind".into()),
    };
    let source = format!(
        ":prefix: *First*\n\n[#target,caption='{{prefix}}. ']\n:prefix: *Later*\n.Title\n{body}\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let reference = parsed
        .document()
        .references()
        .get("target")
        .ok_or("target")?;
    let Some(CaptionRef::CustomContent(prefix)) = reference.caption() else {
        return Err("expected parsed caption".into());
    };
    let native = serde_json::to_value(prefix)?;
    assert_eq!(
        native
            .pointer("/0/variant")
            .and_then(serde_json::Value::as_str),
        Some("strong")
    );
    assert_eq!(
        native
            .pointer("/0/inlines/0/value")
            .and_then(serde_json::Value::as_str),
        Some("First")
    );
    #[cfg(feature = "asg")]
    {
        let exported =
            acdc_parser::asg::inlines_with_profile(prefix, acdc_parser::asg::Profile::Acdc)?;
        assert_eq!(
            exported
                .pointer("/0/variant")
                .and_then(serde_json::Value::as_str),
            Some("strong")
        );
        assert_eq!(
            exported
                .pointer("/0/inlines/0/value")
                .and_then(serde_json::Value::as_str),
            Some("First")
        );
        acdc_parser::asg::document_with_profile(
            parsed.document(),
            acdc_parser::asg::Profile::Acdc,
        )?;
    }
    assert_eq!(serde_json::to_value(prefix)?, native);
    Ok(())
}
