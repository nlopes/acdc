use acdc_parser::{
    Block, CompatibilityMode, ContentAttribute, InlineMacro, InlineNode, Options, parse,
};

type Error = Box<dyn std::error::Error>;

fn paragraph(parsed: &acdc_parser::ParseResult) -> Result<&[InlineNode<'_>], Error> {
    parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            if let Block::Paragraph(paragraph) = block {
                Some(paragraph.content.as_slice())
            } else {
                None
            }
        })
        .ok_or_else(|| "missing paragraph".into())
}

fn text(content: &ContentAttribute<'_>) -> Result<String, Error> {
    content
        .iter()
        .map(|node| {
            if let InlineNode::PlainText(text) = node {
                Ok(text.content)
            } else if let InlineNode::RawText(text) = node {
                Ok(text.content)
            } else {
                Err(format!("expected text: {node:?}").into())
            }
        })
        .collect()
}

#[rstest::rstest]
#[case("Ctrl+Alt", &["Ctrl", "Alt"])]
#[case("Ctrl,Alt", &["Ctrl", "Alt"])]
#[case("Ctrl++", &["Ctrl", "+"])]
#[case("Ctrl,,", &["Ctrl", ","])]
#[case("+", &["+"])]
#[case(",", &[","])]
#[case("Ctrl+", &["Ctrl+"])]
#[case("Ctrl+++", &["Ctrl", "", "+"])]
#[case("Ctrl+Alt,Del", &["Ctrl", "Alt,Del"])]
#[case("Ctrl,Alt+Del", &["Ctrl", "Alt+Del"])]
#[case("Ctrl+,", &["Ctrl", ","])]
#[case("Ctrl+\\]", &["Ctrl", "]"])]
#[case("Ctrl+pass:[a+b]", &["Ctrl", "a+b"])]
fn keyboard_key_boundaries_match_the_selected_separator(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] keys: &str,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let source = format!(":experimental:\n\nkbd:[{keys}]\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(InlineNode::Macro(InlineMacro::Keyboard(keyboard))) = paragraph(&parsed)?.first()
    else {
        return Err("missing keyboard".into());
    };
    assert_eq!(
        keyboard
            .keys
            .iter()
            .map(text)
            .collect::<Result<Vec<_>, _>>()?,
        expected
    );
    if keys == "Ctrl+\\]" {
        let node = keyboard
            .keys
            .last()
            .and_then(|key| key.first())
            .ok_or("missing bracket key")?;
        let start = source.find("\\]").ok_or("missing authored bracket")?;
        assert_eq!(
            (node.location().absolute_start, node.location().absolute_end),
            (start, start + 1)
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case("Open > Recent > Document", &["Open", "Recent", "Document"])]
#[case("Open,Save", &["Open", "Save"])]
#[case("Open>,Save", &["Open", ",Save"])]
#[case("Open>", &["Open"])]
#[case("Open > ", &["Open", ""])]
#[case("", &[])]
#[case("one \\] two > Close", &["one ] two", "Close"])]
#[case("pass:[a,b] > Close", &["a,b", "Close"])]
fn menu_items_keep_nested_and_escaped_boundaries(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] items: &str,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let source = format!(":experimental:\n\nmenu:File[{items}]\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(InlineNode::Macro(InlineMacro::Menu(menu))) = paragraph(&parsed)?.first() else {
        return Err("missing menu".into());
    };
    assert_eq!(text(&menu.target)?, "File");
    assert_eq!(
        menu.items.iter().map(text).collect::<Result<Vec<_>, _>>()?,
        expected
    );
    Ok(())
}

#[rstest::rstest]
fn nested_ui_macros_register_their_content_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n\nkbd:[Ctrl+footnote:key[A+B]] and menu:File[Open footnote:menu[A > B] > Close].";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed
            .document()
            .footnotes
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        [Some("key"), Some("menu")]
    );
    let notes = &parsed.document().footnotes;
    for (note, authored) in notes
        .iter()
        .zip(["footnote:key[A+B]", "footnote:menu[A > B]"])
    {
        let start = source.find(authored).ok_or("missing authored note")?;
        assert_eq!(
            (note.location.absolute_start, note.location.absolute_end),
            (start, start + authored.len() - 1)
        );
    }
    Ok(())
}

#[rstest::rstest]
fn button_labels_keep_escaped_brackets_inside_the_macro(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n\nbtn:[Save \\] Close]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(InlineNode::Macro(InlineMacro::Button(button))) = paragraph(&parsed)?.first() else {
        return Err("missing button".into());
    };
    assert_eq!(text(&button.label)?, "Save ] Close");
    assert_eq!(button.location.absolute_end, source.len() - 1);
    Ok(())
}

#[rstest::rstest]
fn menu_separators_do_not_split_nested_reference_syntax(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n\nmenu:File[Item <<target>> > Close]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(InlineNode::Macro(InlineMacro::Menu(menu))) = paragraph(&parsed)?.first() else {
        return Err("missing menu".into());
    };
    let [item, close] = menu.items.as_slice() else {
        return Err("expected two menu items".into());
    };
    assert!(item.iter().any(|node| matches!(node, InlineNode::Macro(InlineMacro::CrossReference(reference)) if reference.target == "target")));
    assert_eq!(text(close)?, "Close");
    Ok(())
}
