use acdc_parser::{
    CharacterReference, CharacterReferenceKind, InlineNode, Location,
    ast::{CharacterReferenceRef, InlineKind, InlineRef},
};
use serde_json::json;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn character_reference_keeps_identity_in_typed_reads_and_native_json() -> Result<(), Error> {
    for (kind, value) in [
        (
            CharacterReferenceKind::Named("NotEqualTilde"),
            "&NotEqualTilde;",
        ),
        (CharacterReferenceKind::Named("unknown"), "&unknown;"),
        (CharacterReferenceKind::Numeric(0x1f600), "&#128512;"),
        (CharacterReferenceKind::Numeric(0xd800), "&#55296;"),
    ] {
        let reference =
            CharacterReference::new(kind, Location::default()).ok_or("invalid reference")?;
        let handle = CharacterReferenceRef::from(&reference);
        assert_eq!(handle.kind(), kind);
        let node = InlineNode::CharacterReference(reference);
        let InlineKind::CharacterReference(handle) = InlineRef::from(&node).kind() else {
            return Err("wrong inline kind".into());
        };
        assert_eq!(handle.kind(), kind);
        assert_eq!(
            serde_json::to_value(&node)?,
            json!({
                "name": "charref", "type": "string", "value": value,
                "location": Location::default()
            })
        );
    }
    Ok(())
}

#[test]
fn character_reference_names_follow_xml_name_syntax() {
    for name in [
        "copy",
        "a",
        "a123",
        "_name",
        "prefix:name",
        "éclair",
        "a\u{301}",
        "\u{10000}",
    ] {
        assert!(
            CharacterReference::new(CharacterReferenceKind::Named(name), Location::default())
                .is_some(),
            "{name}"
        );
    }
    for name in [
        "",
        "2name",
        "&copy;",
        "a b",
        "a;",
        "a<",
        "a\0",
        "\u{300}",
        "\u{f0000}",
    ] {
        assert!(
            CharacterReference::new(CharacterReferenceKind::Named(name), Location::default())
                .is_none(),
            "{name:?}"
        );
    }
}

#[cfg(feature = "asg")]
#[test]
fn asg_character_references_are_checked_values() -> Result<(), Error> {
    for (kind, expected) in [
        (CharacterReferenceKind::Named("copy"), "&copy;"),
        (CharacterReferenceKind::Named("unknown"), "&unknown;"),
        (CharacterReferenceKind::Numeric(0x1f600), "&#128512;"),
    ] {
        let nodes = [InlineNode::CharacterReference(
            CharacterReference::new(kind, Location::default()).ok_or("invalid reference")?,
        )];
        assert_eq!(
            acdc_parser::asg::inlines(&nodes)?,
            json!([{
                "name": "charref", "type": "string", "value": expected
            }])
        );
    }
    for value in [0, 1, 0xd800, 0xfffe, 0x0011_0000, u32::MAX] {
        let nodes = [InlineNode::CharacterReference(
            CharacterReference::new(CharacterReferenceKind::Numeric(value), Location::default())
                .ok_or("invalid reference")?,
        )];
        assert!(
            matches!(acdc_parser::asg::inlines(&nodes), Err(acdc_parser::asg::Error::Unsupported { path, .. }) if path == "/0")
        );
    }
    Ok(())
}
