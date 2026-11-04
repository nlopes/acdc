use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn first_section_number(source: &str, options: &Options<'_>) -> Result<Option<String>, Error> {
    let parsed = parse(source, options)?;
    let Some(BlockKind::Section(section)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected a section".into());
    };
    Ok(section.number().map(str::to_owned))
}

#[test]
fn explicit_sectnums_unset_masks_numbered_alias() -> Result<(), Error> {
    let number = first_section_number(
        "= T\n:numbered: all\n:sectnums!:\n\n== A\n",
        &Options::default(),
    )?;

    assert_eq!(number, None);
    Ok(())
}

#[test]
fn sectnums_presence_masks_numbered_all_value() -> Result<(), Error> {
    let number = first_section_number(
        "= T\n:numbered: all\n:sectnums:\n\n[preface]\n== A\n",
        &Options::default(),
    )?;

    assert_eq!(number, None);
    Ok(())
}

#[test]
fn explicit_hardbreaks_option_unset_masks_hardbreaks_alias() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("hardbreaks", true)
        .build()?;
    let parsed = parse("= T\n:hardbreaks-option!:\n\nfirst\nsecond\n", &options)?;
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected a paragraph".into());
    };

    assert!(
        paragraph
            .content()
            .inlines()
            .all(|inline| !matches!(inline.kind(), InlineKind::LineBreak(_)))
    );
    Ok(())
}

#[test]
fn literal_true_attribute_value_remains_text() -> Result<(), Error> {
    let parsed = parse("= T\n:a: true\n\nX{a}X\n", &Options::default())?;
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected a paragraph".into());
    };

    assert!(matches!(
        paragraph.content().inlines().map(InlineRef::kind).collect::<Vec<_>>().as_slice(),
        [InlineKind::Plain(text)] if text.text() == "XtrueX"
    ));
    Ok(())
}
