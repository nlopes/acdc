use acdc_parser::{AttributeSyntax, Block, InlineNode, Options, Substitution, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn draft_title_quoting_is_separate_from_compatibility_and_export() -> Result<(), Error> {
    for syntax in [AttributeSyntax::Asciidoctor, AttributeSyntax::Draft] {
        let options = Options::builder().with_attribute_syntax(syntax).build()?;
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
                quoted || syntax == AttributeSyntax::Asciidoctor
            );
            assert!(block.title.is_present());
            if !quoted && syntax == AttributeSyntax::Draft {
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
        .with_attribute_syntax(AttributeSyntax::Draft)
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
        .with_attribute_syntax(AttributeSyntax::Draft)
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
    assert_eq!(rebuilt.attribute_syntax, AttributeSyntax::Draft);
    Ok(())
}

#[test]
fn draft_media_macro_attributes_accumulate_with_block_metadata() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute_syntax(AttributeSyntax::Draft)
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
        .with_attribute_syntax(AttributeSyntax::Draft)
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
    for syntax in [AttributeSyntax::Asciidoctor, AttributeSyntax::Draft] {
        let options = Options::builder().with_attribute_syntax(syntax).build()?;
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
            assert_eq!(text, "{inner} / Before", "{syntax:?}: {source}");
        }
    }
    Ok(())
}

#[test]
fn draft_dot_titles_still_parse_inline_content() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute_syntax(AttributeSyntax::Draft)
        .build()?;
    let parsed = parse(".*Title*\nText.", &options)?;
    let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
        return Err("missing paragraph".into());
    };
    assert!(matches!(block.title.as_ref(), [InlineNode::BoldText(_)]));
    Ok(())
}

#[test]
fn attribute_policy_survives_option_conversions_and_defaults_to_asciidoctor() -> Result<(), Error> {
    assert_eq!(
        Options::default().attribute_syntax,
        AttributeSyntax::Asciidoctor
    );
    assert_eq!(
        Options::builder().build()?.attribute_syntax,
        AttributeSyntax::Asciidoctor
    );
    for syntax in [AttributeSyntax::Asciidoctor, AttributeSyntax::Draft] {
        let options = Options::builder().with_attribute_syntax(syntax).build()?;
        assert_eq!(
            options.clone().into_builder().build()?.attribute_syntax,
            syntax
        );
        assert_eq!(options.into_static().attribute_syntax, syntax);
    }
    Ok(())
}
