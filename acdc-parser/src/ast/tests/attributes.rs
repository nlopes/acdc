use crate::{
    Block, CompatibilityMode, Options,
    ast::{AttributeValueRef, AttributesRef, BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn assert_inventory(attributes: AttributesRef<'_>) {
    let entries = attributes.iter().collect::<Vec<_>>();
    for (index, (name, value)) in entries.iter().enumerate() {
        assert!(attributes.contains(name));
        assert_eq!(attributes.get(name), Some(*value), "{name}");
        assert!(
            !entries
                .iter()
                .take(index)
                .any(|(previous, _)| previous == name)
        );
        assert!(!matches!(*name, "roles" | "options"));
    }
    assert_eq!(attributes.is_empty(), entries.is_empty());
    for (alias, canonical) in [("roles", "role"), ("options", "opts")] {
        assert_eq!(attributes.get(alias), attributes.get(canonical));
        assert_eq!(attributes.contains(alias), attributes.contains(canonical));
    }
    assert_eq!(attributes.get("missing"), None);
    assert!(!attributes.contains("missing"));
}

#[rstest::rstest]
fn parser_tokens_and_identity_follow_existing_mode_semantics(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[anchor]]\n[role=first,opts=first]\n[role=second,options=second]\n.Title footnote:[Title note.]\nParagraph.\n\n\
         [#image,role=first,opts=first]\nimage::bird.png[Bird,role=second,opts=second]\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let warnings = format!("{:?}", parsed.warnings());
    let footnotes = parsed.document().footnotes.len();
    let references = parsed.document().references.len();
    assert_eq!(footnotes, 1);
    assert!(references > 0);
    let mut blocks = parsed.document_ref().blocks();
    let Some(BlockKind::Paragraph(paragraph)) = blocks.next().map(BlockRef::kind) else {
        return Err("missing paragraph".into());
    };
    let Some(Block::Paragraph(owner)) = parsed.document().blocks.first() else {
        return Err("missing paragraph owner".into());
    };
    assert_eq!(
        paragraph.roles().iter().collect::<Vec<_>>(),
        owner.metadata.roles
    );
    assert_eq!(
        paragraph.options().iter().collect::<Vec<_>>(),
        owner.metadata.options
    );
    assert_eq!(
        paragraph.attributes().get("id"),
        Some(AttributeValueRef::Text("anchor"))
    );
    assert_eq!(
        paragraph.attributes().get("style"),
        paragraph.style().map(AttributeValueRef::Text)
    );
    assert_inventory(paragraph.attributes());
    let Some(BlockKind::Image(image)) = blocks.next().map(BlockRef::kind) else {
        return Err("missing image".into());
    };
    let Some(Block::Image(owner)) = parsed.document().blocks.get(1) else {
        return Err("missing image owner".into());
    };
    assert_eq!(
        image.roles().iter().collect::<Vec<_>>(),
        owner.metadata.roles
    );
    assert_eq!(
        image.options().iter().collect::<Vec<_>>(),
        owner.metadata.options
    );
    assert_inventory(image.attributes());
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    assert_eq!(format!("{:?}", parsed.warnings()), warnings);
    assert_eq!(parsed.document().footnotes.len(), footnotes);
    assert_eq!(parsed.document().references.len(), references);
    Ok(())
}
