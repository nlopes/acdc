use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, InlineKind, InlineRef,
    },
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn role_only_lists_do_not_move_an_earlier_positional_value() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[Old]\n[alt=Named]\n[.role]\nimage::photo.png[]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Some(BlockKind::Image(image)) =
            parsed.document_ref().blocks().next().map(BlockRef::kind)
        else {
            return Err("missing image".into());
        };
        assert_eq!(
            attribute_text(
                image
                    .alt()
                    .and_then(|alt| alt.inlines().next())
                    .ok_or("missing alt")?
            )?,
            "Named"
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
    let Some(BlockKind::Image(image)) = parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("missing image".into());
    };
    let node = image
        .alt()
        .and_then(|alt| alt.inlines().next())
        .ok_or("missing alt")?;
    let location = node.location();
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
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
        assert_eq!(parsed.document_ref().footnotes().count(), 4);
        let included = parsed
            .document_ref()
            .blocks()
            .nth(2)
            .and_then(BlockRef::attributes)
            .and_then(|attributes| content(attributes, "attribution"))
            .and_then(|value| value.inlines().next())
            .ok_or("missing included attribution")?;
        let location = parsed.resolve_location(included.location());
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

        let Some(BlockKind::Table(table)) =
            parsed.document_ref().blocks().last().map(BlockRef::kind)
        else {
            return Err("missing table".into());
        };
        let attribution = table
            .rows()
            .next()
            .and_then(|row| row.cells().next())
            .and_then(|cell| cell.blocks().next())
            .and_then(BlockRef::attributes)
            .and_then(|attributes| content(attributes, "attribution"))
            .and_then(|value| value.inlines().next())
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
fn quoted_empty_media_dimensions_clear_only_in_compatible_mode() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (slot, compatible) in [("\"\"", ""), ("", "10")] {
            let source = format!("[width=10]\nimage::photo.png[Alt,{slot}]");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document_ref()
                .blocks()
                .next()
                .and_then(BlockRef::attributes)
                .ok_or("missing image")?;
            assert_eq!(
                text_value(metadata, "width"),
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
                .document_ref()
                .blocks()
                .next()
                .and_then(BlockRef::attributes)
                .ok_or("missing block")?;
            if name == "attribution" {
                assert!(content(metadata, "attribution").is_some());
            } else {
                assert_eq!(text_value(metadata, name), Some("rust"));
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
                    .document_ref()
                    .blocks()
                    .next()
                    .and_then(BlockRef::attributes)
                    .ok_or("missing media")?;
                assert_eq!(text_value(metadata, "width"), Some(width));
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
            .document_ref()
            .blocks()
            .next()
            .and_then(BlockRef::attributes)
            .ok_or("missing video")?;
        assert_eq!(
            text_value(metadata, "poster"),
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
            .document_ref()
            .footnotes()
            .map(|note| {
                source.get(note.location().absolute_start()..=note.location().absolute_end())
            })
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

fn attribute_text(node: InlineRef<'_>) -> Result<&str, Error> {
    if let InlineKind::Plain(text) = node.kind() {
        Ok(text.text())
    } else if let InlineKind::Raw(text) = node.kind() {
        Ok(text.text())
    } else {
        Err("expected literal attribute text".into())
    }
}

fn content<'a>(attributes: AttributesRef<'a>, name: &str) -> Option<ContentRef<'a>> {
    match attributes.get(name) {
        Some(AttributeValueRef::Content(value)) => Some(value),
        _ => None,
    }
}

fn text_value<'a>(attributes: AttributesRef<'a>, name: &str) -> Option<&'a str> {
    match attributes.get(name) {
        Some(AttributeValueRef::Text(value)) => Some(value),
        _ => None,
    }
}
