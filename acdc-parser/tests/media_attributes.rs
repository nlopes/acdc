use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse};
use serde_json::Value;

type Error = Box<dyn std::error::Error>;

fn json_text<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

#[test]
fn inline_image_json_keeps_tooltips_separate_from_alt() -> Result<(), Error> {
    for (attributes, alt) in [
        ("title=+my title+", None),
        ("Alt,title=+my title+", Some("Alt")),
        ("\"\",title=+my title+", Some("")),
        ("alt=,title=+my title+", Some("")),
    ] {
        let source = format!("image:thing.png[{attributes}]\n");
        let parsed = parse(&source, &Options::default())?;
        let json = serde_json::to_value(parsed.document())?;
        let image = json.pointer("/blocks/0/inlines/0").ok_or("missing image")?;
        assert_eq!(json_text(image, "/tooltip/0/value"), Some("my title"));
        match alt {
            Some("") => assert_eq!(image.get("alt"), Some(&serde_json::json!([]))),
            _ => assert_eq!(json_text(image, "/alt/0/value"), alt),
        }
        assert!(image.get("title").is_none());
    }
    Ok(())
}

#[test]
fn media_and_named_blocks_serialize_content_separately_from_other_attributes() -> Result<(), Error>
{
    let attributes = "title=\"Media *title*\",alt=Named,width=80,height=40,link=https://example.com,window=_blank,role=\"hot cool\",opts=\"inline,nofollow\",id=media-id,format=svg,fallback=fallback.png,align=center,float=right,loading=lazy,custom=kept";
    for (source, inline) in [
        (format!("image:sample.svg[{attributes}]"), true),
        (format!("image::sample.svg[{attributes}]"), false),
        (
            format!("audio::clip.ogg[{attributes},start=5,end=12]"),
            false,
        ),
        (
            format!("video::clip.mp4[{attributes},poster=poster.png,start=5,end=12]"),
            false,
        ),
        (format!("[{attributes}]\nParagraph."), false),
        (format!("[{attributes}]\n----\ncode\n----"), false),
        (format!("[{attributes}]\n====\nExample.\n===="), false),
        (format!("[{attributes}]\n|===\n|Cell\n|==="), false),
        (format!("[{attributes}]\n* Item"), false),
    ] {
        let parsed = parse(&source, &Options::default())?;
        let json = serde_json::to_value(parsed.document())?;
        let node = json
            .pointer(if inline {
                "/blocks/0/inlines/0"
            } else {
                "/blocks/0"
            })
            .ok_or("missing node")?;
        let metadata = node.get("metadata").ok_or("missing metadata")?;
        assert!(metadata.pointer("/attributes/title").is_none());
        for (key, value) in [
            ("alt", "Named"),
            ("width", "80"),
            ("height", "40"),
            ("link", "https://example.com"),
            ("window", "_blank"),
            ("format", "svg"),
            ("fallback", "fallback.png"),
            ("align", "center"),
            ("float", "right"),
            ("loading", "lazy"),
            ("custom", "kept"),
        ] {
            if (inline || source.starts_with("image::")) && key == "alt" {
                continue;
            }
            assert_eq!(
                json_text(metadata, &format!("/attributes/{key}")),
                Some(value),
                "{source}: {key}"
            );
        }
        assert_eq!(
            metadata.get("roles"),
            Some(&serde_json::json!(["hot", "cool"]))
        );
        assert_eq!(
            metadata.get("options"),
            Some(&serde_json::json!(["inline", "nofollow"]))
        );
        assert_eq!(json_text(metadata, "/id/id"), Some("media-id"));
        let name = json_text(node, "/name");
        if matches!(name, Some("audio" | "video")) {
            assert_eq!(json_text(metadata, "/attributes/start"), Some("5"));
            assert_eq!(json_text(metadata, "/attributes/end"), Some("12"));
        }
        if name == Some("video") {
            assert_eq!(
                json_text(metadata, "/attributes/poster"),
                Some("poster.png")
            );
        }
        if inline {
            assert_eq!(json_text(node, "/alt/0/value"), Some("Named"));
            assert_eq!(json_text(node, "/tooltip/0/value"), Some("Media "));
            assert_eq!(json_text(node, "/tooltip/1/variant"), Some("strong"));
        } else {
            assert_eq!(json_text(node, "/title/0/value"), Some("Media "));
            assert_eq!(json_text(node, "/title/1/variant"), Some("strong"));
            assert_eq!(json_text(node, "/title/1/inlines/0/value"), Some("title"));
        }
    }
    Ok(())
}

#[test]
fn media_attributes_select_named_values_and_keep_empty_slots() -> Result<(), Error> {
    for attributes in ["Positional,alt=Named", "alt=First,alt=Named"] {
        for prefix in ["image::", "Inline image:"] {
            let source = format!("{prefix}sample.svg[{attributes},title=Tooltip]\n");
            let parsed = parse(&source, &Options::default())?;
            match parsed.document().blocks.first() {
                Some(Block::Image(image)) => assert_eq!(
                    image.alt.as_ref().map(content_text).as_deref(),
                    Some("Named")
                ),
                Some(Block::Paragraph(paragraph)) => {
                    let image = paragraph
                        .content
                        .iter()
                        .find_map(|node| {
                            if let InlineNode::Macro(InlineMacro::Image(image)) = node {
                                Some(image)
                            } else {
                                None
                            }
                        })
                        .ok_or("missing inline image")?;
                    assert_eq!(
                        image.alt.as_ref().map(content_text).as_deref(),
                        Some("Named")
                    );
                    assert_eq!(
                        image.tooltip.as_ref().map(content_text).as_deref(),
                        Some("Tooltip")
                    );
                }
                _ => return Err("missing image".into()),
            }
        }
    }
    for (attributes, empty_alt) in [
        ("", false),
        (",40,20", false),
        ("\"\",40,20", true),
        ("alt=", true),
    ] {
        let source = format!("Inline image:sample.svg[{attributes}].\n");
        let parsed = parse(&source, &Options::default())?;
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
            return Err("missing paragraph".into());
        };
        let image = paragraph
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Image(image)) = node {
                    Some(image)
                } else {
                    None
                }
            })
            .ok_or("missing inline image")?;
        assert_eq!(
            image.alt.as_ref().map(content_text).as_deref(),
            empty_alt.then_some(""),
            "{attributes}"
        );
        if attributes.contains("40") {
            assert_eq!(
                image.metadata.attributes.get_string("width").as_deref(),
                Some("40")
            );
            assert_eq!(
                image.metadata.attributes.get_string("height").as_deref(),
                Some("20")
            );
        }
    }
    Ok(())
}

#[test]
fn media_and_other_block_titles_register_only_the_selected_inline_content() -> Result<(), Error> {
    for block in [
        "audio::clip.ogg[title=\"Selected footnote:[Chosen.]\"]",
        "video::clip.mp4[title=\"Selected footnote:[Chosen.]\"]",
        "[title=\"Selected footnote:[Chosen.]\"]\n----\ncode\n----",
        "[title=\"Selected footnote:[Chosen.]\"]\nParagraph.",
    ] {
        let source = format!(".Discarded footnote:[Unused.]\n{block}\n");
        let parsed = parse(&source, &Options::default())?;
        let [note] = parsed.document().footnotes.as_slice() else {
            return Err(format!("unexpected footnotes: {source}").into());
        };
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
            Some("footnote:[Chosen.]")
        );
    }
    Ok(())
}

#[test]
fn media_duplicate_attributes_use_last_value_except_accumulated_options() -> Result<(), Error> {
    let parsed = parse(
        "image::sample.svg[alt=First,alt=Last,id=first,id=last,role=first,role=last,opts=nofollow,opts=noopener,width=90,width=80]\n\nvideo::clip.mp4[first.png,90,40,poster=last.png]\n",
        &Options::default(),
    )?;
    let [Block::Image(image), Block::Video(video)] = parsed.document().blocks.as_slice() else {
        return Err("missing media blocks".into());
    };
    assert_eq!(
        image.alt.as_ref().map(content_text).as_deref(),
        Some("Last")
    );
    assert_eq!(
        image.metadata.attributes.get_string("width").as_deref(),
        Some("80")
    );
    assert_eq!(image.metadata.id.as_ref().map(|id| id.id), Some("last"));
    assert_eq!(image.metadata.roles, ["last"]);
    assert_eq!(image.metadata.options, ["nofollow", "noopener"]);
    assert_eq!(
        video.metadata.attributes.get_string("poster").as_deref(),
        Some("last.png")
    );
    Ok(())
}

#[test]
fn inline_image_unquoted_apostrophes_do_not_end_the_attribute_list() -> Result<(), Error> {
    let parsed = parse(
        "Inline image:sample.svg[Bob's photo,40,20].",
        &Options::default(),
    )?;
    let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
        return Err("missing paragraph".into());
    };
    assert!(
        paragraph
            .content
            .iter()
            .any(|node| matches!(node, InlineNode::Macro(InlineMacro::Image(_))))
    );
    Ok(())
}

fn content_text(value: &acdc_parser::ContentAttribute<'_>) -> String {
    value
        .iter()
        .map(|node| {
            assert!(matches!(
                node,
                InlineNode::PlainText(_) | InlineNode::RawText(_)
            ));
            if let InlineNode::PlainText(text) = node {
                text.content
            } else if let InlineNode::RawText(text) = node {
                text.content
            } else {
                ""
            }
        })
        .collect()
}
