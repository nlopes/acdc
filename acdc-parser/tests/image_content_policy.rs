use std::{fs, path::Path};

use acdc_parser::{
    Block, CompatibilityMode, InlineNode, Options, SafeMode,
    ast::{BlockKind, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn draft_block_alt_is_literal_and_does_not_register_macros(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[alt='*Bold* footnote:alt[Note]']\nimage::photo.png[]\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Image(image)) = parsed.document().blocks.first() else {
        return Err("expected image".into());
    };
    let alt = image.alt.as_ref().ok_or("missing alt")?;
    if mode == CompatibilityMode::Draft {
        let [InlineNode::RawText(text)] = alt.as_ref() else {
            return Err("expected protected literal alt".into());
        };
        assert_eq!(text.content, "*Bold* footnote:alt[Note]");
        assert_eq!(parsed.document().footnotes.len(), 0);
    } else {
        assert!(matches!(alt.first(), Some(InlineNode::BoldText(_))));
        assert_eq!(parsed.document().footnotes.len(), 1);
    }
    Ok(())
}

#[rstest::rstest]
fn an_expanded_alt_entry_points_back_to_its_authored_reference(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":media: alt='Expanded π *label*'\n\n[{media}]\nimage::photo.png[]\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Image(image)) = parsed.document().blocks.first() else {
        return Err("expected image".into());
    };
    let first = image
        .alt
        .as_ref()
        .ok_or("missing alt")?
        .first()
        .ok_or("empty alt")?;
    assert!(matches!(
        first,
        InlineNode::PlainText(_) | InlineNode::RawText(_)
    ));
    let location = first.location();
    let start = source.find("{media}").ok_or("missing reference")?;
    assert_eq!(location.absolute_start, start);
    assert_eq!(location.absolute_end, start + "{media}".len() - 1);
    assert_eq!(location.start.line, 3);
    assert_eq!(location.end.line, 3);
    Ok(())
}

#[rstest::rstest]
fn image_alt_snapshots_and_selected_macro_registration(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        root.join("compatibility_image_alt_snapshots.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let image = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            if let Block::Image(image) = block {
                Some(image)
            } else {
                None
            }
        })
        .ok_or("expected image")?;
    let alt = image.alt.as_ref().ok_or("missing alt")?;
    if mode == CompatibilityMode::Draft {
        assert!(matches!(alt.as_ref(), [InlineNode::RawText(text)] if text.content == "*Early*"));
    } else {
        let [InlineNode::BoldText(bold)] = alt.as_ref() else {
            return Err("expected early bold alt".into());
        };
        assert!(
            matches!(bold.content.as_slice(), [InlineNode::PlainText(text)] if text.content == "Early")
        );
    }
    let expected = if mode == CompatibilityMode::Draft {
        vec![Some("caption"), Some("first"), Some("inline")]
    } else {
        vec![
            Some("alt"),
            Some("caption"),
            Some("first"),
            Some("second"),
            Some("inline"),
        ]
    };
    assert_eq!(
        parsed
            .document()
            .footnotes
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        expected
    );
    Ok(())
}

#[rstest::rstest]
fn absent_and_empty_alt_do_not_gain_filename_fallback(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("", "alt=\"\"", "\"\"", "alt=''", "''")] attributes: &str,
) -> Result<(), Error> {
    let source = format!("image::photo.png[{attributes}]\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Image(image)) = parsed.document().blocks.first() else {
        return Err("expected image".into());
    };
    if attributes.is_empty() {
        assert!(image.alt.is_none());
    } else {
        assert!(
            image
                .alt
                .as_ref()
                .ok_or("expected explicitly empty alt")?
                .is_empty()
        );
    }
    Ok(())
}

#[rstest::rstest]
fn included_and_nested_alt_locations_use_the_authored_value(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let primary = root.join("compatibility_image_alt_locations.adoc");
    let included = root.join("includes/compatibility_image_selected.adoc");
    let primary_source = fs::read_to_string(&primary)?;
    let included_source = fs::read_to_string(&included)?;
    let parsed = parse_file(
        &primary,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let images = parsed
        .document_ref()
        .descendants()
        .filter_map(|node| {
            let StructuralRef::Block(block) = node else {
                return None;
            };
            if let BlockKind::Image(image) = block.kind() {
                Some(image)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [expanded, positional, selected, nested, direct] = images.as_slice() else {
        return Err("expected five images".into());
    };
    for (image, authored, source, file) in [
        (*expanded, "{media}", primary_source.as_str(), &primary),
        (
            *selected,
            "{included-alt}",
            included_source.as_str(),
            &included,
        ),
    ] {
        let alt = image.alt().ok_or("missing alt")?;
        for node in alt.inlines() {
            let location = parsed.resolve_location(node.location());
            assert_eq!(location.file, Some(file.canonicalize()?));
            let start = source.find(authored).ok_or("missing reference")?;
            if file == &primary {
                assert_eq!(
                    (
                        location.location.absolute_start,
                        location.location.absolute_end
                    ),
                    (start, start + authored.len() - 1)
                );
            }
            let prefix = source.get(..start).ok_or("invalid authored boundary")?;
            let line = prefix.bytes().filter(|&byte| byte == b'\n').count() + 1;
            let column = prefix
                .rsplit('\n')
                .next()
                .ok_or("missing line")?
                .chars()
                .count()
                + 1;
            assert_eq!(usize::try_from(location.location.start.line)?, line);
            assert_eq!(usize::try_from(location.location.end.line)?, line);
            assert_eq!(usize::try_from(location.location.start.column)?, column);
            assert_eq!(
                usize::try_from(location.location.end.column)?,
                column + authored.chars().count() - 1
            );
        }
    }
    let direct_file = root.join("includes/compatibility_image_direct.adoc");
    let direct_source = fs::read_to_string(&direct_file)?;
    let alt = direct.alt().ok_or("missing direct include alt")?;
    let first = alt.inlines().next().ok_or("empty direct alt")?;
    let location = parsed.resolve_location(first.location());
    assert_eq!(location.file, Some(direct_file.canonicalize()?));
    assert_eq!(
        location.location.absolute_start,
        direct_source.find("Direct").ok_or("missing direct value")?
    );
    for (image, authored) in [
        (*positional, "π *Positional* label"),
        (*nested, "Cell π *label*"),
    ] {
        let start = primary_source
            .find(authored)
            .ok_or("missing literal value")?;
        let alt = image.alt().ok_or("missing alt")?;
        for node in alt.inlines() {
            let location = node.location();
            assert!(location.absolute_start() >= start);
            assert!(location.absolute_end() < start + authored.len());
        }
        assert_eq!(
            alt.inlines()
                .next()
                .ok_or("empty alt")?
                .location()
                .absolute_start(),
            start
        );
    }
    Ok(())
}

#[rstest::rstest]
fn preceding_alt_expansion_uses_its_declaration_snapshot(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source =
        ":other: Before\n:word: \\{other}\n\n[alt='{word}']\n:other: After\nimage::photo.png[]\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let image = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            if let Block::Image(image) = block {
                Some(image)
            } else {
                None
            }
        })
        .ok_or("missing image")?;
    let alt = image.alt.as_ref().ok_or("missing alt")?;
    if mode == CompatibilityMode::Draft {
        assert!(matches!(alt.as_ref(), [InlineNode::RawText(text)] if text.content == "{other}"));
    } else {
        assert!(matches!(alt.as_ref(), [InlineNode::PlainText(text)] if text.content == "Before"));
    }
    Ok(())
}

#[rstest::rstest]
fn compatible_block_alt_resolves_replacements_without_enabling_markup(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "image::bird.png[alt=\"π \\] (C) -> -- *literal* footnote:unused[Note]\"]\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Image(image)) = parsed.document().blocks.first() else {
        return Err("expected block image".into());
    };
    assert_eq!(parsed.document().footnotes.len(), 0);
    let alt = image.alt.as_ref().ok_or("missing alt")?;
    if mode == CompatibilityMode::Draft {
        assert!(matches!(alt.as_ref(), [InlineNode::RawText(_)]));
    } else {
        assert!(alt.iter().any(|node| matches!(node, InlineNode::Symbol(_))));
        assert!(
            alt.iter()
                .all(|node| matches!(node, InlineNode::PlainText(_) | InlineNode::Symbol(_)))
        );
        for node in alt {
            let location = node.location();
            let authored = source
                .get(location.absolute_start..=location.absolute_end)
                .ok_or("invalid source span")?;
            if matches!(node, InlineNode::Symbol(_)) {
                assert!(
                    matches!(authored.trim(), "(C)" | "->" | "--"),
                    "{authored:?}"
                );
            }
        }
    }
    Ok(())
}
