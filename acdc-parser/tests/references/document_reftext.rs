use acdc_parser::{
    Anchor, BlockMetadata, CompatibilityMode, ContentAttribute, Document, InlineNode, Location,
    Options, Plain, ReferenceRepairError, Title,
    ast::{AnchorRef, AttributeValueRef, BlockKind, ContentRef, DocumentRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;
const MODES: [CompatibilityMode; 2] = [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft];

fn text(content: ContentRef<'_>) -> String {
    content
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(plain) = node.kind() {
                plain.text().to_owned()
            } else if let InlineKind::Bold(span) = node.kind() {
                text(span.content())
            } else {
                String::new()
            }
        })
        .collect()
}

#[test]
fn document_reftext_overrides_the_header_label_for_both_targets() -> Result<(), Error> {
    for mode in MODES {
        for (attributes, expected) in [
            ("", Some("Header label")),
            (":reftext: *Document* label", Some("Document label")),
            (":reftext:", None),
            (":reftext!:", None),
        ] {
            let source = format!(
                "[[guide,Header label]]\n= Main: Subtitle\n{attributes}\n\n<<guide>>; xref:#[].\n"
            );
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
            for target in ["guide", ""] {
                let reference = parsed.document().references().get(target).ok_or("target")?;
                assert_eq!(
                    reference.xreflabel().map(text).as_deref(),
                    expected,
                    "{mode:?} {attributes:?} {target}"
                );
                assert_eq!(
                    reference.title().map(text).as_deref(),
                    Some("Main: Subtitle")
                );
            }
            #[cfg(feature = "asg")]
            {
                let asg = acdc_parser::asg::export_document_with_profile(
                    parsed.document(),
                    acdc_parser::asg::Profile::Acdc,
                )?;
                if expected.is_none() {
                    assert_eq!(
                        asg.pointer("/references/guide/reftext"),
                        Some(&serde_json::Value::Null)
                    );
                }
            }
            let mut document = Document::from(parsed.document());
            document.rebuild_references_and_toc()?;
            for target in ["guide", ""] {
                assert_eq!(
                    DocumentRef::from(&document)
                        .references()
                        .get(target)
                        .ok_or("repaired target")?
                        .xreflabel()
                        .map(text)
                        .as_deref(),
                    expected
                );
            }
        }
    }
    Ok(())
}

#[rstest::rstest]
fn authored_header_content_survives_export_and_reference_repair(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("[[guide,*Header* label]]", "[#guide,reftext='*Header* label']")] metadata: &str,
    #[values("<<guide>>.", "xref:#[].", "<<guide>> and xref:#[].")] body: &str,
) -> Result<(), Error> {
    let source = format!("{metadata}\n= Main: Subtitle\n:reftext: *Document* label\n\n{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let read = parsed.document();
    let before = serde_json::to_value(read)?;
    let header = read.header().ok_or("header")?;
    let authored =
        if let Some(AttributeValueRef::Content(content)) = header.attributes().get("reftext") {
            content
        } else {
            header
                .primary_anchor()
                .and_then(AnchorRef::reftext)
                .ok_or("authored header label")?
        };
    assert_eq!(text(authored), "Header label");
    let label = read
        .references()
        .get("guide")
        .ok_or("header target")?
        .xreflabel()
        .ok_or("document label")?;
    assert_eq!(text(label), "Document label");
    assert!(matches!(
        label.inlines().next().ok_or("label inline")?.kind(),
        InlineKind::Bold(_)
    ));
    let prepared_label = ContentAttribute::from(label);
    #[cfg(feature = "asg")]
    {
        let asg =
            acdc_parser::asg::export_document_with_profile(read, acdc_parser::asg::Profile::Acdc)?;
        assert_eq!(
            asg.pointer("/header/reftext/0/inlines/0/value")
                .and_then(serde_json::Value::as_str),
            Some("Header")
        );
        assert_eq!(
            asg.pointer("/references/guide/reftext/0/inlines/0/value")
                .and_then(serde_json::Value::as_str),
            Some("Document")
        );
        assert_eq!(
            asg.pointer("/references/guide/owner")
                .and_then(serde_json::Value::as_str),
            Some("/header")
        );
    }
    assert_eq!(serde_json::to_value(read)?, before);
    let mut document = Document::from(read);
    document.rebuild_references_and_toc()?;
    let repaired = DocumentRef::from(&document);
    assert_eq!(
        repaired
            .references()
            .get("guide")
            .ok_or("repaired target")?
            .xreflabel(),
        Some(ContentRef::from(&prepared_label))
    );
    assert_eq!(serde_json::to_value(repaired)?, before);
    Ok(())
}

#[rstest::rstest]
fn changed_header_ids_keep_the_prepared_document_label(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] top_reference: bool,
) -> Result<(), Error> {
    let body = if top_reference { "xref:#[]." } else { "Body." };
    let source = format!(
        "[[guide,Header label]]\n= Main\n:reftext: *Document* label\n\n{body}\n\n== Section\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let label = ContentAttribute::from(
        parsed
            .document()
            .references()
            .get("guide")
            .ok_or("header target")?
            .xreflabel()
            .ok_or("label")?,
    );
    let mut document = Document::from(parsed.document());
    let mut header = document.take_header().ok_or("header")?;
    header.set_metadata(
        BlockMetadata::new().with_id(Some(Anchor::new("renamed", Location::default()))),
    )?;
    header.set_title(Title::new(vec![InlineNode::PlainText(Plain::new(
        "Changed",
        Location::default(),
    ))]));
    document.set_header(Some(header));
    document.rebuild_references_and_toc()?;
    let read = DocumentRef::from(&document);
    assert!(!read.references().contains("guide"));
    let reference = read
        .references()
        .get("renamed")
        .ok_or("new header target")?;
    assert_eq!(reference.xreflabel(), Some(ContentRef::from(&label)));
    assert_eq!(reference.title().map(text).as_deref(), Some("Changed"));
    assert_eq!(read.toc_entries().count(), 1);
    assert_eq!(read.references().contains(""), top_reference);
    let before = serde_json::to_value(read)?;
    let _ = document.take_references();
    assert!(matches!(
        document.rebuild_references_and_toc(),
        Err(ReferenceRepairError::MissingDocumentReftext { .. })
    ));
    assert_eq!(serde_json::to_value(DocumentRef::from(&document))?, before);
    Ok(())
}

#[rstest::rstest]
fn document_label_assignment_respects_caller_locks(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source =
        "[[guide,Header label]]\n= Main\n:reftext: Source label\n\n<<guide>> and xref:#[].\n";
    for (options, expected) in [
        (
            Options::builder()
                .with_compatibility_mode(mode)
                .with_attribute("reftext", "Caller label")
                .build()?,
            Some("Caller label"),
        ),
        (
            Options::builder()
                .with_compatibility_mode(mode)
                .with_attribute("reftext", false)
                .build()?,
            None,
        ),
        (
            Options::builder()
                .with_compatibility_mode(mode)
                .with_default_attribute("reftext", "Caller label")
                .build()?,
            Some("Source label"),
        ),
        (
            Options::builder()
                .with_compatibility_mode(mode)
                .with_default_attribute("reftext", false)
                .build()?,
            Some("Source label"),
        ),
    ] {
        let parsed = parse(source, &options)?;
        for id in ["", "guide"] {
            assert_eq!(
                parsed
                    .document()
                    .references()
                    .get(id)
                    .ok_or("target")?
                    .xreflabel()
                    .map(text)
                    .as_deref(),
                expected
            );
        }
    }
    Ok(())
}

#[rstest::rstest]
fn natural_references_use_the_document_label_without_reserving_the_header_label(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[guide,Header label]]\n= Main\n:reftext: Document label\n\n[[other,Header label]]\nOther.\n\n<<Document label>> and <<Header label>>.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    let BlockKind::Paragraph(paragraph) =
        parsed.document().blocks().last().ok_or("paragraph")?.kind()
    else {
        return Err("paragraph".into());
    };
    let targets = paragraph
        .content()
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::CrossReference(xref) = inline.kind() {
                Some(xref.target())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(targets, ["guide", "other"]);
    let mut document = Document::from(parsed.document());
    document.rebuild_references_and_toc()?;
    assert_eq!(
        DocumentRef::from(&document)
            .references()
            .get("guide")
            .ok_or("guide")?
            .xreflabel()
            .map(text)
            .as_deref(),
        Some("Document label")
    );
    Ok(())
}
