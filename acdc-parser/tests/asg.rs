#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, ContentAttribute, Document, Header, Location, Options,
    OptionsBuilder, Paragraph, Title, asg, ast::ContentRef, parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn modes() -> Result<[Options<'static>; 2], Error> {
    Ok([
        OptionsBuilder::default()
            .with_compatibility_mode(CompatibilityMode::Asciidoctor)
            .build()?,
        OptionsBuilder::default()
            .with_compatibility_mode(CompatibilityMode::Draft)
            .build()?,
    ])
}

#[test]
fn asg_preserves_present_empty_header_titles() -> Result<(), Error> {
    let parsed = parse("Text.", &Options::default())?;
    let mut document = Document::default();
    document.set_location(Location::from(parsed.document().location()));
    document.set_header(Some(Header::new(
        Title::new(Vec::new()),
        Location::from(acdc_parser::ast::DocumentRef::from(&document).location()),
    )));
    let value = asg::document((&document).into())?;
    assert_eq!(value.pointer("/header/title"), Some(&json!([])));
    Ok(())
}

#[test]
fn asg_preserves_explicit_unsets_and_numeric_spelling() -> Result<(), Error> {
    for options in modes()? {
        let parsed = parse(
            "= Title\n:flag!:\n:toclevels: 03\n:text: 001\n\nText.\n",
            &options,
        )?;
        let value = asg::document(parsed.document())?;
        assert_eq!(value.pointer("/attributes/flag"), Some(&Value::Null));
        assert_eq!(value.pointer("/attributes/toclevels"), Some(&json!("03")));
        assert_eq!(value.pointer("/attributes/text"), Some(&json!("001")));
    }
    Ok(())
}

#[test]
fn asg_preserves_literal_text_raw_text_and_resolved_typography() -> Result<(), Error> {
    let parsed = parse(
        "Text +<literal>+ and +++<b>raw</b>+++ ... (C).",
        &Options::default(),
    )?;
    let value = asg::document(parsed.document())?;
    let content = value
        .pointer("/blocks/0/inlines")
        .and_then(Value::as_array)
        .ok_or("missing inlines")?;
    assert!(
        content
            .iter()
            .any(|node| node.get("name") == Some(&json!("raw"))
                && node.get("value") == Some(&json!("<b>raw</b>")))
    );
    assert!(
        content
            .iter()
            .any(|node| node.get("name") == Some(&json!("text"))
                && node.get("value") == Some(&json!("<literal>")))
    );
    assert!(
        content
            .iter()
            .any(|node| node.get("value") == Some(&json!("…")))
    );
    assert!(
        content
            .iter()
            .any(|node| node.get("value") == Some(&json!("©")))
    );
    Ok(())
}

#[test]
fn asg_rejects_unsupported_meaning_at_any_depth() -> Result<(), Error> {
    for options in modes()? {
        for (source, prefix) in [
            ("Text image:x.png[Alt].", "/blocks/0/inlines/"),
            ("[title='image:x.png[Alt]']\nText.", "/blocks/0/title/"),
            (
                "== Heading\n\nText image:x.png[Alt].",
                "/blocks/0/blocks/0/inlines/",
            ),
            ("Text footnote:[Note].", "/footnotes"),
            ("Text.\n\n:later: value\n\nMore.", "/blocks/1"),
            ("|===\n|Cell\n|===", "/blocks/0"),
            (
                "[subs=unknown]\n*Literal*",
                "/blocks/0/metadata/substitutions",
            ),
            ("[.role]*Bold*", "/blocks/0/inlines/0/role"),
            ("[#inline]*Bold*", "/blocks/0/inlines/0/id"),
        ] {
            let parsed = parse(source, &options)?;
            let before = serde_json::to_value(parsed.document())?;
            let Err(error) = asg::document(parsed.document()) else {
                return Err(format!("unsupported source was accepted: {source}").into());
            };
            let asg::Error::Unsupported { path, .. } = error else {
                return Err(format!("wrong error for {source}: {error}").into());
            };
            assert!(path.starts_with(prefix), "{source}: {path}");
            assert_eq!(serde_json::to_value(parsed.document())?, before);
        }
    }
    Ok(())
}

#[test]
fn asg_does_not_run_pending_passthrough_operations() {
    let nodes = [acdc_parser::InlineNode::RawText(acdc_parser::Raw::new(
        "{missing}",
        vec![acdc_parser::Substitution::Attributes],
        Location::default(),
    ))];
    assert!(
        matches!(asg::inlines(ContentRef::from(&ContentAttribute::new(nodes.to_vec()))), Err(asg::Error::Unsupported { path, .. }) if path == "/0/substitutions")
    );
}

#[test]
fn asg_omits_unknown_optional_locations_and_rejects_partial_ones() -> Result<(), Error> {
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        Vec::new(),
        Location::default(),
    )));
    let value = asg::document((&document).into())?;
    assert!(value.get("location").is_none());
    assert!(value.pointer("/blocks/0/location").is_none());
    let mut blocks: Vec<_> = document.take_blocks().collect();
    let Block::Paragraph(paragraph) = blocks.first_mut().ok_or("missing paragraph")? else {
        return Err("wrong block".into());
    };
    let mut location =
        acdc_parser::Location::from(acdc_parser::ast::ParagraphRef::from(&*paragraph).location());
    location.start.line = 1;
    paragraph.set_location(location);
    document.set_blocks(blocks);
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::InvalidLocation { path, .. }) if path == "/blocks/0/location/1")
    );
    Ok(())
}

#[test]
fn asg_preserves_endpoint_include_chains_and_unicode_columns() -> Result<(), Error> {
    let mut source = Location::default();
    source.start.line = 2;
    source.start.column = 3;
    source.end.line = 7;
    source.end.column = 9;
    source.start.file = Some(vec!["outer.adoc".into(), "α.adoc".into()].into());
    source.end.file = Some(vec!["end.adoc".into()].into());
    let nodes = [acdc_parser::InlineNode::PlainText(
        acdc_parser::Plain::new("α", source.clone()).with_escaped(true),
    )];
    let value = asg::inlines(ContentRef::from(&ContentAttribute::new(nodes.to_vec())))?;
    assert_eq!(
        value.pointer("/0/location/0/file"),
        Some(&json!(["outer.adoc", "α.adoc"]))
    );
    assert_eq!(
        value.pointer("/0/location/1/file"),
        Some(&json!(["end.adoc"]))
    );
    assert_eq!(value.pointer("/0/location/0/col"), Some(&json!(3)));
    source.start.file = Some(Vec::new().into());
    let nodes = [acdc_parser::InlineNode::PlainText(
        acdc_parser::Plain::new("α", source).with_escaped(true),
    )];
    assert!(matches!(
        asg::inlines(ContentRef::from(&ContentAttribute::new(nodes.to_vec()))),
        Err(asg::Error::InvalidLocation { .. })
    ));
    Ok(())
}

#[test]
fn asg_rejects_attribute_protection_and_preserves_literal_quotes() -> Result<(), Error> {
    let parsed = parse(
        "= Title\n:protected: pass:q[*bold*]\n\nText.",
        &Options::default(),
    )?;
    assert!(
        matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported { path, .. }) if path == "/attributes/protected")
    );
    let parsed = parse(
        "= Title\n:quoted: \"literal\"\n:enabled:\n\nText.",
        &Options::default(),
    )?;
    let value = asg::document(parsed.document())?;
    assert_eq!(
        value.pointer("/attributes/quoted"),
        Some(&json!("\"literal\""))
    );
    assert_eq!(value.pointer("/attributes/enabled"), Some(&json!("")));
    Ok(())
}

#[test]
fn asg_reads_classified_references_and_literal_ampersands() -> Result<(), Error> {
    for options in modes()? {
        for (source, name, value) in [
            ("&copy;", "charref", "&copy;"),
            ("&#169;", "charref", "&#169;"),
            ("Fish & chips.", "text", "Fish & chips."),
            (r"\&copy;", "text", "&copy;"),
        ] {
            let parsed = parse(source, &options)?;
            let exported = asg::document(parsed.document())?;
            assert_eq!(
                exported.pointer("/blocks/0/inlines/0/name"),
                Some(&json!(name))
            );
            assert_eq!(
                exported.pointer("/blocks/0/inlines/0/value"),
                Some(&json!(value))
            );
        }
    }
    let parsed = parse("[custom,alpha]\nText.", &Options::default())?;
    assert!(matches!(
        asg::document(parsed.document()),
        Err(asg::Error::Unsupported { .. })
    ));
    Ok(())
}

#[test]
fn asg_exports_use_the_audited_schema_pin() -> Result<(), Error> {
    let inventory: Value =
        serde_json::from_str(include_str!("../schemas/asg/support-inventory.json"))?;
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/schema.json"))?;
    assert_eq!(
        inventory.get("schema_revision"),
        Some(&json!(asg::SCHEMA_REVISION))
    );
    assert_eq!(schema.get("$id"), Some(&json!(asg::SCHEMA_ID)));
    Ok(())
}

#[test]
fn asg_distinguishes_absent_and_present_empty_section_titles() -> Result<(), Error> {
    for (title, present) in [(Title::default(), false), (Title::new(Vec::new()), true)] {
        let mut document = Document::default();
        document.push_block(Block::Section(acdc_parser::Section::new(
            title,
            1,
            Vec::new(),
            Location::default(),
        )));
        let result = asg::document((&document).into());
        if present {
            assert_eq!(result?.pointer("/blocks/0/title"), Some(&json!([])));
        } else {
            assert!(
                matches!(result, Err(asg::Error::Unsupported { path, .. }) if path == "/blocks/0/title")
            );
        }
    }
    Ok(())
}

#[test]
fn asg_rejects_verbatim_without_a_literal_block_mapping() {
    let nodes = [acdc_parser::InlineNode::VerbatimText(
        acdc_parser::Verbatim::new("  literal\ntext  ", Location::default()),
    )];
    assert!(
        matches!(asg::inlines(ContentRef::from(&ContentAttribute::new(nodes.to_vec()))), Err(asg::Error::Unsupported { path, .. }) if path == "/0")
    );
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        nodes.to_vec(),
        Location::default(),
    )));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported { path, .. }) if path == "/blocks/0/inlines/0")
    );
}
