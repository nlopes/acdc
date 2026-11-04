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

#[test]
fn parsed_character_references_have_authoritative_identity() -> Result<(), Error> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let options = acdc_parser::Options::builder()
            .with_compatibility_mode(mode)
            .build()?;
        let parsed = acdc_parser::parse_inline("&copy; &#169; &#x1F600;", &options)?;
        let kinds: Vec<_> = parsed
            .inlines()
            .iter()
            .filter_map(|node| {
                if let InlineNode::CharacterReference(reference) = node {
                    Some(reference.kind())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            kinds,
            [
                CharacterReferenceKind::Named("copy"),
                CharacterReferenceKind::Numeric(169),
                CharacterReferenceKind::Numeric(0x1f600)
            ]
        );
    }
    Ok(())
}

#[test]
fn parsed_character_reference_escapes_consume_only_one_backslash() -> Result<(), Error> {
    let parsed = acdc_parser::parse_inline(r"\\&copy;", &acdc_parser::Options::default())?;
    let value = serde_json::to_value(parsed.inlines())?;
    assert!(
        value.as_array().is_some_and(|nodes| nodes
            .iter()
            .any(|node| node["name"] == "raw" && node["value"] == "&copy;")),
        "{value}"
    );
    assert!(
        value
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["value"] == r"\")),
        "{value}"
    );
    Ok(())
}

#[test]
fn generated_section_ids_omit_character_references() -> Result<(), Error> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let options = acdc_parser::Options::builder()
            .with_compatibility_mode(mode)
            .build()?;
        for (source, expected) in [
            ("== Heading &copy;", "_heading"),
            ("== Heading &#65;", "_heading"),
            (r"== Heading \&copy;", "_heading_copy"),
        ] {
            let parsed = acdc_parser::parse(source, &options)?;
            let Some(acdc_parser::Block::Section(section)) = parsed.document().blocks.first()
            else {
                return Err("missing section".into());
            };
            assert_eq!(section.id(), expected);
        }
    }
    Ok(())
}

#[test]
fn passthrough_character_references_respect_later_escaping() -> Result<(), Error> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let options = acdc_parser::Options::builder()
            .with_compatibility_mode(mode)
            .build()?;
        for (source, expected) in [
            ("pass:r[&amp;copy;]", 1),
            ("pass:c,r[&copy; &amp;copy;]", 1),
            ("pass:r,c[&amp;copy;]", 0),
            ("pass:[&copy;]", 0),
            ("pass:c[&copy;]", 0),
        ] {
            let parsed = acdc_parser::parse_inline(source, &options)?;
            let count = parsed
                .inlines()
                .iter()
                .filter(|node| matches!(node, InlineNode::CharacterReference(_)))
                .count();
            assert_eq!(count, expected, "{source} ({mode:?})");
        }
    }
    Ok(())
}

#[test]
fn document_and_caller_character_references_expand_once() -> Result<(), Error> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let options = acdc_parser::Options::builder()
            .with_compatibility_mode(mode)
            .build()?;
        let source = ":reference: &copy;\n:encoded: &amp;copy;\n:escaped: \\&copy;\n\n{reference} {encoded} {escaped}";
        let parsed = acdc_parser::parse(source, &options)?;
        let Some(acdc_parser::Block::Paragraph(paragraph)) = parsed.document().blocks.first()
        else {
            return Err("missing paragraph".into());
        };
        assert_eq!(
            paragraph
                .content
                .iter()
                .filter(|node| matches!(node, InlineNode::CharacterReference(_)))
                .count(),
            1
        );
        assert_eq!(paragraph.content.iter().filter(|node| matches!(node, InlineNode::RawText(raw) if raw.content == "&copy;" && raw.subs == [acdc_parser::Substitution::SpecialChars])).count(), 2);
        let options = acdc_parser::Options::builder()
            .with_compatibility_mode(mode)
            .with_attributes([("encoded", "&amp;copy;")])
            .build()?;
        let parsed = acdc_parser::parse_inline("{encoded}", &options)?;
        assert!(
            matches!(parsed.inlines(), [InlineNode::CharacterReference(reference)] if reference.kind() == CharacterReferenceKind::Named("copy"))
        );
    }
    Ok(())
}

#[test]
fn later_quotes_span_resolved_character_references() -> Result<(), Error> {
    let parsed =
        acdc_parser::parse_inline("pass:c,r,q[*&#169;*]", &acdc_parser::Options::default())?;
    assert!(
        matches!(parsed.inlines(), [InlineNode::BoldText(bold)] if matches!(bold.content.as_slice(), [InlineNode::CharacterReference(reference)] if reference.kind() == CharacterReferenceKind::Numeric(169)))
    );
    Ok(())
}
