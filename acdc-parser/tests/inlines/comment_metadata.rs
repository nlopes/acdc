use acdc_parser::{
    CompatibilityMode, Options, SafeMode, WarningKind,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::delimited("////\nHidden footnote:[Hidden].\n////\n", 4)]
#[case::blank_lines("\n////\nHidden.\n////\n\n", 5)]
#[case::line("\n// Hidden footnote:[Hidden].\n", 3)]
#[case::different_length("/////\n////\nHidden.\n/////\n", 5)]
fn metadata_crosses_comments_without_becoming_comment_metadata(
    #[case] comment: &str,
    #[case] comment_end: u32,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("[.attached]\n{comment}Visible footnote:[Selected].\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let blocks: Vec<_> = parsed.document().blocks().collect();
    assert_eq!(blocks.len(), 2);
    let [comment, visible] = blocks.as_slice() else {
        return Err("comment and visible block".into());
    };
    assert!(matches!(
        comment.kind(),
        BlockKind::Comment(_) | BlockKind::DelimitedComment(_)
    ));
    assert_eq!(comment.location().end().line(), comment_end);
    assert!(
        comment
            .attributes()
            .is_none_or(|attributes| attributes.get("role").is_none())
    );
    let BlockKind::Paragraph(paragraph) = visible.kind() else {
        return Err("visible paragraph".into());
    };
    assert_eq!(paragraph.roles().iter().collect::<Vec<_>>(), ["attached"]);
    assert_eq!(parsed.document().footnotes().count(), 1);
    let Some(InlineKind::Plain(plain)) = parsed
        .document()
        .footnotes()
        .next()
        .ok_or("selected note")?
        .content()
        .inlines()
        .next()
        .map(InlineRef::kind)
    else {
        return Err("plain note content".into());
    };
    assert_eq!(plain.text(), "Selected");
    Ok(())
}

#[rstest::rstest]
fn hidden_discrete_markers_do_not_change_section_ownership(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "= Document\n\n== First\n\n[.second]\n////\n[discrete]\n////\n== Second\n\n[discrete]\n////\nHidden.\n////\n=== Floating\n\nBody.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let sections: Vec<_> = parsed
        .document()
        .blocks()
        .filter_map(|block| {
            if let BlockKind::Section(section) = block.kind() {
                Some(section)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(sections.len(), 2);
    let [first, second] = sections.as_slice() else {
        return Err("two sections".into());
    };
    assert!(first.blocks().next().is_none());
    assert_eq!(second.roles().iter().collect::<Vec<_>>(), ["second"]);
    assert_eq!(
        second
            .blocks()
            .filter(|block| matches!(block.kind(), BlockKind::DiscreteHeader(_)))
            .count(),
        1
    );
    assert_eq!(parsed.document().toc_entries().count(), 2);
    Ok(())
}

#[rstest::rstest]
#[case::closed("////\nHidden.\n////\n", false)]
#[case::unterminated("////\nHidden.\n", true)]
fn unused_titles_have_no_inline_effects_but_comments_survive(
    #[case] comment: &str,
    #[case] unterminated: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("[.unused]\n.Unused footnote:[Hidden]\n{comment}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().footnotes().count(), 0);
    assert_eq!(parsed.document().blocks().count(), 1);
    assert!(matches!(
        parsed.document().blocks().next().map(BlockRef::kind),
        Some(BlockKind::DelimitedComment(_))
    ));
    assert_eq!(
        parsed
            .warnings()
            .iter()
            .filter(|warning| matches!(
                warning.kind,
                WarningKind::UnterminatedDelimitedBlock { .. }
            ))
            .count(),
        usize::from(unterminated)
    );
    Ok(())
}

#[rstest::rstest]
fn adjacent_line_comments_keep_the_existing_preprocessor_policy(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[.attached]\n// Hidden footnote:[Hidden].\nVisible.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().blocks().count(), 1);
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("visible paragraph".into());
    };
    assert_eq!(paragraph.roles().iter().collect::<Vec<_>>(), ["attached"]);
    assert_eq!(parsed.document().footnotes().count(), 0);
    Ok(())
}

#[rstest::rstest]
#[case::selected_title("metadata", 1)]
#[case::nested_contexts("contexts", 1)]
#[case::trailing_metadata("eof", 0)]
#[case::unterminated("unterminated", 0)]
fn comment_fixtures_register_only_selected_visible_content(
    #[case] name: &str,
    #[case] notes: usize,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests")
        .join(format!("compatibility_comment_{name}.adoc"));
    let parsed = parse_file(
        path,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    assert_eq!(parsed.document().footnotes().count(), notes);
    Ok(())
}

#[rstest::rstest]
fn a_styled_comment_body_does_not_discard_its_delimited_prefix(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[comment]\n////\nDelimited hidden.\n////\nParagraph hidden.\n\nVisible.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let blocks: Vec<_> = parsed.document().blocks().map(BlockRef::kind).collect();
    assert!(matches!(
        blocks.as_slice(),
        [
            BlockKind::DelimitedComment(_),
            BlockKind::Comment(_),
            BlockKind::Paragraph(_)
        ]
    ));
    Ok(())
}
