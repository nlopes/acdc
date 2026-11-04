use std::borrow::Cow;

use acdc_parser::{
    AttributeValue, CompatibilityMode, DocumentAttributeValue, DocumentAttributes, Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse,
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
        let attributes = parsed.document().attributes();
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
        let inputs: std::collections::HashMap<_, _> =
            DocumentAttributes::from(attributes).into_inputs().collect();
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
            DocumentAttributes::from(parsed.document().attributes()).into_static()
        };
        assert_eq!(
            snapshot.get("name").and_then(DocumentAttributeValue::text),
            Some("Early")
        );
        let options = options.with_document_attributes(snapshot);
        let parsed = parse(".Title {alias}\nBody.\n\n{alias}\n", &options)?;
        let [BlockKind::Paragraph(titled), BlockKind::Paragraph(body)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected two paragraphs".into());
        };
        for nodes in [titled.title().ok_or("missing title")?, body.content()] {
            assert!(nodes.inlines().any(|node| matches!(node.kind(), InlineKind::Bold(bold) if matches!(&*bold.content().inlines().map(InlineRef::kind).collect::<Vec<_>>(), [InlineKind::Raw(text)] if text.text() == "Early"))));
            assert!(nodes.inlines().any(|node| matches!(node.kind(), InlineKind::Raw(text) if text.text() == "<raw> &" && text.substitutions().next().is_none())));
        }
        assert!(parsed.document().footnotes().next().is_none());
    }
    Ok(())
}
