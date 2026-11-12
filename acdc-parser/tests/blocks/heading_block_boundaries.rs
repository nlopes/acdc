use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn plain_text(content: ContentRef<'_>) -> String {
    content
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::Plain(text) = inline.kind() {
                Some(text.text())
            } else {
                None
            }
        })
        .collect()
}

#[rstest::rstest]
fn atx_headings_do_not_interrupt_paragraphs(
    #[values("=", "#")] marker: &str,
    #[values("\n", "\r\n")] newline: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for depth in 2..=6 {
        for separator in [" ", "\t", "  ", " \t "] {
            let heading = format!("{}{separator}Heading", marker.repeat(depth));
            let source = format!("= Doc{newline}{newline}Before.{newline}{heading}{newline}After.");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err(format!("expected one paragraph: {source:?}").into());
            };
            let expected = format!("Before.\n{heading}\nAfter.");
            // Longer hash runs can contain inline highlight markup.
            if marker == "=" || depth == 2 {
                assert_eq!(plain_text(paragraph.content()), expected, "{source:?}");
            }
            let location = paragraph.location();
            assert_eq!(location.start().line(), 3);
            assert_eq!(location.end().line(), 5);
            assert_eq!(location.end().column(), 6);
            assert_eq!(
                parsed
                    .preprocessed_source()
                    .get(location.absolute_start()..=location.absolute_end()),
                Some(expected.as_str()),
                "{source:?}"
            );
            assert_eq!(parsed.document().toc_entries().count(), 0);
            assert!(parsed.document().references().get("_heading").is_none());
            assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        }
    }
    Ok(())
}

#[rstest::rstest]
#[case::unordered("* item\n", "item\n", 1)]
#[case::ordered(". item\n", "item\n", 1)]
#[case::description("Term:: item\n", "item\n", 1)]
#[case::empty_description("Term::\n", "", 1)]
#[case::empty_description_with_gap("Term::\n\n", "", 1)]
#[case::callout("----\nx <1>\n----\n<1> item\n", "item\n", 1)]
#[case::nested("* parent\n** item\n", "item\n", 2)]
fn atx_headings_stay_in_list_principal_text(
    #[case] prefix: &str,
    #[case] principal_prefix: &str,
    #[case] items: usize,
    #[values("==", "##", "====", "======")] marker: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("= Doc\n\n{prefix}{marker} Heading\nAfter.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let principals = parsed
        .document()
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::ListItem(item) = node {
                Some(item.principal())
            } else if let StructuralRef::CalloutListItem(item) = node {
                Some(item.principal())
            } else if let StructuralRef::DescriptionListDefinition(definition) = node {
                definition.principal()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(principals.len(), items, "{source}");
    let principal = principals.last().ok_or("missing principal")?;
    assert_eq!(
        plain_text(*principal),
        format!("{principal_prefix}{marker} Heading\nAfter."),
        "{source}"
    );
    let end = principal
        .inlines()
        .last()
        .ok_or("missing inline")?
        .location();
    assert_eq!(end.end().line(), u32::try_from(source.lines().count())?);
    assert_eq!(end.end().column(), 6);
    assert_eq!(parsed.document().toc_entries().count(), 0);
    assert!(parsed.document().references().get("_heading").is_none());
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn atx_headings_stay_in_attached_paragraphs(
    #[values("", "Before.\n")] before: &str,
    #[values("==", "##", "====", "======")] marker: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("* item\n+\n{before}{marker} Heading\nAfter.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::UnorderedList(list)) =
        parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("missing list".into());
    };
    assert_eq!(parsed.document().blocks().count(), 1);
    let item = list.items().next().ok_or("missing item")?;
    let [BlockKind::Paragraph(paragraph)] = &*item.blocks().map(BlockRef::kind).collect::<Vec<_>>()
    else {
        return Err("expected one attached paragraph".into());
    };
    assert_eq!(
        plain_text(paragraph.content()),
        format!("{before}{marker} Heading\nAfter.")
    );
    assert_eq!(
        paragraph.location().end().line(),
        u32::try_from(source.lines().count())?
    );
    assert_eq!(parsed.document().toc_entries().count(), 0);
    Ok(())
}

#[rstest::rstest]
#[case::blank_line("Before.\n\n")]
#[case::anchor("Before.\n[[heading]]\n")]
#[case::attributes("Before.\n[#heading]\n")]
#[case::block_title("Before.\n\n.Title\n")]
#[case::comment("Before.\n\n// comment\n")]
#[case::listing("----\ncode\n----\n")]
#[case::previous_section("== Previous\n")]
#[case::list("* item\n\n")]
fn atx_headings_at_block_boundaries_remain_sections(
    #[case] prefix: &str,
    #[values("==", "##")] marker: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("= Doc\n\n{prefix}{marker} Heading\nAfter.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let section = parsed
        .document()
        .blocks()
        .find_map(|block| {
            let BlockKind::Section(section) = block.kind() else {
                return None;
            };
            section
                .title()
                .is_some_and(|title| plain_text(title) == "Heading")
                .then_some(section)
        })
        .ok_or_else(|| format!("missing section: {source}"))?;
    assert_eq!(section.level(), 1);
    assert!(
        parsed
            .document()
            .references()
            .get(section.id().ok_or("missing ID")?.as_str())
            .is_some()
    );
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn heading_like_lines_in_section_text_do_not_change_the_section_hierarchy(
    #[values("==", "===", "##")] marker: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("= Doc\n\n== Parent\n\nBefore.\n{marker} Heading\nAfter.\n\n== Sibling\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().blocks().count(), 2);
    let Some(BlockKind::Section(parent)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("missing parent section".into());
    };
    let [BlockKind::Paragraph(paragraph)] =
        &*parent.blocks().map(BlockRef::kind).collect::<Vec<_>>()
    else {
        return Err("expected one paragraph inside the parent section".into());
    };
    assert_eq!(
        plain_text(paragraph.content()),
        format!("Before.\n{marker} Heading\nAfter.")
    );
    assert_eq!(parsed.document().toc_entries().count(), 2);
    assert!(parsed.document().references().get("_heading").is_none());
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn included_heading_like_text_keeps_original_unicode_spans(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = tempfile::tempdir()?;
    let main = root.path().join("main.adoc");
    let child = root.path().join("child.adoc");
    let source = "Excluded.\nBefore π.\n== Heading\nAfter.\nExcluded tail.\n";
    std::fs::write(&child, source)?;
    std::fs::write(&main, "include::child.adoc[lines=2..4]\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one included paragraph".into());
    };
    let location = paragraph.location();
    assert_eq!(location.start().line(), 2);
    assert_eq!(location.end().line(), 4);
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
        Some("Before π.\n== Heading\nAfter.")
    );
    Ok(())
}
