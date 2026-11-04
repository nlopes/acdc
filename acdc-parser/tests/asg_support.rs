//! Semantic coverage of the checked upstream subset and its omission policy.
#![cfg(feature = "asg")]

use acdc_parser::{
    Anchor, AttributeValue, Block, Caption, CompatibilityMode, ContentAttribute, Document,
    Location, Options, Paragraph, SubstitutionSpec, asg, parse,
};
use serde_json::json;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn unsupported_block_owners_do_not_acquire_invented_forms() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, owner) in [
            ("toc::[]", "table of contents"),
            ("NOTE: Text.", "admonition"),
            ("[discrete]\n== Heading", "discrete heading"),
            ("'''", "thematic break"),
            ("<<<", "page break"),
            ("* Item", "unordered list"),
            (". Item", "ordered list"),
            ("<1> Item", "callout list"),
            ("Term:: Description", "description list"),
            ("image::photo.png[Alt]", "block image"),
            ("audio::track.ogg[]", "audio"),
            ("video::movie.mp4[]", "video"),
            ("====\nText.\n====", "example"),
            ("----\nText.\n----", "listing"),
            ("....\nText.\n....", "literal"),
            ("--\nText.\n--", "open block"),
            ("****\nText.\n****", "sidebar"),
            ("|===\n|Cell\n|===", "table"),
            ("++++\n<b>Text</b>\n++++", "passthrough block"),
            ("____\nText.\n____", "quote"),
            ("[verse]\n____\nText.\n____", "verse"),
            ("[stem]\n++++\nx+y\n++++", "STEM block"),
        ] {
            let parsed = parse(source, &options)?;
            assert!(
                matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {
                path, construct: Some(actual), ..
            }) if path == "/blocks/0" && actual == owner),
                "{mode:?}: {owner}: {source}"
            );
        }
    }
    Ok(())
}

#[test]
fn unsupported_inline_owners_are_rejected_inside_supported_paragraphs() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, owner) in [
            ("~sub~", "subscript span"),
            ("^super^", "superscript span"),
            ("\"`quote`\"", "curved quotation"),
            ("'`quote`'", "curved apostrophe"),
            ("[[target]]", "inline anchor"),
            ("icon:heart[]", "icon"),
            ("image:photo.png[Alt]", "inline image"),
            ("kbd:[Ctrl+C]", "keyboard"),
            ("btn:[Save]", "button"),
            ("menu:File[Save]", "menu"),
            ("stem:[x+y]", "STEM"),
            ("((Term))", "index term"),
            ("footnote:[Note]", "footnote"),
            ("Text +\nNext", "line break"),
        ] {
            let parsed = parse(
                &format!("= Guide\n:experimental:\n:stem:\n\nBefore {source} after."),
                &options,
            )?;
            let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
                return Err(format!("missing paragraph: {source}").into());
            };
            assert!(
                matches!(asg::inlines(&paragraph.content), Err(asg::Error::Unsupported {
                construct: Some(actual), ..
            }) if actual == owner),
                "{mode:?}: {owner}: {source}"
            );
        }
    }
    Ok(())
}

#[test]
fn supported_metadata_preserves_presence_and_rejects_extra_meaning() -> Result<(), Error> {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph
        .metadata
        .attributes
        .set("empty".into(), AttributeValue::String("".into()));
    paragraph
        .metadata
        .attributes
        .set("flag".into(), AttributeValue::Bool(true));
    paragraph.metadata.roles = vec!["one", "two"];
    paragraph.metadata.options = vec!["first", "second"];
    let mut document = Document::default();
    document.blocks.push(Block::Paragraph(paragraph));
    let projected = asg::document(&document)?;
    assert_eq!(
        projected.pointer("/blocks/0/metadata/attributes"),
        Some(&json!({"empty":"", "flag":""}))
    );
    assert_eq!(
        projected.pointer("/blocks/0/metadata/roles"),
        Some(&json!(["one", "two"]))
    );
    assert_eq!(
        projected.pointer("/blocks/0/metadata/options"),
        Some(&json!(["first", "second"]))
    );
    for (attribute, property) in [
        (AttributeValue::None, "ambiguous"),
        (AttributeValue::Bool(false), "unset"),
    ] {
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        paragraph
            .metadata
            .attributes
            .set(property.into(), attribute);
        document.blocks = vec![Block::Paragraph(paragraph)];
        assert!(
            matches!(asg::document(&document), Err(asg::Error::Unsupported {path, ..})
            if path == format!("/blocks/0/metadata/attributes/{property}"))
        );
    }
    for property in [
        "caption",
        "attribution",
        "citetitle",
        "substitutions",
        "anchors",
    ] {
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        match property {
            "caption" => paragraph.metadata.caption = Some(Caption::Unnumbered),
            "attribution" => {
                paragraph.metadata.attribution = Some(ContentAttribute::new(Vec::new()));
            }
            "citetitle" => paragraph.metadata.citetitle = Some(ContentAttribute::new(Vec::new())),
            "substitutions" => {
                paragraph.metadata.substitutions = Some(SubstitutionSpec::Explicit(Vec::new()));
            }
            "anchors" => {
                paragraph.metadata.anchors = vec![
                    Anchor::new("first", Location::default()),
                    Anchor::new("second", Location::default()),
                ];
            }
            _ => return Err("unknown metadata test property".into()),
        }
        document.blocks = vec![Block::Paragraph(paragraph)];
        assert!(
            matches!(asg::document(&document), Err(asg::Error::Unsupported {path, ..})
            if path == format!("/blocks/0/metadata/{property}"))
        );
    }
    Ok(())
}

#[test]
fn source_comments_are_omitted_without_dropping_their_neighbors() -> Result<(), Error> {
    for source in [
        "// comment\n\nBefore.\n\n// another\n\nAfter.",
        "////\ncomment\n////\n\nBefore.\n\nAfter.",
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = asg::document(parsed.document())?;
        let blocks = value
            .get("blocks")
            .and_then(serde_json::Value::as_array)
            .ok_or("blocks")?;
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            blocks
                .first()
                .ok_or("first paragraph")?
                .pointer("/inlines/0/value"),
            Some(&json!("Before."))
        );
        assert_eq!(
            blocks
                .get(1)
                .ok_or("second paragraph")?
                .pointer("/inlines/0/value"),
            Some(&json!("After."))
        );
    }
    Ok(())
}

#[test]
fn comment_metadata_cannot_disappear_with_source_comments() -> Result<(), Error> {
    for (source, property) in [
        (".Title\n////\ncomment\n////", "title"),
        ("[.review]\n////\ncomment\n////", "metadata/roles"),
    ] {
        let parsed = parse(source, &Options::default())?;
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, ..})
            if path == format!("/blocks/0/{property}"))
        );
    }
    Ok(())
}
