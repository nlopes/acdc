#![cfg(feature = "asg")]

use std::{fs, path::Path};

use acdc_parser::{
    Block, CompatibilityMode, Document, InlineNode, Location, Options, Paragraph, Plain,
    Substitution, SubstitutionOp, SubstitutionSpec, asg, parse, parse_file,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn without_provenance(mut value: Value) -> Value {
    match &mut value {
        Value::Array(values) => {
            for value in values {
                *value = without_provenance(value.take());
            }
        }
        Value::Object(values) => {
            values.remove("location");
            for value in values.values_mut() {
                *value = without_provenance(value.take());
            }
            if values
                .get("metadata")
                .and_then(Value::as_object)
                .is_some_and(serde_json::Map::is_empty)
            {
                values.remove("metadata");
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    value
}

#[test]
fn substitution_effects_keep_body_escaping_and_independent_content_boundaries() -> Result<(), Error>
{
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/substitutions.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse_file(&source, &options)?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        for (path, name) in [
            ("/blocks/0/inlines/0/name", "raw"),
            ("/blocks/1/inlines/0/inlines/0/name", "raw"),
            ("/blocks/2/inlines/1/name", "text"),
            ("/blocks/3/inlines/1/name", "raw"),
            ("/blocks/4/inlines/0/name", "text"),
            ("/blocks/5/inlines/0/inlines/0/name", "raw"),
            ("/blocks/5/inlines/0/tooltip/0/name", "text"),
            ("/blocks/5/inlines/2/alt/0/name", "text"),
            ("/blocks/6/inlines/1/name", "text"),
            ("/blocks/6/inlines/3/name", "raw"),
            ("/blocks/7/title/0/name", "text"),
            ("/blocks/7/inlines/0/inlines/0/name", "raw"),
            ("/blocks/8/inlines/0/inlines/0/name", "text"),
            ("/blocks/9/inlines/0/name", "raw"),
        ] {
            assert_eq!(value.pointer(path), Some(&json!(name)), "{path} {mode:?}");
        }
        for block in value
            .get("blocks")
            .and_then(Value::as_array)
            .ok_or("blocks")?
        {
            assert!(block.pointer("/metadata/substitutions").is_none());
        }
    }
    Ok(())
}

#[test]
fn resolved_settings_match_equivalent_prepared_text_and_raw_nodes() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (settings, equivalent) in [
            (
                ":hardbreaks-option:\n\n[subs=none]\n<em>one</em>\ntwo",
                ":hardbreaks-option:\n\n+++<em>one</em>\ntwo+++",
            ),
            ("[subs=normal]\n*Bold* &copy;", "*Bold* &copy;"),
            (
                "[subs=none]\n*Literal* <i>raw</i>",
                "+++*Literal* <i>raw</i>+++",
            ),
            (
                "[subs=-quotes]\n*Literal* <i>text</i>",
                "++*Literal* <i>text</i>++",
            ),
        ] {
            let parsed = parse(settings, &options)?;
            let prepared = parse(equivalent, &options)?;
            for profile in [asg::Profile::Upstream, asg::Profile::Acdc] {
                assert_eq!(
                    without_provenance(asg::document_with_profile(parsed.document(), profile)?),
                    without_provenance(asg::document_with_profile(prepared.document(), profile)?),
                    "{mode:?} {profile:?} {settings}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn unresolved_substitution_instructions_are_not_silently_omitted() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse("[subs=unknown]\n*source*", &options)?;
        for profile in [asg::Profile::Upstream, asg::Profile::Acdc] {
            assert!(
                matches!(asg::document_with_profile(parsed.document(), profile), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/metadata/substitutions")
            );
        }
    }
    Ok(())
}

#[test]
fn typed_and_source_settings_share_the_same_output_semantics() -> Result<(), Error> {
    for spec in [
        SubstitutionSpec::Explicit(Vec::new()),
        SubstitutionSpec::Modifiers(vec![SubstitutionOp::Remove(Substitution::SpecialChars)]),
        SubstitutionSpec::Source(vec!["none".into()]),
    ] {
        let mut paragraph = Paragraph::new(
            vec![InlineNode::PlainText(Plain::new(
                "*prepared* <em>raw</em>",
                Location::default(),
            ))],
            Location::default(),
        );
        let mut metadata = paragraph.take_metadata();
        metadata.substitutions = Some(spec);
        paragraph.set_metadata(metadata)?;
        let mut document = Document::default();
        document.set_blocks(vec![Block::Paragraph(paragraph)]);
        let value = asg::document((&document).into())?;
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/name"),
            Some(&json!("raw"))
        );
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/value"),
            Some(&json!("*prepared* <em>raw</em>"))
        );
    }
    let parsed = parse("[subs=none]\n....\n<em>raw</em>\n....", &Options::default())?;
    let value = asg::document(parsed.document())?;
    assert_eq!(value.pointer("/blocks/0/name"), Some(&json!("literal")));
    assert_eq!(value.pointer("/blocks/0/form"), Some(&json!("delimited")));
    assert_eq!(value.pointer("/blocks/0/delimiter"), Some(&json!("....")));
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/name"),
        Some(&json!("raw"))
    );
    Ok(())
}
