use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, BlockRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn a_later_description_marker_does_not_change_an_ordinary_paragraph(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "Wrapped paragraph.\nTerm:: Still paragraph text.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().blocks().count(), 1);
    assert!(matches!(
        parsed.document().blocks().next().map(BlockRef::kind),
        Some(BlockKind::Paragraph(_))
    ));
    Ok(())
}

#[rstest::rstest]
fn an_ordinary_paragraph_between_terms_ends_the_description_list(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "First:: One\n\nWrapped paragraph.\nSecond:: Still paragraph text.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().blocks().count(), 2);
    let Some(BlockKind::DescriptionList(list)) =
        parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected description list".into());
    };
    assert_eq!(list.items().count(), 1);
    assert!(matches!(
        parsed.document().blocks().nth(1).map(BlockRef::kind),
        Some(BlockKind::Paragraph(_))
    ));
    Ok(())
}

#[test]
fn wrapped_included_text_keeps_its_original_unicode_range() -> Result<(), Error> {
    let root = tempfile::tempdir()?;
    let main = root.path().join("main.adoc");
    let child = root.path().join("child.adoc");
    let source = "Excluded.\nWrapped π text.\nTerm:: Still paragraph text.\nExcluded tail.\n";
    std::fs::write(&child, source)?;
    std::fs::write(&main, "include::child.adoc[lines=2..3]\n")?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &main,
            &Options::builder()
                .with_compatibility_mode(mode)
                .with_safe_mode(SafeMode::Unsafe)
                .build()?,
        )?;
        let Some(BlockKind::Paragraph(paragraph)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected included paragraph".into());
        };
        assert_eq!(paragraph.location().start().line(), 2);
        assert_eq!(paragraph.location().end().line(), 3);
        assert_eq!(
            paragraph.location().absolute_start(),
            source.find("Wrapped").ok_or("missing start")?
        );
        assert_eq!(
            paragraph.location().absolute_end(),
            source.find("text.\nExcluded").ok_or("missing end")? + "text".len()
        );
    }
    Ok(())
}
