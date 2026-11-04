use crate::{
    CompatibilityMode, DocumentAttributeAssignment, DocumentAttributeValue, DocumentAttributes,
    Options,
    ast::{BlockKind, ContentRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    let mut result = String::new();
    for inline in content.inlines() {
        if let InlineKind::Plain(node) = inline.kind() {
            result.push_str(node.text());
        } else if let InlineKind::Bold(node) = inline.kind() {
            result.push_str(&text(node.content())?);
        } else if let InlineKind::Raw(node) = inline.kind() {
            result.push_str(node.text());
        } else {
            return Err("unexpected test content".into());
        }
    }
    Ok(result)
}

#[rstest::rstest]
fn cell_initial_assignments_are_ordered_and_stay_in_the_cell_scope(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= T\n:doctype: book\n:toc:\n:showtitle:\n\n[cols=a]\n|===\n|\n:local: cell\n\n{doctype}\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document_ref();
    let BlockKind::Table(table) = document.blocks().next().ok_or("table")?.kind() else {
        return Err("table".into());
    };
    let cell = table
        .rows()
        .next()
        .ok_or("row")?
        .cells()
        .next()
        .ok_or("cell")?;
    let mut doctype = document.attributes().get("doctype");
    let mut toc = document.attributes().get("toc");
    for (name, assignment) in cell.initial_attributes() {
        match name {
            "doctype" => doctype = assignment.value(),
            "toc" => toc = assignment.value(),
            _ => {}
        }
    }
    assert_eq!(
        doctype.and_then(DocumentAttributeValue::text),
        Some("article")
    );
    assert_eq!(toc, None);
    let mut local = None;
    for block in cell.blocks() {
        if let BlockKind::DocumentAttribute(event) = block.kind() {
            if event.name() == "local" {
                local = event.assignment().value();
            }
        } else if let BlockKind::Paragraph(paragraph) = block.kind() {
            assert_eq!(text(paragraph.content())?, "article");
        }
    }
    assert_eq!(local.and_then(DocumentAttributeValue::text), Some("cell"));
    assert_eq!(
        document
            .attributes()
            .get("doctype")
            .and_then(DocumentAttributeValue::text),
        Some("book")
    );
    assert!(document.attributes().contains("toc"));
    assert!(document.attributes().get("local").is_none());
    let owner_table = parsed.document().blocks.first().ok_or("owner table")?;
    let crate::Block::DelimitedBlock(block) = owner_table else {
        return Err("delimited table".into());
    };
    let crate::DelimitedBlockType::DelimitedTable(owner) = &block.inner else {
        return Err("table owner".into());
    };
    let expected = owner
        .rows
        .first()
        .ok_or("owner row")?
        .columns
        .first()
        .ok_or("owner cell")?
        .initial_attributes();
    assert!(cell.initial_attributes().eq(expected));
    Ok(())
}

#[rstest::rstest]
fn document_attribute_snapshots_preserve_values_provenance_and_input_policy(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("locked", "caller")
        .with_attribute("literal-at", "caller@")
        .with_attribute("max-include-depth", "064")
        .with_default_attribute("processor", "fallback")
        .build()?;
    let parsed = parse(":flag:\n:removed!:\n:figure-caption!:\n\nBody.\n", &options)?;
    let original = &parsed.document().attributes;
    let before = serde_json::to_value(original)?;
    let copied = DocumentAttributes::from(parsed.document_ref().attributes());
    assert_eq!(&copied, original);
    assert_eq!(serde_json::to_value(&copied)?, before);
    assert!(
        copied
            .get("flag")
            .is_some_and(DocumentAttributeValue::is_presence)
    );
    for name in ["removed", "figure-caption"] {
        assert!(matches!(
            copied.assignment(name),
            Some(DocumentAttributeAssignment::Unset)
        ));
        assert!(copied.is_explicit(name));
    }
    assert!(copied.assignment("missing").is_none());
    assert!(!copied.is_explicit("table-caption"));
    assert_eq!(
        copied
            .get("max-include-depth")
            .and_then(DocumentAttributeValue::as_integer),
        Some(64)
    );
    assert_eq!(
        copied
            .get("max-include-depth")
            .and_then(DocumentAttributeValue::text),
        Some("064")
    );
    let retained = copied.clone().into_static();
    assert_eq!(&retained, original);

    let input = ":locked: changed\n:literal-at: changed\n:processor: changed\n\nNext.\n";
    let replay = parse(input, &Options::default().with_document_attributes(copied))?;
    let expected = parse(
        input,
        &Options::default().with_document_attributes(original.clone()),
    )?;
    assert_eq!(replay.document().attributes, expected.document().attributes);
    let values = replay.document_ref().attributes();
    assert_eq!(
        values.get("locked").and_then(DocumentAttributeValue::text),
        Some("caller")
    );
    assert_eq!(
        values
            .get("literal-at")
            .and_then(DocumentAttributeValue::text),
        Some("caller@")
    );
    assert_eq!(
        values
            .get("processor")
            .and_then(DocumentAttributeValue::text),
        Some("changed")
    );
    assert_eq!(serde_json::to_value(original)?, before);
    Ok(())
}
