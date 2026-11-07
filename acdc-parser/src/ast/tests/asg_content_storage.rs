use crate::{
    Block, Caption, CharacterReference, CharacterReferenceKind, CompatibilityMode,
    ContentAttribute, Document, InlineNode, Location, Options, Paragraph, asg, ast::ContentRef,
    parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn export(document: &Document<'_>) -> Result<Value, asg::Error> {
    asg::document_with_profile(document.into(), asg::Profile::Acdc)
}

#[test]
fn resolved_content_fields_preserve_their_inline_trees() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for source in [
            "[quote,attribution='*Ada*',citetitle='_Book_']\n____\nBody.\n____",
            "[verse,attribution=\"*literal*\",citetitle=\"\"]\n____\nBody.\n____",
            ".Title\n[caption='*Custom* ']\n====\nBody.\n====",
        ] {
            let parsed = parse(source, &options)?;
            let before = serde_json::to_value(parsed.document_storage())?;
            let value = export(parsed.document_storage())?;
            let metadata = parsed
                .document_storage()
                .blocks
                .first()
                .ok_or("block")?
                .metadata()
                .ok_or("metadata")?;
            if source.contains("attribution=") {
                assert!(metadata.attribution.is_some());
                assert!(metadata.citetitle.is_some());
            } else {
                assert!(matches!(metadata.caption, Some(Caption::CustomContent(_))));
                assert_eq!(
                    value.pointer("/blocks/0/caption/prefix/0/variant"),
                    Some(&json!("strong"))
                );
            }
            for (name, content) in [
                ("attribution", metadata.attribution.as_ref()),
                ("citetitle", metadata.citetitle.as_ref()),
            ] {
                if let Some(content) = content {
                    assert_eq!(
                        value.pointer(&format!("/blocks/0/{name}")),
                        Some(&asg::inlines(ContentRef::from(content))?)
                    );
                }
            }
            if let Some(Caption::CustomContent(content)) = &metadata.caption {
                assert_eq!(
                    value.pointer("/blocks/0/caption/prefix"),
                    Some(&asg::inlines(ContentRef::from(content))?)
                );
            }
            assert_eq!(serde_json::to_value(parsed.document_storage())?, before);
        }
    }
    Ok(())
}

#[test]
fn captioned_targets_are_reconstructible_but_stale_catalogs_fail() -> Result<(), Error> {
    let parsed = parse("[#sample]\n.Title\n====\nBody.\n====", &Options::default())?;
    let value = export(parsed.document_storage())?;
    assert_eq!(value.pointer("/blocks/0/caption/number"), Some(&json!(1)));
    let mut document = Document::default();
    document
        .blocks
        .clone_from(&parsed.document_storage().blocks);
    document
        .references
        .clone_from(&parsed.document_storage().references);
    document.attributes = parsed.document_storage().attributes.clone();
    document
        .references
        .get_mut("sample")
        .ok_or("reference")?
        .caption = Some(Caption::Unnumbered);
    assert!(
        matches!(export(&document),Err(asg::Error::Unsupported {path,..}) if path=="/references/sample")
    );
    Ok(())
}

#[test]
fn nested_unsupported_content_keeps_its_property_path() -> Result<(), Error> {
    let invalid = InlineNode::CharacterReference(
        CharacterReference::new(CharacterReferenceKind::Numeric(0), Location::default())
            .ok_or("numeric reference")?,
    );
    for property in ["attribution", "citetitle", "caption"] {
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        let content = ContentAttribute::new(vec![invalid.clone()]);
        match property {
            "attribution" => paragraph.metadata.attribution = Some(content),
            "citetitle" => paragraph.metadata.citetitle = Some(content),
            _ => paragraph.metadata.caption = Some(Caption::CustomContent(content)),
        }
        let mut document = Document::default();
        document.blocks.push(Block::Paragraph(paragraph));
        let field = if property == "caption" {
            "caption/prefix"
        } else {
            property
        };
        assert!(
            matches!(export(&document),Err(asg::Error::Unsupported {path,..}) if path==format!("/blocks/0/{field}/0"))
        );
    }
    Ok(())
}
