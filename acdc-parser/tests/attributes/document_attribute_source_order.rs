use std::fmt::Write;

use acdc_parser::{
    AttributeValue, CompatibilityMode, DocumentAttributeAssignment, DocumentAttributeValue,
    Options, SafeMode,
    ast::{BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("{signed}", "+1", "2")]
#[case("{negative}", "-1", "0")]
#[case("+{number}", "1", "2")]
#[case("-{number}", "1", "0")]
#[case("pass:a[{signed}]", "+1", "2")]
fn leveloffset_references_follow_the_selected_mode(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] body: bool,
    #[case] value: &str,
    #[case] asciidoctor: &str,
    #[case] draft: &str,
) -> Result<(), Error> {
    let boundary = if body { "\nBody.\n\n" } else { "" };
    let source = format!(
        "= Document\n:number: 1\n:signed: +1\n:negative: -1\n:leveloffset: 1\n{boundary}:leveloffset: {value}\n\n== Section"
    );
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse(&source, &options)?;
    let document = parsed.document();
    let actual = if body {
        document.blocks().find_map(|block| {
            let BlockKind::DocumentAttribute(event) = block.kind() else {
                return None;
            };
            (event.name() == "leveloffset")
                .then(|| {
                    event
                        .assignment()
                        .value()
                        .and_then(DocumentAttributeValue::text)
                })
                .flatten()
        })
    } else {
        document
            .attributes()
            .get("leveloffset")
            .and_then(DocumentAttributeValue::text)
    };
    let expected = if mode == CompatibilityMode::Asciidoctor {
        asciidoctor
    } else {
        draft
    };
    assert_eq!(actual, Some(expected));
    Ok(())
}

#[test]
fn relative_leveloffset_events_contain_the_resolved_value() -> Result<(), Error> {
    let parsed = parse(
        "= Header\n:leveloffset: 1\n\nBody.\n\n:leveloffset: +1\n\nNext.\n\n:leveloffset: -1\n\nLast.",
        &Options::default(),
    )?;
    assert_eq!(
        parsed
            .document()
            .attributes()
            .get("leveloffset")
            .and_then(DocumentAttributeValue::text),
        Some("1")
    );
    let values = parsed
        .document()
        .blocks()
        .filter_map(|block| {
            let BlockKind::DocumentAttribute(event) = block.kind() else {
                return None;
            };
            event
                .assignment()
                .value()
                .and_then(DocumentAttributeValue::text)
        })
        .collect::<Vec<_>>();
    assert_eq!(values, ["2", "1"]);
    Ok(())
}

#[test]
fn document_attributes_are_the_header_snapshot() -> Result<(), Error> {
    let parsed = parse(
        "= Header\n:imagesdir: header\n\nimage::before.png[]\n\n:imagesdir: body\n\nimage::after.png[]\n",
        &Options::default(),
    )?;
    let document = parsed.document();

    assert_eq!(
        document
            .attributes()
            .get("imagesdir")
            .and_then(|value| value.text()),
        Some("header")
    );
    let event = document
        .blocks()
        .find_map(|block| {
            let BlockKind::DocumentAttribute(attribute) = block.kind() else {
                return None;
            };
            Some(attribute)
        })
        .ok_or("expected a body attribute event")?;
    assert_eq!(event.name(), "imagesdir");
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

    assert!(parsed.document().attributes().contains("hardbreaks-option"));
    Ok(())
}

#[test]
fn metadata_adjacent_assignment_precedes_its_block() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../../fixtures/tests/document_attributes_source_order.adoc"),
        &Options::default(),
    )?;
    let document = parsed.document();

    assert_eq!(
        document
            .attributes()
            .get("metadata-value")
            .and_then(|value| value.text()),
        Some("header")
    );
    let event_index = document
        .blocks()
        .position(|block| matches!(block.kind(), BlockKind::DocumentAttribute(_)))
        .ok_or("expected a metadata-adjacent event")?;
    assert!(matches!(
        document.blocks().nth(event_index + 1).map(BlockRef::kind),
        Some(BlockKind::Paragraph(_))
    ));
    let Some(BlockKind::DocumentAttribute(event)) =
        document.blocks().nth(event_index).map(BlockRef::kind)
    else {
        return Err("expected a document attribute".into());
    };
    assert_eq!(event.name(), "metadata-value");
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
            .blocks()
            .all(|block| !matches!(block.kind(), BlockKind::DocumentAttribute(_)))
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
    let mut active = document.attributes().get(family);
    assert_eq!(active.and_then(DocumentAttributeValue::text), header);
    let mut observed = Vec::new();
    for block in document.blocks() {
        if let BlockKind::DocumentAttribute(event) = block.kind()
            && event.name() == family
        {
            active = event.assignment().value();
        } else if matches!(block.kind(), BlockKind::Paragraph(_)) {
            observed.push(active.and_then(DocumentAttributeValue::text));
        }
    }
    assert_eq!(observed, expected);
    Ok(())
}
