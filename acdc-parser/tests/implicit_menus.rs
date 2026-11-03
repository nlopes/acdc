use acdc_parser::{
    Block, CompatibilityMode, ContentAttribute, InlineMacro, InlineNode, Options, parse,
};

type Error = Box<dyn std::error::Error>;

fn text(content: &ContentAttribute<'_>) -> String {
    content
        .iter()
        .filter_map(|node| {
            if let InlineNode::PlainText(node) = node {
                Some(node.content)
            } else if let InlineNode::RawText(node) = node {
                Some(node.content)
            } else {
                None
            }
        })
        .collect()
}

#[rstest::rstest]
#[case("", "\"File > Open\"", vec![vec!["File", "Open"]])]
#[case("", "\"File > Open > Recent\"", vec![vec!["File", "Open", "Recent"]])]
#[case("", "\"File &gt; Open\"", vec![vec!["File", "Open"]])]
#[case("", "\"File &#62; Open\"", vec![])]
#[case("", "\"File &#x3e; Open\"", vec![])]
#[case("", "\"File&gt;Open\"", vec![])]
#[case("", "\"File>Open\"", vec![])]
#[case("", "\" File > Open\"", vec![])]
#[case("", "\"File\t>\tOpen\"", vec![])]
#[case("", "\"File\n > Open\"", vec![vec!["File", "Open"]])]
#[case("", "\"Édition > Ouvrir\"", vec![vec!["Édition", "Ouvrir"]])]
#[case("", "\"́Menu > Open\"", vec![vec!["́Menu", "Open"]])]
#[case("", "\"&File > Open\"", vec![vec!["&File", "Open"]])]
#[case("", "\"*File* > Open\"", vec![])]
#[case("", "\\\"File > Open\"", vec![])]
#[case("", "\\\\\"File > Open\"", vec![])]
#[case("", "\"File > pass:[A > B]\"", vec![vec!["File", "A > B"]])]
#[case("", "\"File pass:[A > B] Open\"", vec![])]
#[case("", "\"File footnote:n[A > B] Open\"", vec![])]
#[case("[subs=macros]\n", "\"File > Open\"", vec![])]
#[case("[subs=macros]\n", "\"File &gt; Open\"", vec![vec!["File", "Open"]])]
#[case("[subs=\"specialchars,macros\"]\n", "\"File > Open\"", vec![vec!["File", "Open"]])]
#[case("[subs=\"specialchars,macros\"]\n", "\"File &gt; Open\"", vec![])]
#[case("[subs=\"replacements,specialchars,macros\"]\n", "\"File &gt; Open\"", vec![])]
#[case("[subs=\"specialchars,replacements,macros\"]\n", "\"File &gt; Open\"", vec![vec!["File", "Open"]])]
#[case("[subs=none]\n", "\"File > Open\"", vec![])]
#[case("", "\"File > Open\" and \"Edit > Copy\"", vec![vec!["File", "Open"], vec!["Edit", "Copy"]])]
fn quoted_menus_follow_the_macro_stage(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] settings: &str,
    #[case] body: &str,
    #[case] expected: Vec<Vec<&str>>,
) -> Result<(), Error> {
    let source = format!(":experimental:\n\n{settings}{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let actual = parsed
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
            if let InlineNode::Macro(InlineMacro::Menu(menu)) = node {
                Some(
                    std::iter::once(text(&menu.target))
                        .chain(menu.items.iter().map(text))
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    Ok(())
}

#[rstest::rstest]
#[case("", true)]
#[case(
    "[subs=\"macros,attributes,quotes,replacements,specialchars\"]\n",
    false
)]
fn later_attributes_cannot_create_the_quoted_menu_form(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] settings: &str,
    #[case] expected: bool,
) -> Result<(), Error> {
    for body in ["\"{path}\"", "{quoted}", "\"File{gap}> Open\""] {
        let source = format!(
            ":experimental:\n:path: File > Open\n:quoted: \"File > Open\"\n:gap: {{sp}}\n\n{settings}{body}"
        );
        let parsed = parse(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let found = parsed.document().blocks.iter().any(|block| {
            if let Block::Paragraph(paragraph) = block {
                paragraph
                    .content
                    .iter()
                    .any(|node| matches!(node, InlineNode::Macro(InlineMacro::Menu(_))))
            } else {
                false
            }
        });
        assert_eq!(found, expected, "{source}");
    }
    Ok(())
}

#[rstest::rstest]
fn quoted_menus_share_the_existing_ui_availability(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "\"File > Open\"",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Block::Paragraph(paragraph) = parsed.document().blocks.first().ok_or("missing block")?
    else {
        return Err("missing paragraph".into());
    };
    assert_eq!(
        paragraph
            .content
            .iter()
            .any(|node| matches!(node, InlineNode::Macro(InlineMacro::Menu(_)))),
        mode == CompatibilityMode::Draft
    );
    Ok(())
}

#[rstest::rstest]
fn quoted_menu_labels_keep_rich_content_and_source_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":experimental:\n\nπ \"Édition *Menu* > *Ouvrir* footnote:n[Note.]\".";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    let Block::Paragraph(paragraph) = parsed.document().blocks.first().ok_or("missing block")?
    else {
        return Err("missing paragraph".into());
    };
    let menu = paragraph
        .content
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::Menu(menu)) = node {
                Some(menu)
            } else {
                None
            }
        })
        .ok_or("missing menu")?;
    assert!(
        menu.target
            .iter()
            .any(|node| matches!(node, InlineNode::BoldText(_)))
    );
    assert!(
        menu.items
            .first()
            .ok_or("missing menu item")?
            .iter()
            .any(|node| matches!(node, InlineNode::BoldText(_)))
    );
    let start = source.find('"').ok_or("missing opening quote")?;
    let end = source.rfind('"').ok_or("missing closing quote")?;
    assert_eq!(
        (menu.location.absolute_start, menu.location.absolute_end),
        (start, end)
    );
    let note = parsed
        .document()
        .footnotes
        .first()
        .ok_or("missing footnote")?;
    let start = source.find("footnote:").ok_or("missing note")?;
    assert_eq!(
        (note.location.absolute_start, note.location.absolute_end),
        (start, start + "footnote:n[Note.]".len() - 1)
    );
    Ok(())
}
