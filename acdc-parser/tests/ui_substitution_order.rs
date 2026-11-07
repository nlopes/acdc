use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, ContentRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> String {
    content
        .inlines()
        .filter_map(|node| {
            if let InlineKind::Plain(node) = node.kind() {
                Some(node.text().to_owned())
            } else if let InlineKind::Raw(node) = node.kind() {
                Some(node.text().to_owned())
            } else if let InlineKind::CharacterReference(node) = node.kind() {
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
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                paragraph.content().inlines().find_map(|node| {
                    if let InlineKind::Keyboard(keyboard) = node.kind() {
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
    assert_eq!(keyboard.keys().map(text).collect::<Vec<_>>(), expected);
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
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                paragraph.content().inlines().find_map(|node| {
                    if let InlineKind::Menu(menu) = node.kind() {
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
    assert_eq!(menu.items().map(text).collect::<Vec<_>>(), expected);
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
        .blocks()
        .filter_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                Some(paragraph.content().inlines())
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|node| {
            if let InlineKind::Keyboard(keyboard) = node.kind() {
                Some(keyboard.keys().map(text).collect::<Vec<_>>())
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
    for block in parsed.document().blocks() {
        if let BlockKind::Paragraph(paragraph) = block.kind() {
            for node in paragraph.content().inlines() {
                if let InlineKind::Keyboard(keyboard) = node.kind() {
                    for key in keyboard.keys() {
                        for node in key.inlines() {
                            locations.push((
                                node.location().absolute_start(),
                                node.location().absolute_end(),
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
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                paragraph.content().inlines().find_map(|node| {
                    if let InlineKind::Keyboard(keyboard) = node.kind() {
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
    let [key] = &*keyboard.keys().collect::<Vec<_>>() else {
        return Err("expected one retained key".into());
    };
    assert!(
        key.inlines()
            .any(|node| matches!(node.kind(), InlineKind::Bold(_)))
    );
    assert!(
        key.inlines()
            .any(|node| matches!(node.kind(), InlineKind::Raw(raw) if raw.text() == "+Alt"))
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
    assert_eq!(parsed.document().footnotes().count(), count);
    Ok(())
}
