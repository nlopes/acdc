use std::{io::Cursor, path::Path};

use acdc_parser::{
    Block, CompatibilityMode, DelimitedBlockType, InlineMacro, InlineNode, Options, Parser,
    SafeMode, Substitution, parse, parse_file, parse_from_reader, parse_inline,
};

type Error = Box<dyn std::error::Error>;

const ALTERNATING_MODES: [CompatibilityMode; 4] = [
    CompatibilityMode::Draft,
    CompatibilityMode::Asciidoctor,
    CompatibilityMode::Draft,
    CompatibilityMode::Asciidoctor,
];

fn assert_named_title(
    title: &[InlineNode<'_>],
    mode: CompatibilityMode,
    word: &str,
) -> Result<(), Error> {
    match (mode, title) {
        (CompatibilityMode::Asciidoctor, [InlineNode::BoldText(bold)]) => {
            assert!(
                matches!(bold.content.as_slice(), [InlineNode::PlainText(text)] if text.content == word)
            );
        }
        (CompatibilityMode::Draft, [InlineNode::RawText(text)]) => {
            assert_eq!(text.content, format!("*{word}*"));
            assert_eq!(text.subs, [Substitution::SpecialChars]);
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
            let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
                return Err("missing paragraph".into());
            };
            assert_named_title(&block.title, mode, "Title")?;
            let header = parsed.document().header.as_ref().ok_or("missing header")?;
            assert_eq!(
                header.authors.first().ok_or("missing author")?.first_name,
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
                Block::DocumentAttribute(_),
                Block::Paragraph(listing),
                Block::DelimitedBlock(table),
                Block::Image(image),
                Block::Paragraph(link),
            ] = parsed.document().blocks.as_slice()
            else {
                return Err(format!("unexpected blocks: {:?}", parsed.document().blocks).into());
            };
            assert_named_title(&listing.title, mode, "Outer")?;
            let title_location =
                parsed.source_location(listing.title.first().ok_or("missing title")?.location());
            assert_eq!(
                title_location.file,
                Some(root.join("includes/compatibility_nested_contexts.adoc"))
            );
            assert_eq!(title_location.location.start.line, 4);
            let DelimitedBlockType::DelimitedTable(table) = &table.inner else {
                return Err("missing table".into());
            };
            let cells = &table.rows.first().ok_or("missing row")?.columns;
            for (cell, word) in cells.iter().zip(["Inner", "Outer"]) {
                let Some(Block::Paragraph(paragraph)) = cell.content.last() else {
                    return Err("missing cell paragraph".into());
                };
                assert_named_title(&paragraph.title, mode, word)?;
            }
            assert_eq!(cells.len(), 2);
            let sibling = cells.last().ok_or("missing sibling")?;
            let Some(Block::Paragraph(paragraph)) = sibling.content.last() else {
                return Err("missing sibling paragraph".into());
            };
            assert!(
                matches!(paragraph.content.as_slice(), [InlineNode::PlainText(text)] if text.content == "Cell two: {cell-word}.")
            );
            assert!(!parsed.document().attributes.contains_key("cell-word"));
            assert_named_title(&image.title, mode, "Outer")?;
            assert!(matches!(
                image.alt.as_ref().ok_or("missing alt")?.as_ref(),
                [InlineNode::BoldText(_)]
            ));
            assert!(matches!(link.content.as_slice(), [InlineNode::Macro(_)]));
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
        let [Block::Paragraph(paragraph)] = block.document().blocks.as_slice() else {
            return Err("missing paragraph".into());
        };
        assert_eq!(direct.inlines(), fluent.inlines());
        assert_eq!(direct.inlines(), paragraph.content);
        let json = serde_json::to_value(direct.inlines())?;
        assert_eq!(
            json.pointer("/0/alt/0/variant"),
            Some(&serde_json::json!("strong"))
        );
        assert_eq!(
            json.pointer("/0/tooltip/0/variant"),
            Some(&serde_json::json!("emphasis"))
        );
        let Some(InlineNode::Macro(InlineMacro::Link(link))) = direct.inlines().last() else {
            return Err("missing link".into());
        };
        assert!(matches!(link.text.as_slice(), [InlineNode::BoldText(_)]));
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
            let [Block::DelimitedBlock(block)] = parsed.document().blocks.as_slice() else {
                return Err("missing listing".into());
            };
            assert_eq!(
                matches!(block.title.first(), Some(InlineNode::BoldText(_))),
                quoted || mode == CompatibilityMode::Asciidoctor
            );
            assert!(block.title.is_present());
            if !quoted && mode == CompatibilityMode::Draft {
                assert!(
                    matches!(block.title.as_ref(), [InlineNode::RawText(text)] if text.content == "*Title*" && text.subs == [Substitution::SpecialChars])
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
        let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
            return Err("missing paragraph".into());
        };
        assert_eq!(block.metadata.roles, ["a", "b"]);
        assert_eq!(block.metadata.options, ["one", "two", "three"]);
    }
    let parsed = parse("[role=a,role=b]\nText.", &Options::default())?;
    assert_eq!(
        parsed
            .document()
            .blocks
            .first()
            .ok_or("missing block")?
            .metadata()
            .ok_or("missing metadata")?
            .roles,
        ["b"]
    );
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
    let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
        return Err("missing paragraph".into());
    };
    assert!(
        matches!(block.title.as_ref(), [InlineNode::RawText(text)] if text.content == "expanded footnote:[Literal]" && text.subs == [Substitution::SpecialChars])
    );
    assert_eq!(parsed.document().footnotes, []);
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
        let metadata = parsed
            .document()
            .blocks
            .first()
            .ok_or("missing block")?
            .metadata()
            .ok_or("missing metadata")?;
        assert_eq!(metadata.roles, ["a", "b"]);
        assert_eq!(metadata.options, ["one", "two"]);
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
        let block = parsed.document().blocks.first().ok_or("missing block")?;
        let title = if let Block::Paragraph(block) = block {
            &block.title
        } else if let Block::Image(block) = block {
            &block.title
        } else if let Block::DelimitedBlock(block) = block {
            &block.title
        } else {
            return Err(format!("unexpected block: {block:?}").into());
        };
        assert!(matches!(title.as_ref(), [InlineNode::RawText(text)]
            if text.content == value && text.subs == [Substitution::SpecialChars]));
        assert_eq!(parsed.document().footnotes, []);
    }
    Ok(())
}

#[test]
fn named_titles_keep_one_reference_pass_and_the_declaration_snapshot() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for quote in ['\'', '"'] {
            let source = format!(
                ":inner: Before\n:outer: \\{{inner}}\n\n[title={quote}{{outer}} / {{inner}}{quote}]\n:inner: After\nBody."
            );
            let parsed = parse(&source, &options)?;
            let Block::Paragraph(block) = parsed.document().blocks.last().ok_or("missing block")?
            else {
                return Err("missing paragraph".into());
            };
            let mut text = String::new();
            for node in &block.title {
                if let InlineNode::PlainText(plain) = node {
                    text.push_str(plain.content);
                } else if let InlineNode::RawText(raw) = node {
                    text.push_str(raw.content);
                } else {
                    return Err(format!("unexpected inline: {node:?}").into());
                }
            }
            assert_eq!(text, "{inner} / Before", "{mode:?}: {source}");
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
    let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
        return Err("missing paragraph".into());
    };
    assert!(matches!(block.title.as_ref(), [InlineNode::BoldText(_)]));
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
        let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
            return Err("missing paragraph".into());
        };
        assert_eq!(
            matches!(block.title.first(), Some(InlineNode::BoldText(_))),
            mode == CompatibilityMode::Asciidoctor
        );
    }
    Ok(())
}
