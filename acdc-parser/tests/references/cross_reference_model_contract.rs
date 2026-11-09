use acdc_parser::{
    CrossReference, InlineMacro, InlineNode, Options, XrefCaptionLabel, XrefStyle,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef, ReferenceRef},
    parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn cross_reference_syntax_classifies_punctuation_before_catalog_lookup() -> Result<(), Error> {
    for (source, target, local) in [
        ("<<:colon,Label>>", ":colon", true),
        ("xref::colon[Label]", ":colon", true),
        ("<<a-b.c:d>>", "a-b.c:d", true),
        ("xref:a-b.c:d[]", "a-b.c:d", false),
        ("xref:#a-b.c:d[]", "#a-b.c:d", true),
        ("xref:http:local[]", "http:local", true),
        ("xref:dir.name/topic:one[]", "dir.name/topic:one", true),
        ("xref:guide.adoc#topic:one[]", "guide.adoc#topic:one", false),
    ] {
        let parsed = parse_inline(source, &Options::default())?;
        let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
        let [InlineKind::CrossReference(xref)] = &*parsed
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err(format!(
                "expected cross-reference for {source}: {:?}",
                parsed_inlines.as_slice()
            )
            .into());
        };
        assert_eq!(
            (xref.target(), xref.target_is_local()),
            (target, local),
            "{source}"
        );
        assert_eq!(xref.location().absolute_start(), 0, "{source}");
        assert_eq!(xref.location().absolute_end(), source.len() - 1, "{source}");
        assert_eq!(
            xref.location().end().column(),
            u32::try_from(source.chars().count())?,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn passthrough_cross_reference_targets_keep_source_classification() -> Result<(), Error> {
    for (source, target, local) in [
        ("xref:pass:[topic:one][]", "topic:one", true),
        ("xref:pass:[a-b.c:d][]", "a-b.c:d", false),
        ("xref:pass:[#a-b.c:d][]", "a-b.c:d", true),
    ] {
        let parsed = parse(source, &Options::default())?;
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        let [InlineKind::CrossReference(xref)] = &*paragraph
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err(format!("expected cross-reference for {source}").into());
        };
        assert_eq!((xref.target(), xref.target_is_local()), (target, local));
        assert_eq!(xref.location().absolute_start(), 0);
        assert_eq!(xref.location().absolute_end(), source.len() - 1);
        assert_eq!(
            xref.location().end().column(),
            u32::try_from(source.chars().count())?
        );
    }
    Ok(())
}

#[test]
fn compatibility_mode_keeps_the_entire_cross_reference_target_local() -> Result<(), Error> {
    let parsed = parse(
        ":compat-mode:\n\n<<topic:one>> xref:topic:one[] xref:a-b.c:d[] xref:#topic:one[] xref:guide.adoc[]\n",
        &Options::default(),
    )?;
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("missing paragraph".into());
    };
    let targets = paragraph
        .content()
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::CrossReference(xref) = inline.kind() {
                assert!(xref.target_is_local());
                Some(xref.target())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        [
            "topic:one",
            "topic:one",
            "a-b.c:d",
            "#topic:one",
            "guide.adoc"
        ]
    );
    Ok(())
}

#[test]
fn unused_block_metadata_does_not_register_references_or_footnotes() -> Result<(), Error> {
    // Fixture JSON omits the document's reference and footnote catalogs.
    for source in [
        include_str!("../../fixtures/tests/block_metadata_eof_anchor.adoc"),
        include_str!("../../fixtures/tests/block_metadata_eof_shorthand.adoc"),
        include_str!("../../fixtures/tests/block_metadata_eof_combined.adoc"),
        include_str!("../../fixtures/tests/block_metadata_eof_only_anchor.adoc"),
        include_str!("../../fixtures/tests/block_metadata_eof_title.adoc"),
        include_str!("../../fixtures/tests/block_metadata_eof_events.adoc"),
        include_str!("../../fixtures/tests/block_metadata_eof_contexts.adoc"),
    ] {
        let parsed = parse(source, &Options::default())?;
        assert_eq!(parsed.document().footnotes().count(), 0);
        assert!(
            parsed
                .document()
                .references()
                .iter()
                .all(|(id, _)| !id.starts_with("unused-"))
        );
    }
    Ok(())
}

#[test]
fn cross_reference_model_equality_ignores_parser_state() -> Result<(), Error> {
    let parsed = parse_inline("<<id>>", &Options::default())?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::CrossReference(actual))] = parsed_inlines.as_slice() else {
        return Err(format!(
            "expected one cross-reference, got {:?}",
            parsed_inlines.as_slice()
        )
        .into());
    };
    let expected = CrossReference::new(
        "id",
        acdc_parser::Location::from(
            acdc_parser::ast::CrossReferenceRef::from(actual.as_ref()).location(),
        ),
    );

    assert_eq!(actual.as_ref(), &expected);
    let mut differing_destination = expected.clone();
    differing_destination.set_target_is_local(false);
    assert_ne!(actual.as_ref(), &differing_destination);
    let mut differing_style = expected.clone();
    differing_style.set_xrefstyle(XrefStyle::Full);
    assert_ne!(actual.as_ref(), &differing_style);
    let mut differing_label = expected;
    differing_label.set_caption_label(XrefCaptionLabel::NumberOnly);
    assert_ne!(actual.as_ref(), &differing_label);
    Ok(())
}

#[test]
fn cross_reference_model_debug_ignores_parser_state() -> Result<(), Error> {
    let parsed = parse_inline("<<id>>", &Options::default())?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::CrossReference(actual))] = parsed_inlines.as_slice() else {
        return Err(format!(
            "expected one cross-reference, got {:?}",
            parsed_inlines.as_slice()
        )
        .into());
    };
    let expected = CrossReference::new(
        "id",
        acdc_parser::Location::from(
            acdc_parser::ast::CrossReferenceRef::from(actual.as_ref()).location(),
        ),
    );
    let actual_debug = format!("{actual:?}");

    assert_eq!(actual_debug, format!("{expected:?}"));
    for field in ["target", "text", "location", "xrefstyle", "caption_label"] {
        assert!(actual_debug.contains(field));
    }
    assert!(!actual_debug.contains("caption_label_snapshot_id"));
    assert!(!actual_debug.contains("resolve_natural_target"));
    assert!(!actual_debug.contains("source_syntax"));
    Ok(())
}

#[test]
fn cross_reference_model_serialization_ignores_parser_state() -> Result<(), Error> {
    let parsed = parse_inline("<<id>>", &Options::default())?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::CrossReference(actual))] = parsed_inlines.as_slice() else {
        return Err(format!(
            "expected one cross-reference, got {:?}",
            parsed_inlines.as_slice()
        )
        .into());
    };
    let expected = CrossReference::new(
        "id",
        acdc_parser::Location::from(
            acdc_parser::ast::CrossReferenceRef::from(actual.as_ref()).location(),
        ),
    );

    assert_eq!(
        serde_json::to_value(actual)?,
        serde_json::to_value(expected)?
    );
    Ok(())
}

#[test]
fn cross_reference_model_full_document_resolves_caption_label() -> Result<(), Error> {
    let parsed = parse(
        ":table-caption: ReferenceTable\n:xrefstyle: full\n\nSee <<table-target>>.\n\n:table-caption: TargetTable\n\n[[table-target]]\n.A table\n|===\n|Cell\n|===\n",
        &Options::default(),
    )?;
    let Some(xref) = parsed.document().blocks().find_map(|block| {
        let BlockKind::Paragraph(paragraph) = block.kind() else {
            return None;
        };
        paragraph.content().inlines().find_map(|inline| {
            let InlineKind::CrossReference(xref) = inline.kind() else {
                return None;
            };
            Some(xref)
        })
    }) else {
        return Err("expected a cross-reference in the full document".into());
    };

    assert_eq!(xref.target(), "table-target");
    assert_eq!(xref.xrefstyle(), XrefStyle::Full);
    assert_eq!(
        xref.caption_label(),
        XrefCaptionLabel::AtReference("ReferenceTable")
    );
    Ok(())
}

#[test]
fn reference_catalog_covers_rendered_header_title_credits_and_footnotes() -> Result<(), Error> {
    let parsed = parse(
        concat!(
            include_str!("../../fixtures/tests/document_title_anchor_reference.adoc"),
            "\n.Target [[title-anchor]]\nParagraph body.\n\n[quote, 'Author [[attribution-anchor]]', 'Work [[citation-anchor]]']\n____\nQuote body.\n____\n\nA note.footnote:[Footnote [[footnote-anchor]] body.]\n\nSee <<title-anchor>>, <<attribution-anchor>>, <<citation-anchor>>, and <<footnote-anchor>>.\n"
        ),
        &Options::default(),
    )?;
    let document = parsed.document();

    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    assert!(!document.references().contains("unused-header-anchor"));
    for id in [
        "document-header",
        "title-anchor",
        "attribution-anchor",
        "citation-anchor",
        "footnote-anchor",
    ] {
        assert!(document.references().contains(id), "missing reference {id}");
    }

    let title = document
        .references()
        .get("document-header")
        .and_then(ReferenceRef::title)
        .ok_or("missing document-title reference text")?;
    assert!(
        title
            .inlines()
            .any(|inline| matches!(inline.kind(), InlineKind::Plain(text) if text.text() == ": "))
    );
    Ok(())
}
