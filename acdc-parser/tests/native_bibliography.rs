use acdc_parser::{Block, CompatibilityMode, InlineNode, Options, parse};
use serde_json::{Value, json};

type TestError = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("Before [[target,Label]] after.", false, false)]
#[case("Before anchor:target[Label] after.", false, false)]
#[case("[bibliography]\n* [[[target]]] Entry.", true, false)]
#[case("[bibliography]\n* [[[target,*café (C)*]]] Entry.", true, true)]
#[case(
    "[bibliography,subs=\"specialcharacters,quotes,macros\"]\n* [[[target,café (C)]]] Entry.",
    true,
    true
)]
fn native_anchor_fields_preserve_bibliography_kind_and_label_presence(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
    #[case] bibliography: bool,
    #[case] label_present: bool,
) -> Result<(), TestError> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document().blocks.first().ok_or("block")?;
    let content = if let Block::Paragraph(paragraph) = block {
        &paragraph.content
    } else if let Block::UnorderedList(list) = block {
        &list.items.first().ok_or("entry")?.principal
    } else {
        return Err("paragraph or bibliography".into());
    };
    let (node, anchor) = content
        .iter()
        .find_map(|node| {
            if let InlineNode::InlineAnchor(anchor) = node {
                Some((node, anchor))
            } else {
                None
            }
        })
        .ok_or("anchor")?;
    assert_eq!(anchor.is_bibliography(), bibliography);
    assert_eq!(anchor.bibliography_label().is_some(), label_present);
    let direct = serde_json::to_value(anchor)?;
    let mut enclosing = serde_json::to_value(node)?;
    let fields = enclosing.as_object_mut().ok_or("anchor object")?;
    assert_eq!(fields.remove("name"), Some(json!("anchor")));
    assert_eq!(fields.remove("type"), Some(json!("inline")));
    assert_eq!(direct, enclosing);
    assert_eq!(
        direct.get("bibliography"),
        bibliography.then_some(&json!(true))
    );
    assert_eq!(
        direct.get("bibliography_label"),
        anchor
            .bibliography_label()
            .map(serde_json::to_value)
            .transpose()?
            .as_ref()
    );
    assert!(direct.get("source").is_none());
    assert!(direct.get("reftext_source").is_none());
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn bibliography_display_and_citation_keep_distinct_profiles_in_json(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let parsed = parse(
        "[bibliography,subs=\"specialcharacters,quotes,macros\"]\n* [[[target,café (C)]]] Entry.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::UnorderedList(list)) = parsed.document().blocks.first() else {
        return Err("bibliography".into());
    };
    let anchor = list
        .items
        .first()
        .ok_or("entry")?
        .principal
        .iter()
        .find_map(|node| {
            if let InlineNode::InlineAnchor(anchor) = node {
                Some(anchor)
            } else {
                None
            }
        })
        .ok_or("anchor")?;
    let value = serde_json::to_value(anchor)?;
    let citation = value
        .get("xreflabel")
        .and_then(Value::as_array)
        .ok_or("citation")?;
    let label = value
        .get("bibliography_label")
        .and_then(Value::as_array)
        .ok_or("display label")?;
    assert!(
        citation
            .iter()
            .any(|node| node.get("value") == Some(&json!("©")))
    );
    assert!(
        label
            .iter()
            .any(|node| node.get("value") == Some(&json!("café (C)")))
    );
    assert_ne!(citation, label);
    Ok(())
}
