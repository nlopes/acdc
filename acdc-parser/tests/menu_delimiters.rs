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
#[case("", "Open > Close", &["Open", "Close"])]
#[case("", "Open &gt; Close", &["Open", "Close"])]
#[case("", "Open &#62; Close", &["Open &#62; Close"])]
#[case("", "Open &#x3e; Close", &["Open &#62; Close"])]
#[case("", "Open &amp;gt; Close", &["Open &amp;gt; Close"])]
#[case("[subs=macros]\n", "Open > Close", &["Open > Close"])]
#[case("[subs=macros]\n", "Open &gt; Close", &["Open", "Close"])]
#[case("[subs=\"specialchars,macros\"]\n", "Open > Close", &["Open", "Close"])]
#[case("[subs=\"specialchars,macros\"]\n", "Open &gt; Close", &["Open &gt; Close"])]
#[case("[subs=\"replacements,specialchars,macros\"]\n", "Open &gt; Close", &["Open &gt; Close"])]
#[case("[subs=\"specialchars,replacements,macros\"]\n", "Open &gt; Close", &["Open", "Close"])]
#[case("[subs=macros]\n", "Open > Close,Save", &["Open > Close", "Save"])]
#[case("", "Open &gt; pass:[A &gt; B]", &["Open", "A &gt; B"])]
#[case("", "Open &gt; ", &["Open", ""])]
fn menu_separators_follow_the_macro_stage(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] settings: &str,
    #[case] items: &str,
    #[case] expected: &[&str],
) -> Result<(), Error> {
    let source = format!(":experimental:\n\n{settings}menu:File[{items}]");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let menu = parsed
        .document_ref()
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
fn entity_width_does_not_shift_unicode_labels_or_registered_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n\nπ menu:File[Öpen &gt; Closé footnote:n[Note.]]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document_ref().footnotes().count(), 1);
    let note = parsed
        .document_ref()
        .footnotes()
        .next()
        .ok_or("missing note")?;
    let authored = "footnote:n[Note.]";
    let start = source.find(authored).ok_or("missing source")?;
    assert_eq!(
        (
            note.location().absolute_start(),
            note.location().absolute_end()
        ),
        (start, start + authored.len() - 1)
    );
    let menu = parsed
        .document_ref()
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
    let [open, close] = &*menu.items().collect::<Vec<_>>() else {
        return Err("expected two items".into());
    };
    let start = source.find("Öpen").ok_or("missing first item")?;
    let node = open.inlines().next().ok_or("missing first item node")?;
    assert_eq!(
        (
            node.location().absolute_start(),
            node.location().absolute_end()
        ),
        (start, start + "Öpen".len() - 1)
    );
    let start = source.find("Closé").ok_or("missing second item")?;
    let node = close.inlines().next().ok_or("missing second item node")?;
    assert_eq!(node.location().absolute_start(), start);
    Ok(())
}
