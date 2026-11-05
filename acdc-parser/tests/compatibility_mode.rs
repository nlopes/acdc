use std::{io::Cursor, path::Path};

use acdc_parser::{
    CompatibilityMode, Options, Parser, SafeMode, Substitution,
    ast::{BlockKind, BlockRef, ContentRef, DocumentRef, InlineKind, InlineRef},
    parse, parse_file, parse_from_reader, parse_inline,
};

type Error = Box<dyn std::error::Error>;

const ALTERNATING_MODES: [CompatibilityMode; 4] = [
    CompatibilityMode::Draft,
    CompatibilityMode::Asciidoctor,
    CompatibilityMode::Draft,
    CompatibilityMode::Asciidoctor,
];

fn blocks(document: DocumentRef<'_>) -> Vec<BlockKind<'_>> {
    document.blocks().map(BlockRef::kind).collect()
}

fn kinds(content: ContentRef<'_>) -> Vec<InlineKind<'_>> {
    content.inlines().map(InlineRef::kind).collect()
}

fn assert_named_title(
    title: ContentRef<'_>,
    mode: CompatibilityMode,
    word: &str,
) -> Result<(), Error> {
    match (mode, kinds(title).as_slice()) {
        (CompatibilityMode::Asciidoctor, [InlineKind::Bold(bold)]) => {
            assert!(
                matches!(kinds(bold.content()).as_slice(), [InlineKind::Plain(text)] if text.text() == word)
            );
        }
        (CompatibilityMode::Draft, [InlineKind::Raw(text)]) => {
            assert_eq!(text.text(), format!("*{word}*"));
            assert_eq!(
                text.substitutions().collect::<Vec<_>>(),
                [Substitution::SpecialChars]
            );
        }
        _ => return Err(format!("unexpected {mode:?} title: {title:?}").into()),
    }
    Ok(())
}

#[test]
fn entry_points_keep_modes_local_to_each_parse() -> Result<(), Error> {
    let source = "= Context\n:author: Ada Lovelace\n\n[title=\"*Title*\"]\nBody.";
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("context.adoc");
    std::fs::write(&path, source)?;
    for mode in ALTERNATING_MODES {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for parsed in [
            parse(source, &options)?,
            parse_from_reader(Cursor::new(source), &options)?,
            parse_file(&path, &options)?,
            Parser::new(source).with_options(options.clone()).parse()?,
        ] {
            let [BlockKind::Paragraph(block)] = &*blocks(parsed.document()) else {
                return Err("missing paragraph".into());
            };
            assert_named_title(block.title().ok_or("missing title")?, mode, "Title")?;
            let header = parsed.document().header().ok_or("missing header")?;
            assert_eq!(
                header
                    .authors()
                    .next()
                    .ok_or("missing author")?
                    .first_name(),
                "Ada"
            );
            assert_eq!(parsed.warnings(), []);
        }
    }
    Ok(())
}

#[test]
fn includes_and_cells_inherit_mode_without_leaking_local_attributes() -> Result<(), Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let path = root.join("compatibility_nested_contexts.adoc");
    let source = std::fs::read_to_string(&path)?;
    for mode in ALTERNATING_MODES {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .with_base_dir(&root)
            .build()?;
        for parsed in [
            parse_file(&path, &options)?,
            parse(&source, &options)?,
            parse_from_reader(Cursor::new(&source), &options)?,
        ] {
            let [
                BlockKind::DocumentAttribute(_),
                BlockKind::Paragraph(listing),
                BlockKind::Table(table),
                BlockKind::Image(image),
                BlockKind::Paragraph(link),
            ] = &*blocks(parsed.document())
            else {
                return Err(format!(
                    "unexpected blocks: {:?}",
                    parsed.document().blocks().collect::<Vec<_>>()
                )
                .into());
            };
            assert_named_title(listing.title().ok_or("missing title")?, mode, "Outer")?;
            let title_location = parsed.resolve_location(
                listing
                    .title()
                    .ok_or("missing title")?
                    .inlines()
                    .next()
                    .ok_or("missing title inline")?
                    .location(),
            );
            assert_eq!(
                title_location.file,
                Some(root.join("includes/compatibility_nested_contexts.adoc"))
            );
            assert_eq!(title_location.location.start.line, 4);
            let cells = table
                .rows()
                .next()
                .ok_or("missing row")?
                .cells()
                .collect::<Vec<_>>();
            for (cell, word) in cells.iter().zip(["Inner", "Outer"]) {
                let Some(BlockKind::Paragraph(paragraph)) =
                    cell.blocks().last().map(BlockRef::kind)
                else {
                    return Err("missing cell paragraph".into());
                };
                assert_named_title(paragraph.title().ok_or("missing title")?, mode, word)?;
            }
            assert_eq!(cells.len(), 2);
            let sibling = cells.last().ok_or("missing sibling")?;
            let Some(BlockKind::Paragraph(paragraph)) = sibling.blocks().last().map(BlockRef::kind)
            else {
                return Err("missing sibling paragraph".into());
            };
            assert!(
                matches!(kinds(paragraph.content()).as_slice(), [InlineKind::Plain(text)] if text.text() == "Cell two: {cell-word}.")
            );
            assert!(!parsed.document().attributes().contains("cell-word"));
            assert_named_title(image.title().ok_or("missing title")?, mode, "Outer")?;
            let alt = kinds(image.alt().ok_or("missing alt")?);
            if mode == CompatibilityMode::Draft {
                assert!(
                    matches!(alt.as_slice(), [InlineKind::Raw(text)] if text.text() == "*Outer*")
                );
            } else {
                assert!(matches!(alt.as_slice(), [InlineKind::Bold(_)]));
            }
            assert!(matches!(
                kinds(link.content()).as_slice(),
                [InlineKind::Footnote(_)
                    | InlineKind::Icon(_)
                    | InlineKind::Image(_)
                    | InlineKind::Keyboard(_)
                    | InlineKind::Button(_)
                    | InlineKind::Menu(_)
                    | InlineKind::Url(_)
                    | InlineKind::Link(_)
                    | InlineKind::Mailto(_)
                    | InlineKind::Autolink(_)
                    | InlineKind::CrossReference(_)
                    | InlineKind::Pass(_)
                    | InlineKind::Stem(_)
                    | InlineKind::IndexTerm(_)]
            ));
            assert_eq!(parsed.warnings(), []);
        }
    }
    Ok(())
}

#[test]
fn inline_entry_points_keep_content_attribute_and_macro_label_context() -> Result<(), Error> {
    let source = "image:photo.svg[alt='*{word}*',title='_Tip_'] link:https://example.org[*{word}*]";
    for mode in ALTERNATING_MODES {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("word", "Label")
            .build()?;
        let direct = parse_inline(source, &options)?;
        let fluent = Parser::new(source)
            .with_options(options.clone())
            .parse_inline()?;
        let block = parse(source, &options)?;
        let [BlockKind::Paragraph(paragraph)] = &*blocks(block.document()) else {
            return Err("missing paragraph".into());
        };
        assert_eq!(direct.content(), fluent.content());
        assert_eq!(direct.content(), paragraph.content());
        let json = serde_json::to_value(direct.content())?;
        assert_eq!(
            json.pointer("/0/alt/0/variant"),
            Some(&serde_json::json!("strong"))
        );
        assert_eq!(
            json.pointer("/0/tooltip/0/variant"),
            Some(&serde_json::json!("emphasis"))
        );
        let Some(InlineKind::Link(link)) =
            direct.content().inlines().next_back().map(InlineRef::kind)
        else {
            return Err("missing link".into());
        };
        assert!(matches!(
            kinds(link.text()).as_slice(),
            [InlineKind::Bold(_)]
        ));
    }
    Ok(())
}

#[test]
fn named_title_quoting_follows_compatibility_mode() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (attribute, quoted) in [
            ("title=\"*Title*\"", false),
            ("title='*Title*'", true),
            ("title=*Title*", false),
        ] {
            let source = format!("[{attribute}]\n----\nx\n----");
            let parsed = parse(&source, &options)?;
            let [BlockKind::Listing(block)] = &*blocks(parsed.document()) else {
                return Err("missing listing".into());
            };
            assert_eq!(
                matches!(
                    block
                        .title()
                        .and_then(|title| title.inlines().next())
                        .map(InlineRef::kind),
                    Some(InlineKind::Bold(_))
                ),
                quoted || mode == CompatibilityMode::Asciidoctor
            );
            assert!(block.title().is_some());
            if !quoted && mode == CompatibilityMode::Draft {
                assert!(
                    matches!(kinds(block.title().ok_or("missing title")?).as_slice(), [InlineKind::Raw(text)] if text.text() == "*Title*" && text.substitutions().collect::<Vec<_>>() == [Substitution::SpecialChars])
                );
            }
        }
    }
    Ok(())
}

#[test]
fn draft_roles_and_options_accumulate_once_in_source_order() -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(CompatibilityMode::Draft)
        .build()?;
    for source in [
        "[role=a,roles=b,role=a,opts='one two',options='two,three']\nText.",
        "[.a.a.b%one%one%two%three]\nText.",
        "[role=a,opts=one]\n[roles=b,options='two,three']\n[role=a,opts=two]\nText.",
    ] {
        let parsed = parse(source, &options)?;
        let [BlockKind::Paragraph(block)] = &*blocks(parsed.document()) else {
            return Err("missing paragraph".into());
        };
        assert_eq!(block.roles().iter().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(
            block.options().iter().collect::<Vec<_>>(),
            ["one", "two", "three"]
        );
    }
    let parsed = parse("[role=a,role=b]\nText.", &Options::default())?;
    let Some(BlockKind::Paragraph(block)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("missing paragraph".into());
    };
    assert_eq!(block.roles().iter().collect::<Vec<_>>(), ["b"]);
    Ok(())
}

#[test]
fn draft_literal_titles_expand_attributes_without_registering_macros() -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(CompatibilityMode::Draft)
        .build()?;
    let parsed = parse(
        "= T\n:word: expanded\n\n[title=\"{word} footnote:[Literal]\"]\nText.",
        &options,
    )?;
    let [BlockKind::Paragraph(block)] = &*blocks(parsed.document()) else {
        return Err("missing paragraph".into());
    };
    assert!(
        matches!(kinds(block.title().ok_or("missing title")?).as_slice(), [InlineKind::Raw(text)] if text.text() == "expanded footnote:[Literal]" && text.substitutions().collect::<Vec<_>>() == [Substitution::SpecialChars])
    );
    assert_eq!(parsed.document().footnotes().count(), 0);
    let rebuilt = options.into_static().into_builder().build()?;
    assert_eq!(rebuilt.compatibility_mode, CompatibilityMode::Draft);
    Ok(())
}

#[test]
fn draft_media_macro_attributes_accumulate_with_block_metadata() -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(CompatibilityMode::Draft)
        .build()?;
    for media in ["image::photo.svg", "audio::sound.ogg", "video::clip.mp4"] {
        let source = format!("[role=a,opts=one]\n{media}[role=b,role=a,opts='two one']");
        let parsed = parse(&source, &options)?;
        let block = parsed.document().blocks().next().ok_or("missing block")?;
        let (roles, options) = if let BlockKind::Image(image) = block.kind() {
            (image.roles(), image.options())
        } else if let BlockKind::Audio(audio) = block.kind() {
            (audio.roles(), audio.options())
        } else if let BlockKind::Video(video) = block.kind() {
            (video.roles(), video.options())
        } else {
            return Err("expected image, audio or video".into());
        };
        assert_eq!(roles.iter().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(options.iter().collect::<Vec<_>>(), ["one", "two"]);
    }
    Ok(())
}

#[test]
fn draft_literal_titles_protect_typography_passthroughs_and_entities() -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(CompatibilityMode::Draft)
        .build()?;
    let value = "A -- B *bold* ++literal++ &copy; footnote:[Note]";
    for source in [
        format!("[title=\"{value}\"]\nText."),
        format!("image::photo.png[title=\"{value}\"]"),
        format!("[title={value}]\n----\nx\n----"),
    ] {
        let parsed = parse(&source, &options)?;
        let block = parsed
            .document()
            .blocks()
            .next()
            .map(BlockRef::kind)
            .ok_or("missing block")?;
        let title = if let BlockKind::Paragraph(block) = block {
            block.title().ok_or("missing title")?
        } else if let BlockKind::Image(block) = block {
            block.title().ok_or("missing title")?
        } else if let BlockKind::Listing(block) = block {
            block.title().ok_or("missing title")?
        } else {
            return Err(format!("unexpected block: {block:?}").into());
        };
        assert!(matches!(kinds(title).as_slice(), [InlineKind::Raw(text)]
            if text.text() == value && text.substitutions().collect::<Vec<_>>() == [Substitution::SpecialChars]));
        assert_eq!(parsed.document().footnotes().count(), 0);
    }
    Ok(())
}

#[test]
fn named_titles_keep_mode_specific_expansion_and_the_declaration_snapshot() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for quote in ['\'', '"'] {
            let source = format!(
                ":inner: Before\n:outer: \\{{inner}}\n\n[title={quote}{{outer}} / {{inner}}{quote}]\n:inner: After\nBody."
            );
            let parsed = parse(&source, &options)?;
            let BlockKind::Paragraph(block) = parsed
                .document()
                .blocks()
                .last()
                .map(BlockRef::kind)
                .ok_or("missing block")?
            else {
                return Err("missing paragraph".into());
            };
            let mut text = String::new();
            for node in block.title().ok_or("missing title")?.inlines() {
                if let InlineKind::Plain(plain) = node.kind() {
                    text.push_str(plain.text());
                } else if let InlineKind::Raw(raw) = node.kind() {
                    text.push_str(raw.text());
                } else {
                    return Err(format!("unexpected inline: {node:?}").into());
                }
            }
            assert_eq!(
                text,
                if mode == CompatibilityMode::Asciidoctor {
                    "Before / Before"
                } else {
                    "{inner} / Before"
                },
                "{mode:?}: {source}"
            );
        }
    }
    Ok(())
}

#[test]
fn draft_dot_titles_still_parse_inline_content() -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(CompatibilityMode::Draft)
        .build()?;
    let parsed = parse(".*Title*\nText.", &options)?;
    let [BlockKind::Paragraph(block)] = &*blocks(parsed.document()) else {
        return Err("missing paragraph".into());
    };
    assert!(matches!(
        kinds(block.title().ok_or("missing title")?).as_slice(),
        [InlineKind::Bold(_)]
    ));
    Ok(())
}

#[test]
fn compatibility_mode_survives_option_conversions_and_defaults_to_asciidoctor() -> Result<(), Error>
{
    assert_eq!(
        Options::default().compatibility_mode,
        CompatibilityMode::Asciidoctor
    );
    assert_eq!(
        Options::builder().build()?.compatibility_mode,
        CompatibilityMode::Asciidoctor
    );
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        assert_eq!(
            options.clone().into_builder().build()?.compatibility_mode,
            mode
        );
        assert_eq!(options.into_static().compatibility_mode, mode);
    }
    Ok(())
}

#[test]
fn document_attributes_cannot_select_the_compatibility_mode() -> Result<(), Error> {
    let source =
        ":compatibility-mode: draft\n:attribute-syntax: draft\n\n[title=\"*Title*\"]\nBody.";
    for mode in [
        CompatibilityMode::Draft,
        CompatibilityMode::Asciidoctor,
        CompatibilityMode::Draft,
    ] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(source, &options)?;
        let [BlockKind::Paragraph(block)] = &*blocks(parsed.document()) else {
            return Err("missing paragraph".into());
        };
        assert_eq!(
            matches!(
                block
                    .title()
                    .and_then(|title| title.inlines().next())
                    .map(InlineRef::kind),
                Some(InlineKind::Bold(_))
            ),
            mode == CompatibilityMode::Asciidoctor
        );
    }
    Ok(())
}

#[test]
fn compatibility_mode_has_one_configuration_spelling() -> Result<(), Error> {
    for (mode, name) in [
        (CompatibilityMode::Asciidoctor, "asciidoctor"),
        (CompatibilityMode::Draft, "draft"),
    ] {
        assert_eq!(name.parse::<CompatibilityMode>()?, mode);
        assert_eq!(mode.to_string(), name);
        assert_eq!(serde_json::to_value(mode)?, serde_json::json!(name));
        assert_eq!(
            serde_json::from_value::<CompatibilityMode>(serde_json::json!(name))?,
            mode
        );
    }
    for name in ["", "Draft", "spec-only", "draft ", "html"] {
        assert!(name.parse::<CompatibilityMode>().is_err());
        assert!(serde_json::from_value::<CompatibilityMode>(serde_json::json!(name)).is_err());
    }
    assert!(serde_json::from_value::<CompatibilityMode>(serde_json::Value::Null).is_err());
    Ok(())
}
