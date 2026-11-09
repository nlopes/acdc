use acdc_parser::{
    CompatibilityMode, ContentAttribute, Document, Location, Options,
    ast::{ContentRef, CrossReferenceRef, DocumentRef, InlineKind},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn xrefs(document: DocumentRef<'_>) -> Vec<CrossReferenceRef<'_>> {
    document
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(|node| {
            if let InlineKind::CrossReference(xref) = node.kind() {
                Some(xref)
            } else {
                None
            }
        })
        .collect()
}

fn text(content: ContentRef<'_>) -> String {
    content
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(value) = node.kind() {
                value.text().to_owned()
            } else if let InlineKind::Bold(value) = node.kind() {
                text(value.content())
            } else {
                String::new()
            }
        })
        .collect()
}

#[rstest::rstest]
fn body_context_is_ordered_and_survives_prepared_repair(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[[guide,Anchor label]]\n= Main: Subtitle\n:word: Header\n:reftext: {word} label\n\n<<guide>> xref:#[]\n\n:word: Body\n:reftext: *{word}* label\n:word: Later\n\n<<guide>> xref:#[] <<guide,Explicit>>\n\n:reftext:\n\n<<guide>>\n\n:reftext!:\n\n<<guide>>\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    let contexts = |doc| {
        xrefs(doc)
            .iter()
            .map(|xref| xref.document_reftext().map(text))
            .collect::<Vec<_>>()
    };
    let expected = vec![
        None,
        None,
        Some("Body label".into()),
        Some("Body label".into()),
        None,
        Some(String::new()),
        Some(String::new()),
    ];
    assert_eq!(contexts(parsed.document()), expected);
    assert_eq!(
        text(xrefs(parsed.document()).get(4).ok_or("explicit")?.text()),
        "Explicit"
    );
    assert_eq!(
        parsed
            .document()
            .attributes()
            .get("reftext")
            .and_then(acdc_parser::DocumentAttributeValue::as_str),
        Some("Header label")
    );
    let before = serde_json::to_value(parsed.document())?;
    let mut owned = Document::from(parsed.document());
    owned.rebuild_references_and_toc()?;
    assert_eq!(contexts(DocumentRef::from(&owned)), expected);
    assert_eq!(serde_json::to_value(DocumentRef::from(&owned))?, before);
    let nodes = xrefs(parsed.document());
    let mut copy = acdc_parser::CrossReference::from(*nodes.get(2).ok_or("body")?);
    copy.set_document_reftext(Some(ContentAttribute::default()));
    assert!(
        CrossReferenceRef::from(&copy)
            .document_reftext()
            .ok_or("empty")?
            .is_empty()
    );
    copy.set_document_reftext(None);
    assert!(CrossReferenceRef::from(&copy).document_reftext().is_none());
    assert_eq!(
        nodes
            .get(2)
            .ok_or("original")?
            .document_reftext()
            .map(text)
            .as_deref(),
        Some("Body label")
    );
    Ok(())
}

#[rstest::rstest]
fn header_label_locations_use_the_winning_value(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] caller: bool,
) -> Result<(), Error> {
    let source =
        "[[guide]]\n= Main\n:reftext: First\n:reftext: *Winning* label\n\nxref:#[] <<guide>>\n";
    let builder = Options::builder().with_compatibility_mode(mode);
    let options = if caller {
        builder.with_attribute("reftext", "Caller").build()?
    } else {
        builder.build()?
    };
    let parsed = parse(source, &options)?;
    for target in ["", "guide"] {
        let label = parsed
            .document()
            .references()
            .get(target)
            .ok_or("target")?
            .xreflabel()
            .ok_or("label")?;
        let first = label.inlines().next().ok_or("inline")?;
        let location = Location::from(first.location());
        if caller {
            assert!(location.is_unknown(), "{location:?}");
        } else {
            assert_eq!(location.start.line, 4);
            assert_eq!(location.start.column, 11);
        }
    }
    Ok(())
}

#[rstest::rstest]
fn included_labels_keep_declaration_file_and_line(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("header.adoc"), ":reftext: *Header* label\n")?;
    std::fs::write(
        dir.path().join("body.adoc"),
        ":reftext: *Included* label\n\n<<guide>> xref:#[]\n",
    )?;
    let path = dir.path().join("main.adoc");
    std::fs::write(
        &path,
        "[[guide]]\n= Main\ninclude::header.adoc[]\n\nBefore <<guide>>.\n\ninclude::body.adoc[]\n\nAfter <<guide>>.\n",
    )?;
    let parsed = parse_file(
        &path,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    let label = parsed
        .document()
        .references()
        .get("guide")
        .ok_or("target")?
        .xreflabel()
        .ok_or("label")?;
    let location = Location::from(label.inlines().next().ok_or("header")?.location());
    assert_eq!(location.start.line, 1);
    assert_eq!(
        location.start.file.as_deref().map(Vec::as_slice),
        Some(["header.adoc".to_owned()].as_slice())
    );
    for node in xrefs(parsed.document()).into_iter().skip(1) {
        let context = node.document_reftext().ok_or("body context")?;
        assert_eq!(text(context), "Included label");
        let location = Location::from(context.inlines().next().ok_or("body")?.location());
        assert_eq!(location.start.line, 1);
        assert_eq!(location.start.column, 11);
        assert_eq!(
            location.start.file.as_deref().map(Vec::as_slice),
            Some(["body.adoc".to_owned()].as_slice())
        );
    }
    Ok(())
}

#[rstest::rstest]
fn nested_blocks_keep_source_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(("--", "--"), ("====", "===="), ("* Parent\n+\n--", "--"))] delimiters: (&str, &str),
) -> Result<(), Error> {
    let (open, close) = delimiters;
    let source = format!(
        "[[guide]]\n= Main\n:reftext: Header\n\n<<guide>>\n\n{open}\n:reftext: Nested\n\n<<guide>>\n{close}\n\n<<guide>>\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let labels = xrefs(parsed.document())
        .iter()
        .map(|xref| xref.document_reftext().map(text))
        .collect::<Vec<_>>();
    assert_eq!(labels, [None, Some("Nested".into()), Some("Nested".into())]);
    Ok(())
}

#[rstest::rstest]
fn body_context_respects_locks_and_declaration_expansion(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] locked: bool,
) -> Result<(), Error> {
    let builder = Options::builder().with_compatibility_mode(mode);
    let options = if locked {
        builder.with_attribute("reftext", "Caller").build()?
    } else {
        builder
            .with_default_attribute("reftext", "Caller")
            .build()?
    };
    let source = "[[guide]]\n= Main\n\n<<guide>>\n\n:reftext: {reftext} next\n\n<<guide>>\n\n:reftext!:\n\n<<guide>>\n";
    let parsed = parse(source, &options)?;
    let labels = xrefs(parsed.document())
        .iter()
        .map(|xref| xref.document_reftext().map(text))
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        if locked {
            vec![None, None, None]
        } else {
            vec![None, Some("Caller next".into()), Some(String::new())]
        }
    );
    Ok(())
}

#[rstest::rstest]
fn table_cell_context_does_not_escape_to_siblings_or_outer_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[[guide]]\n= Main\n\nBefore <<guide>>.\n\n[cols=\"a,a\"]\n|===\n|:reftext: Cell label\n\n<<guide>>\n|<<guide>>\n|===\n\nAfter <<guide>>.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let labels = xrefs(parsed.document())
        .iter()
        .map(|xref| xref.document_reftext().map(text))
        .collect::<Vec<_>>();
    assert_eq!(labels, [None, Some("Cell label".into()), None, None]);
    Ok(())
}

#[cfg(feature = "asg")]
#[rstest::rstest]
fn named_asg_keeps_context_separate_from_authored_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::asg;
    let parsed = parse(
        "[[guide]]\n= Main\n\n:reftext: *Body* label\n\n<<guide>> <<guide,Explicit>>\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let output = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let node = output.pointer("/blocks/1/inlines/0").ok_or("xref")?;
    assert_eq!(
        node.pointer("/document_reftext/0/variant"),
        Some(&serde_json::json!("strong"))
    );
    assert_eq!(
        node.pointer("/document_reftext/0/location/0/line"),
        Some(&serde_json::json!(4))
    );
    assert_eq!(node.get("inlines"), Some(&serde_json::json!([])));
    assert!(
        output
            .pointer("/blocks/1/inlines/2/document_reftext")
            .is_none()
    );
    let occurrence = ContentAttribute::new(vec![acdc_parser::InlineNode::from(
        parsed
            .document()
            .content_occurrences()
            .flat_map(|field| field.content().inlines())
            .find(|node| matches!(node.kind(), InlineKind::CrossReference(_)))
            .ok_or("xref")?,
    )]);
    assert!(matches!(asg::export_inlines(ContentRef::from(&occurrence)),
        Err(asg::Error::Unsupported { path, .. }) if path == "/0/document_reftext"));
    assert!(matches!(
        asg::export_document(parsed.document()),
        Err(asg::Error::Unsupported { .. })
    ));
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn unicode_value_endpoints_remain_in_the_declaration(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[[guide]]\n= Main\n:reftext: É中\n\n<<guide>>\n\n:reftext: Ω終\n\n<<guide>>\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let header = parsed
        .document()
        .references()
        .get("guide")
        .ok_or("header")?
        .xreflabel()
        .ok_or("label")?;
    let body = xrefs(parsed.document())
        .get(1)
        .ok_or("xref")?
        .document_reftext()
        .ok_or("context")?;
    for (content, label, line) in [(header, "É中", 3), (body, "Ω終", 7)] {
        assert_eq!(text(content), label);
        let location = Location::from(content.inlines().next().ok_or("inline")?.location());
        assert_eq!(location.start.line, line);
        assert_eq!(location.end.line, line);
        let start = source.find(label).ok_or("source")?;
        assert_eq!(location.absolute_start, start);
        assert_eq!(
            location.absolute_end,
            start + label.char_indices().last().ok_or("last")?.0
        );
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn derived_labels_do_not_register_their_inline_ids() -> Result<(), Error> {
    use acdc_parser::asg;
    let parsed = parse(
        "[[guide]]\n= Main\n\n:reftext: [#derived]*Body*\n\n<<guide>> <<guide>>\n",
        &Options::default(),
    )?;
    assert!(!parsed.document().references().contains("derived"));
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/1/inlines/0/document_reftext/0/id"),
        Some(&serde_json::json!("derived"))
    );
    assert!(value.pointer("/references/derived").is_none());
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn caller_label_export_has_no_fabricated_source_coordinates() -> Result<(), Error> {
    use acdc_parser::asg;
    let parsed = parse(
        "[[guide]]\n= Main\n\n<<guide>>\n",
        &Options::builder()
            .with_attribute("reftext", "Caller")
            .build()?,
    )?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/references/guide/reftext/0/value"),
        Some(&serde_json::json!("Caller"))
    );
    assert!(
        value
            .pointer("/references/guide/reftext/0/location")
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
fn prepared_reordering_does_not_replay_document_attributes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[[guide]]\n= Main\n:reftext: Header\n\nBefore <<guide>>.\n\n:reftext: Body\n\nAfter <<guide>>.\n\n:reftext: Unused\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut owned = Document::from(parsed.document());
    let reversed = owned.take_blocks().rev();
    owned.set_blocks(reversed);
    owned.set_attributes(acdc_parser::DocumentAttributes::default());
    owned.rebuild_references_and_toc()?;
    let read = DocumentRef::from(&owned);
    assert_eq!(
        xrefs(read)
            .iter()
            .map(|xref| xref.document_reftext().map(text))
            .collect::<Vec<_>>(),
        [Some("Body".into()), None]
    );
    assert_eq!(
        read.references()
            .get("guide")
            .ok_or("target")?
            .xreflabel()
            .map(text)
            .as_deref(),
        Some("Header")
    );
    Ok(())
}
