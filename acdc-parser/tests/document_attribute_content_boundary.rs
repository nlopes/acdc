use std::borrow::Cow;

use acdc_parser::{
    AttributeValue, Block, CompatibilityMode, DocumentAttributeValue, InlineNode, Options, parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn document_value_inputs_preserve_text_presence_unsets_and_integer_spelling() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("max-include-depth", "0003")
            .build()?;
        let parsed = parse(
            "= T\n:toclevels: 0003\n:present:\n:empty: pass:[]\n:unset!:\n:formatted: pass:q[*Early*]\n:raw: pass:[<raw> &]\n:alias: {formatted} / {raw}\n\nBody.\n",
            &options,
        )?;
        let attributes = &parsed.document().attributes;
        for (name, expected) in [
            ("toclevels", "0003"),
            ("max-include-depth", "0003"),
            ("empty", ""),
            ("formatted", "*Early*"),
            ("raw", "<raw> &"),
            ("alias", "*Early* / <raw> &"),
        ] {
            let assignment = attributes.assignment(name).ok_or("missing assignment")?;
            assert!(
                matches!(
                    AttributeValue::from(assignment),
                    AttributeValue::String(Cow::Borrowed(text)) if text == expected
                ),
                "{mode:?}: {name}"
            );
        }
        assert_eq!(
            attributes
                .get("max-include-depth")
                .and_then(DocumentAttributeValue::as_integer),
            Some(3)
        );
        assert_eq!(
            AttributeValue::from(attributes.assignment("present").ok_or("missing presence")?),
            AttributeValue::Bool(true)
        );
        assert_eq!(
            AttributeValue::from(attributes.assignment("unset").ok_or("missing unset")?),
            AttributeValue::Bool(false)
        );
        let inputs: std::collections::HashMap<_, _> = attributes.clone().into_inputs().collect();
        assert_eq!(inputs.get("toclevels"), Some(&AttributeValue::from("0003")));
        assert_eq!(inputs.get("unset"), Some(&AttributeValue::None));
    }
    Ok(())
}

#[test]
fn parsed_snapshots_retain_prepared_fragments_and_definition_time_references() -> Result<(), Error>
{
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let snapshot = {
            let parsed = parse(
                "= T\n:name: Early\n:formatted: pass:a,q[*{name}*]\n:raw: pass:[<raw> &]\n:alias: {formatted} / {raw}\n\n:name: Late\n\nBody.\n",
                &options,
            )?;
            parsed.document().attributes.clone().into_static()
        };
        assert_eq!(
            snapshot.get("name").and_then(DocumentAttributeValue::text),
            Some("Early")
        );
        let options = options.with_document_attributes(snapshot);
        let parsed = parse(".Title {alias}\nBody.\n\n{alias}\n", &options)?;
        let [Block::Paragraph(titled), Block::Paragraph(body)] =
            parsed.document().blocks.as_slice()
        else {
            return Err("expected two paragraphs".into());
        };
        for nodes in [titled.title.as_ref(), body.content.as_slice()] {
            assert!(nodes.iter().any(|node| matches!(node, InlineNode::BoldText(bold) if matches!(bold.content.as_slice(), [InlineNode::RawText(text)] if text.content == "Early"))));
            assert!(nodes.iter().any(|node| matches!(node, InlineNode::RawText(text) if text.content == "<raw> &" && text.subs.is_empty())));
        }
        assert_eq!(parsed.document().footnotes, []);
    }
    Ok(())
}
