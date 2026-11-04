use acdc_parser::{
    Block, CompatibilityMode, ContentAttribute, InlineMacro, InlineNode, Options, parse,
};

type Error = Box<dyn std::error::Error>;

fn text(content: &ContentAttribute<'_>) -> String {
    content
        .iter()
        .filter_map(|node| {
            if let InlineNode::PlainText(node) = node {
                Some(node.content.to_owned())
            } else if let InlineNode::RawText(node) = node {
                Some(node.content.to_owned())
            } else if let InlineNode::CharacterReference(node) = node {
                Some(node.kind().to_string())
            } else {
                None
            }
        })
        .collect()
}

#[rstest::rstest]
#[case("", "Ctrl+Alt", &["Ctrl", "Alt"])]
#[case("", "Ctrl,Alt", &["Ctrl", "Alt"])]
#[case("", "pass:q[Ctrl+Alt]", &["Ctrl+Alt"])]
#[case("", "Ctrl++", &["Ctrl", "+"])]
#[case("", "Ctrl+pass:[a+b]", &["Ctrl", "pass:[a+b]"])]
#[case("[subs=\"macros,attributes,quotes,replacements\"]\n", "Ctrl+Alt", &["Ctrl+Alt"])]
#[case("[subs=\"macros,attributes,quotes,replacements\"]\n", "Ctrl,Alt", &["Ctrl,Alt"])]
fn keyboard_boundaries_follow_attribute_substitution_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] settings: &str,
    #[case] keys: &str,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let source = format!(":experimental:\n:keys: {keys}\n\n{settings}kbd:[{{keys}}]\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let keyboard = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            if let Block::Paragraph(paragraph) = block {
                paragraph.content.iter().find_map(|node| {
                    if let InlineNode::Macro(InlineMacro::Keyboard(keyboard)) = node {
                        Some(keyboard)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or("missing keyboard")?;
    assert_eq!(keyboard.keys.iter().map(text).collect::<Vec<_>>(), expected);
    Ok(())
}

#[rstest::rstest]
#[case("", "Open > Close", &["Open", "Close"])]
#[case("", "Open,Close", &["Open", "Close"])]
#[case("[subs=\"macros,attributes,quotes,replacements\"]\n", "Open > Close", &["Open &gt; Close"])]
#[case("[subs=\"macros,attributes,quotes,replacements\"]\n", "Open,Close", &["Open,Close"])]
fn menu_boundaries_follow_attribute_substitution_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] settings: &str,
    #[case] items: &str,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let source = format!(":experimental:\n:items: {items}\n\n{settings}menu:File[{{items}}]\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let menu = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            if let Block::Paragraph(paragraph) = block {
                paragraph.content.iter().find_map(|node| {
                    if let InlineNode::Macro(InlineMacro::Menu(menu)) = node {
                        Some(menu)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or("missing menu")?;
    assert_eq!(menu.items.iter().map(text).collect::<Vec<_>>(), expected);
    Ok(())
}

#[rstest::rstest]
fn keyboard_passthroughs_stay_within_one_macro(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n\nkbd:[+Literal+] and kbd:[+] and kbd:[Ctrl++]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let keys = parsed
        .document()
        .blocks
        .iter()
        .filter_map(|block| {
            if let Block::Paragraph(paragraph) = block {
                Some(paragraph.content.iter())
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|node| {
            if let InlineNode::Macro(InlineMacro::Keyboard(keyboard)) = node {
                Some(keyboard.keys.iter().map(text).collect::<Vec<_>>())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(keys, [vec!["Literal"], vec!["+"], vec!["Ctrl", "+"]]);
    Ok(())
}

#[rstest::rstest]
fn expanded_keys_keep_the_authored_reference_location(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n:keys: Échap+Alt\n\nπ kbd:[{keys}]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let start = source.rfind("{keys}").ok_or("missing source reference")?;
    let mut locations = Vec::new();
    for block in &parsed.document().blocks {
        if let Block::Paragraph(paragraph) = block {
            for node in &paragraph.content {
                if let InlineNode::Macro(InlineMacro::Keyboard(keyboard)) = node {
                    for key in &keyboard.keys {
                        for node in key {
                            locations.push((
                                node.location().absolute_start,
                                node.location().absolute_end,
                            ));
                        }
                    }
                }
            }
        }
    }
    assert_eq!(locations, [(start, start + 5), (start, start + 5)]);
    Ok(())
}

#[rstest::rstest]
fn retained_attribute_profiles_keep_rich_keys_together(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n:keys: pass:q[*Ctrl*+Alt]\n\nkbd:[{keys}]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let keyboard = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            if let Block::Paragraph(paragraph) = block {
                paragraph.content.iter().find_map(|node| {
                    if let InlineNode::Macro(InlineMacro::Keyboard(keyboard)) = node {
                        Some(keyboard)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or("missing keyboard")?;
    let [key] = keyboard.keys.as_slice() else {
        return Err("expected one retained key".into());
    };
    assert!(
        key.iter()
            .any(|node| matches!(node, InlineNode::BoldText(_)))
    );
    assert!(
        key.iter()
            .any(|node| matches!(node, InlineNode::RawText(raw) if raw.content == "+Alt"))
    );
    Ok(())
}

#[rstest::rstest]
#[case("", 1)]
#[case("[subs=\"macros,attributes,quotes,replacements\"]\n", 0)]
fn introduced_ui_content_registers_only_at_its_macro_stage(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] settings: &str,
    #[case] count: usize,
) -> Result<(), Error> {
    let source = format!(
        ":experimental:\n:keys: Ctrl+footnote:note[Note body.]\n\n{settings}kbd:[{{keys}}]\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().footnotes.len(), count);
    Ok(())
}
