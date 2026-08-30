use acdc_parser::{
    Block, CrossReference, InlineMacro, InlineNode, Options, XrefCaptionLabel, XrefStyle, parse,
    parse_inline,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn cross_reference_model_equality_ignores_parser_state() -> Result<(), Error> {
    let parsed = parse_inline("<<id>>", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::CrossReference(actual))] = parsed.inlines() else {
        return Err(format!("expected one cross-reference, got {:?}", parsed.inlines()).into());
    };
    let expected = CrossReference::new("id", actual.location.clone());

    assert_eq!(actual, &expected);
    let mut differing_style = expected.clone();
    differing_style.xrefstyle = XrefStyle::Full;
    assert_ne!(actual, &differing_style);
    let mut differing_label = expected;
    differing_label.caption_label = XrefCaptionLabel::NumberOnly;
    assert_ne!(actual, &differing_label);
    Ok(())
}

#[test]
fn cross_reference_model_debug_ignores_parser_state() -> Result<(), Error> {
    let parsed = parse_inline("<<id>>", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::CrossReference(actual))] = parsed.inlines() else {
        return Err(format!("expected one cross-reference, got {:?}", parsed.inlines()).into());
    };
    let expected = CrossReference::new("id", actual.location.clone());
    let actual_debug = format!("{actual:?}");

    assert_eq!(actual_debug, format!("{expected:?}"));
    for field in ["target", "text", "location", "xrefstyle", "caption_label"] {
        assert!(actual_debug.contains(field));
    }
    assert!(!actual_debug.contains("caption_label_snapshot_id"));
    assert!(!actual_debug.contains("resolve_natural_target"));
    Ok(())
}

#[test]
fn cross_reference_model_serialization_ignores_parser_state() -> Result<(), Error> {
    let parsed = parse_inline("<<id>>", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::CrossReference(actual))] = parsed.inlines() else {
        return Err(format!("expected one cross-reference, got {:?}", parsed.inlines()).into());
    };
    let expected = CrossReference::new("id", actual.location.clone());

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
    let Some(xref) = parsed.document().blocks.iter().find_map(|block| {
        let Block::Paragraph(paragraph) = block else {
            return None;
        };
        paragraph.content.iter().find_map(|inline| {
            let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                return None;
            };
            Some(xref)
        })
    }) else {
        return Err("expected a cross-reference in the full document".into());
    };

    assert_eq!(xref.target, "table-target");
    assert_eq!(xref.xrefstyle, XrefStyle::Full);
    assert_eq!(
        xref.caption_label,
        XrefCaptionLabel::AtReference("ReferenceTable")
    );
    Ok(())
}

#[test]
fn reference_catalog_covers_rendered_header_title_credits_and_footnotes() -> Result<(), Error> {
    let parsed = parse(
        concat!(
            include_str!("../fixtures/tests/document_title_anchor_reference.adoc"),
            "\n.Target [[title-anchor]]\nParagraph body.\n\n[quote, 'Author [[attribution-anchor]]', 'Work [[citation-anchor]]']\n____\nQuote body.\n____\n\nA note.footnote:[Footnote [[footnote-anchor]] body.]\n\nSee <<title-anchor>>, <<attribution-anchor>>, <<citation-anchor>>, and <<footnote-anchor>>.\n"
        ),
        &Options::default(),
    )?;
    let document = parsed.document();

    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    assert!(!document.references.contains_key("unused-header-anchor"));
    for id in [
        "document-header",
        "title-anchor",
        "attribution-anchor",
        "citation-anchor",
        "footnote-anchor",
    ] {
        assert!(
            document.references.contains_key(id),
            "missing reference {id}"
        );
    }

    let title = document
        .references
        .get("document-header")
        .and_then(|reference| reference.title.as_ref())
        .ok_or("missing document-title reference text")?;
    assert!(
        title
            .iter()
            .any(|inline| matches!(inline, InlineNode::PlainText(text) if text.content == ": "))
    );
    Ok(())
}
