use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, ContentRef, FootnoteDefinitionRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn paragraph(parsed: &acdc_parser::ParseResult) -> Result<ContentRef<'_>, Error> {
    parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                Some(paragraph.content())
            } else {
                None
            }
        })
        .ok_or_else(|| "missing paragraph".into())
}

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    content
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(text) = node.kind() {
                Ok(text.text())
            } else if let InlineKind::Raw(text) = node.kind() {
                Ok(text.text())
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
#[case("[π]", &["[π]"])]
#[case("\\[π\\]", &["\\[π]"])]
#[case("[π\\]x]", &["[π]x]"])]
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
    let Some(InlineKind::KeyboardShortcut(keyboard)) =
        paragraph(&parsed)?.inlines().next().map(InlineRef::kind)
    else {
        return Err("missing keyboard".into());
    };
    assert_eq!(
        keyboard.keys().map(text).collect::<Result<Vec<_>, _>>()?,
        expected
    );
    if keys == "Ctrl+\\]" {
        let node = keyboard
            .keys()
            .last()
            .and_then(|key| key.inlines().next())
            .ok_or("missing bracket key")?;
        let start = source.find("\\]").ok_or("missing authored bracket")?;
        assert_eq!(
            (
                node.location().absolute_start(),
                node.location().absolute_end()
            ),
            (start, start + 1)
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::plain("π")]
#[case::preprocessed("π+")]
fn keyboard_deep_brackets_do_not_overflow_the_stack(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("", "Before ")] prefix: &str,
    #[case] inner: &str,
) -> Result<(), Error> {
    let label = format!("{}{inner}{}", "[".repeat(20_000), "]".repeat(20_000));
    let source = format!(":experimental:\n\n{prefix}kbd:[{label}] after.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let content = paragraph(&parsed)?;
    let keyboard = content
        .inlines()
        .find_map(|node| {
            if let InlineKind::KeyboardShortcut(keyboard) = node.kind() {
                Some(keyboard)
            } else {
                None
            }
        })
        .ok_or("missing keyboard")?;
    assert_eq!(
        keyboard.keys().map(text).collect::<Result<Vec<_>, _>>()?,
        [label]
    );
    let start = source.find("kbd:[").ok_or("missing authored keyboard")?;
    assert_eq!(keyboard.location().absolute_start(), start);
    assert_eq!(
        keyboard.location().absolute_end(),
        source.len() - " after.".len() - 1
    );
    let last = content.inlines().last().ok_or("missing trailing text")?;
    assert!(matches!(last.kind(), InlineKind::Plain(value) if value.text() == " after."));
    Ok(())
}

#[rstest::rstest]
#[case::empty("")]
#[case::unclosed_nested("[")]
#[case::escaped_close("\\]")]
#[case::closed_nested("[π]")]
fn keyboard_unclosed_brackets_remain_literal(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
) -> Result<(), Error> {
    let literal = format!("Before kbd:[{body}");
    let source = format!(":experimental:\n\n{literal}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(text(paragraph(&parsed)?)?, literal);
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
    let Some(InlineKind::MenuPath(menu)) =
        paragraph(&parsed)?.inlines().next().map(InlineRef::kind)
    else {
        return Err("missing menu".into());
    };
    assert_eq!(text(menu.target())?, "File");
    assert_eq!(
        menu.items().map(text).collect::<Result<Vec<_>, _>>()?,
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
            .footnotes()
            .map(FootnoteDefinitionRef::id)
            .collect::<Vec<_>>(),
        [Some("key"), Some("menu")]
    );
    let notes = parsed.document().footnotes();
    for (note, authored) in notes.zip(["footnote:key[A+B]", "footnote:menu[A > B]"]) {
        let start = source.find(authored).ok_or("missing authored note")?;
        assert_eq!(
            (
                note.location().absolute_start(),
                note.location().absolute_end()
            ),
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
    let Some(InlineKind::Button(button)) =
        paragraph(&parsed)?.inlines().next().map(InlineRef::kind)
    else {
        return Err("missing button".into());
    };
    assert_eq!(text(button.label())?, "Save ] Close");
    assert_eq!(button.location().absolute_end(), source.len() - 1);
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
    let Some(InlineKind::MenuPath(menu)) =
        paragraph(&parsed)?.inlines().next().map(InlineRef::kind)
    else {
        return Err("missing menu".into());
    };
    let [item, close] = &*menu.items().collect::<Vec<_>>() else {
        return Err("expected two menu items".into());
    };
    assert!(item.inlines().any(|node| matches!(node.kind(), InlineKind::CrossReference(reference) if reference.target() == "target")));
    assert_eq!(text(*close)?, "Close");
    Ok(())
}
