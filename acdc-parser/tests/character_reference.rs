use acdc_parser::{
    CharacterReference, CharacterReferenceKind, InlineNode, Location,
    ast::{BlockKind, BlockRef, CharacterReferenceRef, InlineKind, InlineRef},
};
use serde_json::json;

type Error = Box<dyn std::error::Error + Send + Sync>;

fn is_macro(kind: InlineKind<'_>) -> bool {
    matches!(
        kind,
        InlineKind::Footnote(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
    )
}

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
            acdc_parser::asg::inlines(acdc_parser::ast::ContentRef::from(
                &acdc_parser::ContentAttribute::new(nodes.to_vec())
            ))?,
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
            matches!(acdc_parser::asg::inlines(acdc_parser::ast::ContentRef::from(&acdc_parser::ContentAttribute::new(nodes.to_vec()))), Err(acdc_parser::asg::Error::Unsupported { path, .. }) if path == "/0")
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
            let Some(BlockKind::Section(section)) =
                parsed.document().blocks().next().map(BlockRef::kind)
            else {
                return Err("missing section".into());
            };
            assert_eq!(section.resolve_id(), expected);
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
        let Some(BlockKind::Paragraph(paragraph)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("missing paragraph".into());
        };
        assert_eq!(
            paragraph
                .content()
                .inlines()
                .filter(|node| matches!(node.kind(), InlineKind::CharacterReference(_)))
                .count(),
            1
        );
        assert_eq!(paragraph.content().inlines().filter(|node| matches!(node.kind(), InlineKind::Raw(raw) if raw.text() == "&copy;" && raw.substitutions().collect::<Vec<_>>() == [acdc_parser::Substitution::SpecialChars])).count(), 2);
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

#[test]
fn intrinsic_references_have_semantic_identity_and_authored_ranges() -> Result<(), Error> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let options = acdc_parser::Options::builder()
            .with_compatibility_mode(mode)
            .build()?;
        for settings in [
            "",
            "[subs=attributes]\n",
            "[subs=\"specialchars,attributes\"]\n",
        ] {
            let source = format!("{settings}é {{quot}} {{apos}} {{plus}} {{asterisk}} {{cpp}}.");
            let parsed = acdc_parser::parse(&source, &options)?;
            let Some(BlockKind::Paragraph(paragraph)) =
                parsed.document().blocks().next().map(BlockRef::kind)
            else {
                return Err("missing paragraph".into());
            };
            let references: Vec<_> = paragraph
                .content()
                .inlines()
                .filter_map(|node| {
                    if let InlineKind::CharacterReference(reference) = node.kind() {
                        Some(reference)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(references.len(), 2, "{source} ({mode:?})");
            for (reference, (expected, token)) in
                references.into_iter().zip([(34, "{quot}"), (39, "{apos}")])
            {
                assert_eq!(reference.kind(), CharacterReferenceKind::Numeric(expected));
                assert_eq!(
                    &parsed.source()[reference.location().absolute_start()
                        ..=reference.location().absolute_end()],
                    token
                );
            }
            assert!(
                !paragraph
                    .content().inlines()
                    .any(|node| matches!(node.kind(), InlineKind::Bold(_)) || is_macro(node.kind()))
            );
        }
    }
    Ok(())
}

#[test]
fn intrinsic_references_keep_a_later_escape_stage() -> Result<(), Error> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let options = acdc_parser::Options::builder()
            .with_compatibility_mode(mode)
            .build()?;
        for substitutions in [
            "attributes,specialchars",
            "attributes,replacements,specialchars",
            "replacements,attributes,specialchars",
        ] {
            let source =
                format!("[subs=\"{substitutions}\"]\n{{quot}} {{apos}} {{lt}} {{gt}} {{amp}}");
            let parsed = acdc_parser::parse(&source, &options)?;
            let Some(BlockKind::Paragraph(paragraph)) =
                parsed.document().blocks().next().map(BlockRef::kind)
            else {
                return Err("missing paragraph".into());
            };
            assert!(
                !paragraph
                    .content()
                    .inlines()
                    .any(|node| matches!(node.kind(), InlineKind::CharacterReference(_))),
                "{source} ({mode:?})"
            );
            for value in ["&#34;", "&#39;", "<", ">", "&"] {
                assert!(paragraph.content().inlines().any(|node| matches!(node.kind(), InlineKind::Raw(raw) if raw.text() == value && raw.substitutions().collect::<Vec<_>>() == [acdc_parser::Substitution::SpecialChars])), "missing {value}: {source} ({mode:?})");
            }
        }
    }
    Ok(())
}

#[test]
fn resolved_reference_names_do_not_become_later_formatting() -> Result<(), Error> {
    let options = acdc_parser::Options::builder()
        .with_compatibility_mode(acdc_parser::CompatibilityMode::Draft)
        .build()?;
    for source in [
        "pass:c,r,q[Before &a__b__c; after]",
        "pass:c,r,q[*Before &a__b__c; after*]",
        "pass:c,r,q[_Before &a__b__c; after_]",
        "pass:c,r,q[*Before &a__b__c; +literal+ after*]",
        "pass:c,r,m[link:page.html[Before &a__b__c; after]]",
    ] {
        let parsed = acdc_parser::parse_inline(source, &options)?;
        let mut found = Vec::new();
        collect_reference_names(parsed.content().inlines(), &mut found);
        assert_eq!(found, ["&a__b__c;"], "{source}: {:?}", parsed.inlines());
    }
    Ok(())
}

fn collect_reference_names<'a>(
    nodes: impl Iterator<Item = InlineRef<'a>>,
    found: &mut Vec<String>,
) {
    for node in nodes {
        if let InlineKind::CharacterReference(reference) = node.kind() {
            found.push(reference.kind().to_string());
        }
        for field in node.content_fields() {
            collect_reference_names(field.content().inlines(), found);
        }
    }
}

#[test]
fn later_formatting_can_enclose_reference_marker_characters() -> Result<(), Error> {
    let options = acdc_parser::Options::builder()
        .with_compatibility_mode(acdc_parser::CompatibilityMode::Draft)
        .build()?;
    for (source, expected) in [
        ("pass:c,r,q[__Before &a__b__c; after__]", "&a__b__c;"),
        ("pass:c,r,q[#Before &#169; after#]", "&#169;"),
    ] {
        let parsed = acdc_parser::parse_inline(source, &options)?;
        assert!(
            matches!(
                parsed.inlines(),
                [InlineNode::ItalicText(_) | InlineNode::HighlightText(_)]
            ),
            "{source}: {:?}",
            parsed.inlines()
        );
        let mut found = Vec::new();
        collect_reference_names(parsed.content().inlines(), &mut found);
        assert_eq!(found, [expected]);
    }
    Ok(())
}
