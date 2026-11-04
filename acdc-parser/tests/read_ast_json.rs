//! Native serialization and checked ASG export keep separate wire contracts.

use acdc_parser::{CompatibilityMode, Options, parse};
use serde_json::json;

type Error = Box<dyn std::error::Error>;

#[test]
fn typed_document_json_keeps_native_targets_and_content_presence() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (attributes, count) in [
            ("", None),
            ("alt=\"\",title=\"\"", Some(0)),
            ("alt='*Bird*',title='_Hover_'", Some(1)),
        ] {
            let source = format!("image:bird.svg[{attributes}]");
            let parsed = parse(&source, &options)?;
            let document = parsed.document_ref();
            let value = serde_json::to_value(document)?;
            let image = value.pointer("/blocks/0/inlines/0").ok_or("image")?;
            assert_eq!(image.pointer("/target/type"), Some(&json!("path")));
            assert_eq!(image.pointer("/target/value"), Some(&json!("bird.svg")));
            for name in ["alt", "tooltip"] {
                assert_eq!(
                    image
                        .get(name)
                        .and_then(serde_json::Value::as_array)
                        .map(Vec::len),
                    count
                );
            }
            if count == Some(1) {
                assert_eq!(image.pointer("/alt/0/variant"), Some(&json!("strong")));
                assert_eq!(
                    image.pointer("/tooltip/0/variant"),
                    Some(&json!("emphasis"))
                );
            }
            assert!(value.get("profile").is_none());
            assert!(value.get("$schema").is_none());
            assert_eq!(document.blocks().count(), 1);
            assert_eq!(serde_json::to_value(document)?, value);
        }
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn checked_export_does_not_change_native_serialization() -> Result<(), Error> {
    use acdc_parser::asg;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse("image:bird.svg[alt='*Bird*',title='_Hover_']", &options)?;
        let document = parsed.document_ref();
        let native = serde_json::to_string(&document)?;
        assert!(matches!(
            asg::document(document),
            Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/inlines/0"
        ));
        let exported = asg::document_with_profile(document, asg::Profile::Acdc)?;
        assert_eq!(exported.get("profile"), Some(&json!("urn:acdc:asg:1")));
        assert_eq!(
            exported.pointer("/blocks/0/inlines/0/target"),
            Some(&json!("bird.svg"))
        );
        assert_eq!(serde_json::to_string(&document)?, native);
        assert_eq!(document.blocks().count(), 1);
    }
    Ok(())
}
