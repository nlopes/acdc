//! Block export uses retained forms, content and targets without invented delimiters.
#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, DelimitedBlock, DelimitedBlockType, Document, Image, InlineNode,
    Location, Options, Source, asg, parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn real_delimited_blocks_preserve_names_and_delimiters() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, name, delimiter, body, text) in [
            (
                "....\nLiteral\n....",
                "literal",
                "....",
                "inlines",
                "Literal",
            ),
            ("--\nText.\n--", "open", "--", "blocks/0/inlines", "Text."),
            (
                "****\nText.\n****",
                "sidebar",
                "****",
                "blocks/0/inlines",
                "Text.",
            ),
            (
                "____\nText.\n____",
                "quote",
                "____",
                "blocks/0/inlines",
                "Text.",
            ),
            (
                "++++\n<b>Raw</b>\n++++",
                "pass",
                "++++",
                "inlines",
                "<b>Raw</b>",
            ),
        ] {
            let parsed = parse(source, &options)?;
            let before = serde_json::to_value(parsed.document())?;
            let value = asg::document(parsed.document())?;
            assert_eq!(
                value.pointer("/blocks/0/name"),
                Some(&json!(name)),
                "{source}"
            );
            assert_eq!(value.pointer("/blocks/0/form"), Some(&json!("delimited")));
            assert_eq!(
                value.pointer("/blocks/0/delimiter"),
                Some(&json!(delimiter))
            );
            assert_eq!(
                value
                    .pointer(&format!("/blocks/0/{body}/0/value"))
                    .and_then(Value::as_str)
                    .map(str::trim_end),
                Some(text)
            );
            assert_eq!(serde_json::to_value(parsed.document())?, before);
        }
    }
    Ok(())
}

#[test]
fn paragraph_and_indented_leaf_forms_have_no_delimiters() -> Result<(), Error> {
    for (source, name, form) in [
        ("[literal]\n*Literal*", "literal", "paragraph"),
        ("  Indented literal", "literal", "indented"),
        ("[verse]\nLine one\nLine two", "verse", "paragraph"),
        ("[pass]\n<b>Raw</b>", "pass", "paragraph"),
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = asg::document(parsed.document())?;
        assert_eq!(
            value.pointer("/blocks/0/name"),
            Some(&json!(name)),
            "{source}"
        );
        assert_eq!(
            value.pointer("/blocks/0/form"),
            Some(&json!(form)),
            "{source}"
        );
        assert!(value.pointer("/blocks/0/delimiter").is_none());
    }
    Ok(())
}

#[test]
fn media_targets_and_retained_slots_are_read_from_owners() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, name, target) in [
            ("audio::track.ogg[]", "audio", "track.ogg"),
            ("video::movie.mp4[]", "video", "movie.mp4"),
            (
                "video::RvRhUHTV_8k,_SvwdK_HibQ,SGqg_ZzThDU[youtube]",
                "video",
                "RvRhUHTV_8k,_SvwdK_HibQ,SGqg_ZzThDU",
            ),
        ] {
            let parsed = parse(source, &options)?;
            let value = asg::document(parsed.document())?;
            assert_eq!(value.pointer("/blocks/0/name"), Some(&json!(name)));
            assert_eq!(value.pointer("/blocks/0/form"), Some(&json!("macro")));
            assert_eq!(
                value.pointer("/blocks/0/target"),
                Some(&json!(target)),
                "{source}"
            );
        }
    }
    let mut document = Document::default();
    document.push_block(Block::Image(Image::new(
        Source::Name("photo.png"),
        Location::default(),
    )));
    assert_eq!(
        asg::document((&document).into())?.pointer("/blocks/0/target"),
        Some(&json!("photo.png"))
    );
    let parsed = parse("video::movie.mp4[youtube,640,\"\"]", &Options::default())?;
    let value = asg::document(parsed.document())?;
    assert_eq!(
        value.pointer("/blocks/0/metadata/attributes/poster"),
        Some(&json!("youtube"))
    );
    assert_eq!(
        value.pointer("/blocks/0/metadata/attributes/$1"),
        Some(&json!("640"))
    );
    assert_eq!(
        value.pointer("/blocks/0/metadata/attributes/$2"),
        Some(&json!(""))
    );
    assert!(value.pointer("/blocks/0/metadata/attributes/$3").is_none());

    Ok(())
}

#[test]
fn toc_discrete_heading_and_breaks_use_their_schema_forms() -> Result<(), Error> {
    for (source, name, property, expected) in [
        ("toc::[]", "toc", "form", json!("macro")),
        ("[discrete]\n== *Heading*", "heading", "level", json!(1)),
        ("<<<", "break", "variant", json!("page")),
        ("'''", "break", "variant", json!("thematic")),
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = asg::document(parsed.document())?;
        assert_eq!(
            value.pointer("/blocks/0/name"),
            Some(&json!(name)),
            "{source}"
        );
        assert_eq!(
            value.pointer(&format!("/blocks/0/{property}")),
            Some(&expected),
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn missing_forms_and_unsupported_semantics_fail_explicitly() -> Result<(), Error> {
    for (source, property) in [
        ("NOTE: Paragraph.", "form"),
        ("[NOTE]\n--\nBody.\n--", "delimiter"),
        ("[stem]\n++++\nx+y\n++++", "inner"),
        ("|===\n|Cell\n|===", "inner"),
        ("image::photo.png[Alternative]", "alt"),
        ("[sidebar]\nParagraph.", "form"),
        ("[subs=none]\n....\nText\n....", "metadata/substitutions"),
    ] {
        let parsed = parse(source, &Options::default())?;
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, ..})
            if path == format!("/blocks/0/{property}")),
            "{source}"
        );
    }
    let mut document = Document::default();
    document.push_block(Block::DelimitedBlock(DelimitedBlock::new(
        DelimitedBlockType::DelimitedQuote(Vec::new()),
        ">",
        Location::default(),
    )));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/delimiter")
    );
    Ok(())
}

#[test]
fn nested_block_failures_keep_their_paths_and_owner() -> Result<(), Error> {
    let parsed = parse("--\nimage:photo.png[Alt]\n--", &Options::default())?;
    assert!(
        matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, construct: Some("inline image"), ..})
        if path == "/blocks/0/blocks/0/inlines/0")
    );
    Ok(())
}

#[test]
fn uncaptioned_library_listing_forms_are_supported() -> Result<(), Error> {
    for delimited in [false, true] {
        let content = vec![InlineNode::VerbatimText(acdc_parser::Verbatim {
            content: "<code> *literal*",
            location: Location::default(),
        })];
        let block = if delimited {
            Block::DelimitedBlock(DelimitedBlock::new(
                DelimitedBlockType::DelimitedListing(content),
                "```",
                Location::default(),
            ))
        } else {
            let mut paragraph = acdc_parser::Paragraph::new(content, Location::default());
            let mut metadata = paragraph.take_metadata();
            metadata.style = Some("source");
            paragraph.set_metadata(metadata)?;
            Block::Paragraph(paragraph)
        };
        let mut document = Document::default();
        document.push_block(block);
        let value = asg::document((&document).into())?;
        assert_eq!(value.pointer("/blocks/0/name"), Some(&json!("listing")));
        assert_eq!(
            value.pointer("/blocks/0/form"),
            Some(&json!(if delimited { "delimited" } else { "paragraph" }))
        );
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/value"),
            Some(&json!("<code> *literal*"))
        );
    }
    for source in ["----\nText\n----", "[source,rust]\nText"] {
        let parsed = parse(source, &Options::default())?;
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, ..})
            if path == "/blocks/0/metadata/caption")
        );
    }
    Ok(())
}
