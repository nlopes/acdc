use crate::{Block, CompatibilityMode, Document, Options, OptionsBuilder, Title, asg, parse};
use serde_json::json;

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
fn owned_blocks(parsed: &crate::ParseResult) -> Document<'_> {
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
            .with_attributes(crate::ElementAttributes::default())?;
        let value = asg::document((&document).into())?;
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

#[test]
fn asg_preserves_formatted_titles_reftext_and_ids() -> Result<(), Error> {
    for options in modes()? {
        let parsed = parse(
            include_str!("../../../fixtures/tests/asg_core_content.adoc"),
            &options,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let references = parsed.document().references.clone();
        let toc = parsed.document().toc_entries.clone();
        let value = asg::document(parsed.document_ref())?;
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
        assert_eq!(asg::document(parsed.document_ref())?, value);
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        assert_eq!(parsed.document().references, references);
        assert_eq!(parsed.document().toc_entries, toc);
    }
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
                asg::document(parsed.document_ref()),
                Err(asg::Error::Unsupported { .. })
            ),
            "{source}"
        );
    }
    let parsed = parse("[#one]\n== Heading\n\nText.", &Options::default())?;
    let mut document = owned_blocks(&parsed);
    asg::document((&document).into())?;
    document
        .references
        .get_mut("one")
        .ok_or("missing reference")?
        .title = Some(Title::new(Vec::new()));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported { path, .. }) if path == "/references/one")
    );
    let mut document = owned_blocks(&parsed);
    document
        .toc_entries
        .first_mut()
        .ok_or("missing TOC entry")?
        .level = 4;
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported { path, .. }) if path == "/toc_entries/0")
    );
    Ok(())
}
