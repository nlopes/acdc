#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, Document, Header, Location, Options, OptionsBuilder, Paragraph,
    Title, asg, parse,
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
    document.location = parsed.document().location.clone();
    document.header = Some(Header::new(
        Title::new(Vec::new()),
        document.location.clone(),
    ));
    let value = asg::document(&document)?;
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
fn asg_reads_retained_positional_slots() -> Result<(), Error> {
    for options in modes()? {
        let parsed = parse("[custom,alpha,\"\",omega]\nText.\n", &options)?;
        let mut document = owned_blocks(&parsed);
        let Block::Paragraph(paragraph) = document.blocks.first_mut().ok_or("missing paragraph")?
        else {
            return Err("wrong block".into());
        };
        paragraph.metadata = paragraph
            .metadata
            .clone()
            .with_attributes(acdc_parser::ElementAttributes::default())?;
        let value = asg::document(&document)?;
        assert_eq!(
            value.pointer("/blocks/0/metadata/attributes/$1"),
            Some(&json!("alpha"))
        );
        assert_eq!(
            value.pointer("/blocks/0/metadata/attributes/$2").cloned(),
            (options.compatibility_mode == CompatibilityMode::Asciidoctor).then(|| json!("")),
            "{:?}",
            options.compatibility_mode
        );
        assert_eq!(
            value.pointer("/blocks/0/metadata/attributes/$3"),
            Some(&json!("omega"))
        );
    }
    Ok(())
}

fn owned_blocks(parsed: &acdc_parser::ParseResult) -> Document<'_> {
    let mut document = Document::default();
    document.location = parsed.document().location.clone();
    document.attributes = parsed.document().attributes.clone();
    document.blocks.clone_from(&parsed.document().blocks);
    document
        .references
        .clone_from(&parsed.document().references);
    document
        .toc_entries
        .clone_from(&parsed.document().toc_entries);
    document
}

#[test]
fn asg_preserves_formatted_titles_reftext_and_ids() -> Result<(), Error> {
    for options in modes()? {
        let parsed = parse(
            include_str!("../fixtures/tests/asg_core_content.adoc"),
            &options,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let references = parsed.document().references.clone();
        let toc = parsed.document().toc_entries.clone();
        let value = asg::document(parsed.document())?;
        assert_eq!(
            value.pointer("/header/title/0/variant"),
            Some(&json!("strong"))
        );
        assert_eq!(
            value.pointer("/header/authors/0/address"),
            Some(&json!("ada@example.org"))
        );
        assert_eq!(value.pointer("/blocks/0/id"), Some(&json!("intro")));
        assert_eq!(
            value.pointer("/blocks/0/metadata/roles"),
            Some(&json!(["compact"]))
        );
        assert_eq!(
            value.pointer("/blocks/0/blocks/0/reftext/0/variant"),
            Some(&json!("strong"))
        );
        assert!(
            value
                .pointer("/blocks/0/blocks/0/metadata/attributes/title")
                .is_none()
        );
        assert_eq!(asg::document(parsed.document())?, value);
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        assert_eq!(parsed.document().references, references);
        assert_eq!(parsed.document().toc_entries, toc);
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
            ("[subs=none]\n*Literal*", "/blocks/0/metadata/substitutions"),
            ("[.role]*Bold*", "/blocks/0/inlines/0/metadata"),
            ("[#inline]*Bold*", "/blocks/0/inlines/0/metadata"),
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
    let nodes = [acdc_parser::InlineNode::RawText(acdc_parser::Raw {
        content: "{missing}",
        location: Location::default(),
        subs: vec![acdc_parser::Substitution::Attributes],
    })];
    assert!(
        matches!(asg::inlines(&nodes), Err(asg::Error::Unsupported { path, .. }) if path == "/0/substitutions")
    );
}

#[test]
fn asg_omits_unknown_optional_locations_and_rejects_partial_ones() -> Result<(), Error> {
    let mut document = Document::default();
    document.blocks.push(Block::Paragraph(Paragraph::new(
        Vec::new(),
        Location::default(),
    )));
    let value = asg::document(&document)?;
    assert!(value.get("location").is_none());
    assert!(value.pointer("/blocks/0/location").is_none());
    let Block::Paragraph(paragraph) = document.blocks.first_mut().ok_or("missing paragraph")?
    else {
        return Err("wrong block".into());
    };
    paragraph.location.start.line = 1;
    assert!(
        matches!(asg::document(&document), Err(asg::Error::InvalidLocation { path, .. }) if path == "/blocks/0/location/1")
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
    let nodes = [acdc_parser::InlineNode::PlainText(acdc_parser::Plain {
        content: "α",
        escaped: true,
        location: source.clone(),
    })];
    let value = asg::inlines(&nodes)?;
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
    let nodes = [acdc_parser::InlineNode::PlainText(acdc_parser::Plain {
        content: "α",
        escaped: true,
        location: source,
    })];
    assert!(matches!(
        asg::inlines(&nodes),
        Err(asg::Error::InvalidLocation { .. })
    ));
    Ok(())
}

#[test]
fn asg_rejects_independent_header_and_catalog_values() -> Result<(), Error> {
    for source in [
        "= Title: Subtitle\n\nText.",
        "[#title]\n= Title\n\nText.",
        "= Guide\n:sectnums:\n\n== Heading\n",
    ] {
        let parsed = parse(source, &Options::default())?;
        assert!(
            matches!(
                asg::document(parsed.document()),
                Err(asg::Error::Unsupported { .. })
            ),
            "{source}"
        );
    }
    let parsed = parse("[#one]\n== Heading\n\nText.", &Options::default())?;
    let mut document = owned_blocks(&parsed);
    asg::document(&document)?;
    document
        .references
        .get_mut("one")
        .ok_or("missing reference")?
        .title = Some(Title::new(Vec::new()));
    assert!(
        matches!(asg::document(&document), Err(asg::Error::Unsupported { path, .. }) if path == "/references/one")
    );
    let mut document = owned_blocks(&parsed);
    document
        .toc_entries
        .first_mut()
        .ok_or("missing TOC entry")?
        .level = 4;
    assert!(
        matches!(asg::document(&document), Err(asg::Error::Unsupported { path, .. }) if path == "/toc_entries/0")
    );
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
        document
            .blocks
            .push(Block::Section(acdc_parser::Section::new(
                title,
                1,
                Vec::new(),
                Location::default(),
            )));
        let result = asg::document(&document);
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
        acdc_parser::Verbatim {
            content: "  literal\ntext  ",
            location: Location::default(),
        },
    )];
    assert!(
        matches!(asg::inlines(&nodes), Err(asg::Error::Unsupported { path, .. }) if path == "/0")
    );
    let mut document = Document::default();
    document.blocks.push(Block::Paragraph(Paragraph::new(
        nodes.to_vec(),
        Location::default(),
    )));
    assert!(
        matches!(asg::document(&document), Err(asg::Error::Unsupported { path, .. }) if path == "/blocks/0/inlines/0")
    );
}
