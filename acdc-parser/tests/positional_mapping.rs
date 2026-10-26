use acdc_parser::{
    Block, CompatibilityMode, DelimitedBlockType, InlineNode, Options, SafeMode, parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn role_only_lists_do_not_move_an_earlier_positional_value() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[Old]\n[alt=Named]\n[.role]\nimage::photo.png[]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Some(Block::Image(image)) = parsed.document().blocks.first() else {
            return Err("missing image".into());
        };
        assert!(
            matches!(image.alt.as_ref().and_then(|alt| alt.first()), Some(InlineNode::PlainText(text)) if text.content == "Named")
        );
    }
    Ok(())
}

#[test]
fn mapped_image_alt_points_to_the_authored_reference() -> Result<(), Error> {
    let source = ":text: A much longer image label\n\n[alt=Named]\n[{text}]\nimage::photo.png[]";
    let parsed = parse(
        source,
        &Options::builder()
            .with_compatibility_mode(CompatibilityMode::Draft)
            .build()?,
    )?;
    let Some(Block::Image(image)) = parsed.document().blocks.first() else {
        return Err("missing image".into());
    };
    let node = image
        .alt
        .as_ref()
        .and_then(|alt| alt.first())
        .ok_or("missing alt")?;
    let location = node.location();
    assert_eq!(
        source.get(location.absolute_start..=location.absolute_end),
        Some("{text}")
    );
    Ok(())
}

#[test]
fn includes_and_table_cells_keep_selected_content_and_locations() -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let input = root.join("compatibility_mapped_content.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &input,
            &Options::builder()
                .with_compatibility_mode(mode)
                .with_safe_mode(SafeMode::Unsafe)
                .build()?,
        )?;
        assert_eq!(parsed.document().footnotes.len(), 4);
        let included = parsed
            .document()
            .blocks
            .get(2)
            .and_then(Block::metadata)
            .and_then(|metadata| metadata.attribution.as_ref())
            .and_then(|value| value.first())
            .ok_or("missing included attribution")?;
        let location = parsed.source_location(included.location());
        assert_eq!(
            location.file,
            Some(root.join("includes/compatibility_mapped_content.adoc"))
        );
        assert_eq!(
            location.location.start.line,
            if mode == CompatibilityMode::Draft {
                2
            } else {
                1
            }
        );

        let Some(Block::DelimitedBlock(block)) = parsed.document().blocks.last() else {
            return Err("missing table".into());
        };
        let DelimitedBlockType::DelimitedTable(table) = &block.inner else {
            return Err("expected table".into());
        };
        let attribution = table
            .rows
            .first()
            .and_then(|row| row.columns.first())
            .and_then(|cell| cell.content.first())
            .and_then(Block::metadata)
            .and_then(|metadata| metadata.attribution.as_ref())
            .and_then(|value| value.first())
            .ok_or("missing nested attribution")?;
        let text = attribute_text(attribution)?;
        assert_eq!(
            text,
            if mode == CompatibilityMode::Draft {
                "Ada"
            } else {
                "Old"
            }
        );
    }
    Ok(())
}

#[test]
fn fence_language_does_not_shift_or_map_preceding_positions() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[quote,Author,Book,linenums]\n```rust\nx\n```",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let metadata = parsed
            .document()
            .blocks
            .first()
            .and_then(Block::metadata)
            .ok_or("missing fence")?;
        assert_eq!(
            metadata.attributes.get_string("language").as_deref(),
            Some("rust")
        );
        assert!(!metadata.attributes.contains_key("linenums"));
        assert!(metadata.attribution.is_none());
        assert_eq!(
            metadata.positional_attributes().collect::<Vec<_>>(),
            [Some("Author"), Some("Book"), Some("linenums")]
        );
    }
    Ok(())
}

#[test]
fn quoted_empty_media_dimensions_clear_only_in_compatible_mode() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (slot, compatible) in [("\"\"", ""), ("", "10")] {
            let source = format!("[width=10]\nimage::photo.png[Alt,{slot}]");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document()
                .blocks
                .first()
                .and_then(Block::metadata)
                .ok_or("missing image")?;
            assert_eq!(
                metadata.attributes.get_string("width").as_deref(),
                Some(if mode == CompatibilityMode::Draft {
                    "10"
                } else {
                    compatible
                })
            );
        }
    }
    Ok(())
}

#[test]
fn draft_maps_preceding_media_positions_after_aggregation() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let source = "[alt=Named]\n[Positional,100,200]\nimage::photo.png[]";
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Some(Block::Image(image)) = parsed.document().blocks.first() else {
            return Err("missing image".into());
        };
        let alt = image.alt.as_ref().ok_or("missing alt")?;
        let Some(InlineNode::PlainText(text)) = alt.first() else {
            return Err("expected plain alt".into());
        };
        assert_eq!(
            text.content,
            if mode == CompatibilityMode::Draft {
                "Positional"
            } else {
                "Named"
            }
        );
        assert_eq!(
            image.metadata.attributes.get_string("width").as_deref(),
            (mode == CompatibilityMode::Draft).then_some("100")
        );
        assert_eq!(
            image.metadata.positional_attributes().collect::<Vec<_>>(),
            [Some("100"), Some("200")]
        );
    }
    Ok(())
}

#[test]
fn draft_selects_source_and_quote_attributes_in_document_order() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for source in [
            "[source,rust,linenums,language=python,linenums=named]\n----\nx\n----",
            "[source,rust,linenums]\n[language=python,linenums=named]\n----\nx\n----",
        ] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document()
                .blocks
                .first()
                .and_then(Block::metadata)
                .ok_or("missing block")?;
            let expected = if mode == CompatibilityMode::Draft {
                "python"
            } else {
                "rust"
            };
            assert_eq!(
                metadata.attributes.get_string("language").as_deref(),
                Some(expected)
            );
            assert_eq!(
                metadata.positional_attributes().collect::<Vec<_>>(),
                [Some("rust"), Some("linenums")]
            );
        }
        let parsed = parse(
            "[quote,Positional,Citation]\n[attribution=Named,citetitle=NamedTitle]\n____\nText.\n____",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let metadata = parsed
            .document()
            .blocks
            .first()
            .and_then(Block::metadata)
            .ok_or("missing quote")?;
        let attribution = metadata.attribution.as_ref().ok_or("missing attribution")?;
        let text = attribute_text(attribution.first().ok_or("missing text")?)?;
        assert_eq!(
            text,
            if mode == CompatibilityMode::Draft {
                "Named"
            } else {
                "Positional"
            }
        );
    }
    Ok(())
}

#[test]
fn delimiter_supplies_the_mapping_when_style_is_absent() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (source, name) in [
            ("[,Author,Book]\n____\nText.\n____", "attribution"),
            ("[,rust,linenums]\n----\nx\n----", "language"),
        ] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document()
                .blocks
                .first()
                .and_then(Block::metadata)
                .ok_or("missing block")?;
            if name == "attribution" {
                assert!(metadata.attribution.is_some());
            } else {
                assert_eq!(
                    metadata.attributes.get_string(name).as_deref(),
                    Some("rust")
                );
            }
        }
    }
    Ok(())
}

#[test]
fn macro_dimensions_follow_the_last_authored_value() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for macro_name in ["image", "video"] {
            for (attributes, width) in [
                ("width=300,100,200", "100"),
                ("First,100,200,width=300", "300"),
            ] {
                let source = format!("{macro_name}::file[{attributes}]");
                let parsed = parse(
                    &source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let metadata = parsed
                    .document()
                    .blocks
                    .first()
                    .and_then(Block::metadata)
                    .ok_or("missing media")?;
                assert_eq!(
                    metadata.attributes.get_string("width").as_deref(),
                    Some(width)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn compatible_video_does_not_map_preceding_style_to_poster() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[poster=Named]\n[Positional]\nvideo::movie.mp4[]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let metadata = parsed
            .document()
            .blocks
            .first()
            .and_then(Block::metadata)
            .ok_or("missing video")?;
        assert_eq!(
            metadata.attributes.get_string("poster").as_deref(),
            Some(if mode == CompatibilityMode::Draft {
                "Positional"
            } else {
                "Named"
            })
        );
    }
    Ok(())
}

#[test]
fn only_the_selected_attribution_registers_before_body_content() -> Result<(), Error> {
    let source = "[quote,'Positional footnote:[Positional.]']\n[attribution='Named footnote:[Named.]']\n____\nBody footnote:[Body.]\n____";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let selected = parsed
            .document()
            .footnotes
            .iter()
            .map(|note| source.get(note.location.absolute_start..=note.location.absolute_end))
            .collect::<Vec<_>>();
        assert_eq!(
            selected,
            [
                Some(if mode == CompatibilityMode::Draft {
                    "footnote:[Named.]"
                } else {
                    "footnote:[Positional.]"
                }),
                Some("footnote:[Body.]")
            ]
        );
    }
    Ok(())
}

#[test]
fn unrelated_block_forms_keep_positions_without_routing_them() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for source in [
            "[quote,Author,Book]\n----\nx\n----",
            "[quote,Author,Book]\n* Item",
            "[source,rust]\n====\nText.\n====",
        ] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document()
                .blocks
                .first()
                .and_then(Block::metadata)
                .ok_or("missing block")?;
            assert!(metadata.attribution.is_none(), "{source}");
            assert!(!metadata.attributes.contains_key("language"), "{source}");
            assert!(metadata.positional_attribute(0).is_some());
        }
    }
    Ok(())
}

fn attribute_text<'a>(node: &InlineNode<'a>) -> Result<&'a str, Error> {
    if let InlineNode::PlainText(text) = node {
        Ok(text.content)
    } else if let InlineNode::RawText(text) = node {
        Ok(text.content)
    } else {
        Err("expected literal attribute text".into())
    }
}
