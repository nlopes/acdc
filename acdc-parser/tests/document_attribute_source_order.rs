use std::fmt::Write;

use acdc_parser::{
    AttributeValue, Block, DocumentAttributeAssignment, DocumentAttributeValue, Options, SafeMode,
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn document_attributes_are_the_header_snapshot() -> Result<(), Error> {
    let parsed = parse(
        "= Header\n:imagesdir: header\n\nimage::before.png[]\n\n:imagesdir: body\n\nimage::after.png[]\n",
        &Options::default(),
    )?;
    let document = parsed.document();

    assert_eq!(
        document
            .attributes
            .get("imagesdir")
            .and_then(|value| value.text()),
        Some("header")
    );
    let event = document
        .blocks
        .iter()
        .find_map(|block| {
            let Block::DocumentAttribute(attribute) = block else {
                return None;
            };
            Some(attribute)
        })
        .ok_or("expected a body attribute event")?;
    assert_eq!(event.name, "imagesdir");
    assert_eq!(
        *event.assignment(),
        DocumentAttributeAssignment::Set(DocumentAttributeValue::from("body"))
    );
    Ok(())
}

#[test]
fn untitled_document_attributes_are_the_header_snapshot() -> Result<(), Error> {
    let parsed = parse(
        ":hardbreaks-option:\n\nBefore.\n\n:hardbreaks-option!:\n\nAfter.\n",
        &Options::default(),
    )?;

    assert!(
        parsed
            .document()
            .attributes
            .contains_key("hardbreaks-option")
    );
    Ok(())
}

#[test]
fn metadata_adjacent_assignment_precedes_its_block() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/document_attributes_source_order.adoc"),
        &Options::default(),
    )?;
    let document = parsed.document();

    assert_eq!(
        document
            .attributes
            .get("metadata-value")
            .and_then(|value| value.text()),
        Some("header")
    );
    let event_index = document
        .blocks
        .iter()
        .position(|block| matches!(block, Block::DocumentAttribute(_)))
        .ok_or("expected a metadata-adjacent event")?;
    assert!(matches!(
        document.blocks.get(event_index + 1),
        Some(Block::Paragraph(_))
    ));
    let Some(Block::DocumentAttribute(event)) = document.blocks.get(event_index) else {
        return Err("expected a document attribute".into());
    };
    assert_eq!(event.name, "metadata-value");
    assert_eq!(
        event
            .assignment()
            .value()
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some("body")
    );
    Ok(())
}

#[test]
fn rejected_body_assignment_is_not_an_ast_event() -> Result<(), Error> {
    let parsed = parse(
        "= Header\n\n:safe-mode-name: forged\n\nParagraph.\n",
        &Options::default(),
    )?;

    assert!(
        parsed
            .document()
            .blocks
            .iter()
            .all(|block| !matches!(block, Block::DocumentAttribute(_)))
    );
    Ok(())
}

#[rstest::rstest]
fn assignments_follow_caller_precedence_in_source_order(
    #[values(
        "imagesdir",
        "hardbreaks-option",
        "source-language",
        "table-frame",
        "experimental",
        "sectnums",
        "safe-mode-name",
        "backend",
        "allow-uri-read"
    )]
    family: &str,
    #[values(false, true)] unset_header: bool,
    #[values("default", "locked-set", "locked-unset", "soft-set", "soft-unset")] caller: &str,
) -> Result<(), Error> {
    let builder = Options::builder().with_safe_mode(SafeMode::Safe);
    let options = match caller {
        "default" => builder,
        "locked-set" => builder.with_attribute(family, "caller-value"),
        "locked-unset" => builder.with_attribute(family, AttributeValue::None),
        "soft-set" => builder.with_default_attribute(family, "caller-value"),
        "soft-unset" => builder.with_default_attribute(family, false),
        _ => return Err(format!("unknown caller mode: {caller}").into()),
    }
    .with_default_attribute("backend", "html5")
    .build()?;
    let (header_value, body_value) = if family == "backend" {
        ("html5", "docbook5")
    } else {
        ("header-value", "body-value")
    };
    let header_entry = if unset_header {
        format!(":{family}!:")
    } else {
        format!(":{family}: {header_value}")
    };
    let mut source = format!(
        "= Policy\n{header_entry}\n\nHeader.\n\n:{family}: {body_value}\n\nSet.\n\n:!{family}:\n\nUnset.\n"
    );
    if !unset_header {
        writeln!(source, "\n:{family}: {header_value}\n\nReset.")?;
    }

    let header = match (family, caller) {
        ("backend", _) => Some("html5"),
        ("safe-mode-name", _) => Some("safe"),
        ("allow-uri-read", "locked-set" | "soft-set") | (_, "locked-set") => Some("caller-value"),
        ("allow-uri-read", _) | (_, "locked-unset") => None,
        _ if unset_header => None,
        _ => Some("header-value"),
    };
    let fixed = matches!(family, "backend" | "safe-mode-name" | "allow-uri-read")
        || caller == "locked-unset"
        || (caller == "locked-set" && family != "sectnums");
    let expected = if fixed {
        vec![header; if unset_header { 3 } else { 4 }]
    } else if unset_header {
        vec![header, Some("body-value"), None]
    } else {
        vec![header, Some("body-value"), None, Some("header-value")]
    };

    let parsed = parse(&source, &options)?;
    let document = parsed.document();
    let mut active = document.attributes.get(family);
    assert_eq!(active.and_then(DocumentAttributeValue::text), header);
    let mut observed = Vec::new();
    for block in &document.blocks {
        if let Block::DocumentAttribute(event) = block
            && event.name == family
        {
            active = event.assignment().value();
        } else if matches!(block, Block::Paragraph(_)) {
            observed.push(active.and_then(DocumentAttributeValue::text));
        }
    }
    assert_eq!(observed, expected);
    Ok(())
}
