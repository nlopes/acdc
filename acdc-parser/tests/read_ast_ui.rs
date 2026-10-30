use acdc_parser::{
    CompatibilityMode, ContentAttribute, InlineMacro, InlineNode, Keyboard, Location, Options,
    SafeMode, StemNotation,
    ast::{
        BlockKind, ButtonRef, ContentRef, DocumentRef, InlineKind, KeyboardRef, MenuRef, StemRef,
    },
    parse, parse_file, parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn shared<T: Copy + Send + Sync>(_: T) {}

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    content
        .inlines()
        .map(|inline| {
            if let InlineKind::Plain(node) = inline.kind() {
                Ok(node.text().to_owned())
            } else if let InlineKind::Bold(node) = inline.kind() {
                text(node.content())
            } else if let InlineKind::Italic(node) = inline.kind() {
                text(node.content())
            } else {
                Err("expected text or formatting".into())
            }
        })
        .collect()
}

fn paragraphs(document: DocumentRef<'_>) -> impl Iterator<Item = ContentRef<'_>> {
    document.blocks().filter_map(|block| {
        if let BlockKind::Paragraph(paragraph) = block.kind() {
            Some(paragraph.content())
        } else {
            None
        }
    })
}

#[rstest::rstest]
fn ui_reads_preserve_structured_labels_and_source_ranges(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n\né btn:[*Save*] kbd:[*Ctrl*+S] menu:*File*[_Open_ > Recent]\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let content = paragraphs(parsed.document_ref())
        .next()
        .ok_or("paragraph")?;
    let mut families = 0;
    for inline in content.inlines() {
        if let InlineKind::Button(button) = inline.kind() {
            shared(button);
            assert_eq!(text(button.label())?, "Save");
            let label = button.label().inlines().next().ok_or("label")?;
            assert!(matches!(label.kind(), InlineKind::Bold(_)));
            assert_eq!(
                source.get(label.location().absolute_start()..=label.location().absolute_end()),
                Some("*Save*")
            );
            assert_eq!(button.location().start().column(), 3);
            families += 1;
        } else if let InlineKind::Keyboard(keyboard) = inline.kind() {
            shared(keyboard);
            let keys = {
                let copy = keyboard;
                copy.keys()
            };
            assert_eq!(
                keys.map(text).collect::<Result<Vec<_>, _>>()?,
                ["Ctrl", "S"]
            );
            assert!(matches!(
                keyboard
                    .keys()
                    .next()
                    .ok_or("key")?
                    .inlines()
                    .next()
                    .ok_or("key text")?
                    .kind(),
                InlineKind::Bold(_)
            ));
            families += 1;
        } else if let InlineKind::Menu(menu) = inline.kind() {
            shared(menu);
            assert_eq!(text(menu.target())?, "File");
            assert!(matches!(
                menu.target().inlines().next().ok_or("target")?.kind(),
                InlineKind::Bold(_)
            ));
            assert_eq!(
                menu.items().map(text).collect::<Result<Vec<_>, _>>()?,
                ["Open", "Recent"]
            );
            let first = menu
                .items()
                .next()
                .ok_or("first item")?
                .inlines()
                .next()
                .ok_or("item text")?;
            assert!(matches!(first.kind(), InlineKind::Italic(_)));
            assert_eq!(
                source.get(first.location().absolute_start()..=first.location().absolute_end()),
                Some("_Open_")
            );
            families += 1;
        }
    }
    assert_eq!(families, 3);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn ui_payloads_keep_attribute_like_text_and_empty_values(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":experimental:\n\nbtn:[Save,role=red] kbd:[Ctrl,role=red] menu:File[Open > role=red]\n\nbtn:[] menu:File[]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut paragraphs = paragraphs(parsed.document_ref());
    let mut found = Vec::new();
    for inline in paragraphs.next().ok_or("paragraph")?.inlines() {
        if let InlineKind::Button(button) = inline.kind() {
            found.push(text(button.label())?);
        } else if let InlineKind::Keyboard(keyboard) = inline.kind() {
            found.extend(keyboard.keys().map(text).collect::<Result<Vec<_>, _>>()?);
        } else if let InlineKind::Menu(menu) = inline.kind() {
            found.extend(menu.items().map(text).collect::<Result<Vec<_>, _>>()?);
        }
    }
    assert_eq!(
        found,
        ["Save,role=red", "Ctrl", "role=red", "Open", "role=red"]
    );
    let mut empty = 0;
    for inline in paragraphs.next().ok_or("empty paragraph")?.inlines() {
        if let InlineKind::Button(button) = inline.kind() {
            assert!(button.label().is_empty());
            empty += 1;
        } else if let InlineKind::Menu(menu) = inline.kind() {
            assert!(menu.items().next().is_none());
            empty += 1;
        }
    }
    assert_eq!(empty, 2);
    let none = Keyboard::new(Vec::new(), Location::default());
    let one = Keyboard::new(vec![ContentAttribute::default()], Location::default());
    assert!(KeyboardRef::from(&none).keys().next().is_none());
    let mut keys = KeyboardRef::from(&one).keys();
    assert!(keys.next().ok_or("present empty key")?.is_empty());
    assert!(keys.next().is_none());
    Ok(())
}

#[rstest::rstest]
#[case("stem", StemNotation::Latexmath)]
#[case("latexmath", StemNotation::Latexmath)]
#[case("asciimath", StemNotation::Asciimath)]
fn formula_reads_preserve_protected_text_and_selected_notation(
    #[case] prefix: &str,
    #[case] notation: StemNotation,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source =
        format!(":stem: tex\n:value: expanded\n\né {prefix}:[{{value}} *x* + y,role=red]\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let formula = paragraphs(parsed.document_ref())
        .next()
        .ok_or("paragraph")?
        .inlines()
        .find_map(|inline| {
            if let InlineKind::Stem(formula) = inline.kind() {
                Some(formula)
            } else {
                None
            }
        })
        .ok_or("formula")?;
    shared(formula);
    assert_eq!(formula.text(), "{value} *x* + y,role=red");
    assert_eq!(formula.notation(), notation);
    assert_eq!(formula.location().start().column(), 3);
    assert_eq!(
        source.get(formula.location().absolute_start()..=formula.location().absolute_end()),
        Some(format!("{prefix}:[{{value}} *x* + y,role=red]").as_str())
    );
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn constructed_and_cloned_owners_use_the_same_reads() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("experimental", "")
        .build()?;
    let parsed = parse_inline(
        "btn:[Label] kbd:[Ctrl+S] menu:File[Open] stem:[x]",
        &options,
    )?;
    let mut families = 0;
    for inline in parsed.inlines() {
        if let InlineNode::Macro(InlineMacro::Button(owner)) = inline {
            let mut owner = owner.clone();
            owner.label = ContentAttribute::default();
            assert!(ButtonRef::from(&owner).label().is_empty());
            families += 1;
        } else if let InlineNode::Macro(InlineMacro::Keyboard(owner)) = inline {
            assert_eq!(
                KeyboardRef::from(owner)
                    .keys()
                    .map(text)
                    .collect::<Result<Vec<_>, _>>()?,
                ["Ctrl", "S"]
            );
            families += 1;
        } else if let InlineNode::Macro(InlineMacro::Menu(owner)) = inline {
            let mut owner = owner.clone();
            owner.target = ContentAttribute::default();
            owner.items = vec![ContentAttribute::default()];
            assert!(MenuRef::from(&owner).target().is_empty());
            assert!(
                MenuRef::from(&owner)
                    .items()
                    .next()
                    .ok_or("empty item")?
                    .is_empty()
            );
            families += 1;
        } else if let InlineNode::Macro(InlineMacro::Stem(owner)) = inline {
            let mut owner = owner.clone();
            owner.content = "";
            owner.notation = StemNotation::Latexmath;
            let read = StemRef::from(&owner);
            assert_eq!(read.text(), "");
            assert_eq!(read.notation(), StemNotation::Latexmath);
            families += 1;
        }
    }
    assert_eq!(families, 4);
    Ok(())
}

#[rstest::rstest]
fn payload_reads_preserve_includes_and_table_cells(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("ui.adoc");
    std::fs::write(
        &main,
        ":experimental:\n:stem: latexmath\n\ninclude::ui.adoc[]\n\n[cols=a]\n|===\n|menu:File[Open] stem:[x]\n|===\n",
    )?;
    std::fs::write(&include, "é btn:[*Save*] kbd:[Ctrl+S]\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut count = 0;
    for inline in paragraphs(parsed.document_ref())
        .next()
        .ok_or("included paragraph")?
        .inlines()
    {
        if let InlineKind::Button(button) = inline.kind() {
            assert_eq!(
                parsed.resolve_location(button.location()).file,
                Some(include.clone())
            );
            let label = button.label().inlines().next().ok_or("label")?;
            assert_eq!(
                parsed.resolve_location(label.location()).file,
                Some(include.clone())
            );
            count += 1;
        } else if let InlineKind::Keyboard(keyboard) = inline.kind() {
            assert_eq!(keyboard.keys().count(), 2);
            assert_eq!(
                parsed.resolve_location(keyboard.location()).file,
                Some(include.clone())
            );
            count += 1;
        }
    }
    let BlockKind::Table(table) = parsed.document_ref().blocks().nth(1).ok_or("table")?.kind()
    else {
        return Err("table".into());
    };
    let cell = table
        .rows()
        .next()
        .ok_or("row")?
        .cells()
        .next()
        .ok_or("cell")?;
    let BlockKind::Paragraph(paragraph) = cell.blocks().next().ok_or("paragraph")?.kind() else {
        return Err("cell paragraph".into());
    };
    for inline in paragraph.content().inlines() {
        if let InlineKind::Menu(menu) = inline.kind() {
            assert_eq!(text(menu.target())?, "File");
            assert_eq!(menu.items().count(), 1);
            assert_eq!(
                parsed.resolve_location(menu.location()).file,
                Some(main.clone())
            );
            count += 1;
        } else if let InlineKind::Stem(formula) = inline.kind() {
            assert_eq!(formula.text(), "x");
            assert_eq!(formula.notation(), StemNotation::Latexmath);
            assert_eq!(
                parsed.resolve_location(formula.location()).file,
                Some(main.clone())
            );
            count += 1;
        }
    }
    assert_eq!(count, 4);
    Ok(())
}
