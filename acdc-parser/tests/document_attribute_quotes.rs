//! Reference labels retain the literal document values at their source position.

use acdc_parser::{
    CompatibilityMode, Options, XrefCaptionLabel, XrefSignifier, XrefStyle,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn reference_snapshots_preserve_document_quote_characters(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("", "\"Label π\"", "'Label π'", "''", "\"α\"tail", "\"π")] value: &str,
) -> Result<(), Error> {
    let input = format!(
        ":table-caption: {value}\n:section-refsig: {value}\n:sectnums:\n\nxref:table[xrefstyle=short] xref:section[xrefstyle=short]\n\n:table-caption: Later\n:section-refsig: Later\n\n[[table]]\n.A table\n|===\n|Cell\n|===\n\n[[section]]\n== One\n\nBody.\n"
    );
    let parsed = parse(
        &input,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(paragraph) = parsed
        .document_ref()
        .blocks()
        .next()
        .map(BlockRef::kind)
        .ok_or("paragraph")?
    else {
        return Err("paragraph".into());
    };
    let refs = paragraph
        .content()
        .inlines()
        .filter_map(|node| {
            if let InlineKind::CrossReference(xref) = node.kind() {
                Some(xref)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [table, section] = refs.as_slice() else {
        return Err("two references".into());
    };
    let expected = if value.is_empty() {
        XrefCaptionLabel::NumberOnly
    } else {
        XrefCaptionLabel::AtReference(value)
    };
    assert_eq!(table.caption_label(), expected);
    assert_eq!(section.signifier(), XrefSignifier::AtReference(value));
    Ok(())
}

#[rstest::rstest]
fn xref_styles_do_not_reinterpret_literal_quotes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let input = ":xrefstyle: \"short\"\n:sectnums:\n\nSee <<section>> and xref:section[] and xref:section[xrefstyle=short] and xref:section[xrefstyle=\"'short'\"].\n\n[[section]]\n== Title\n\nBody.\n";
    let parsed = parse(
        input,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(paragraph) = parsed
        .document_ref()
        .blocks()
        .next()
        .map(BlockRef::kind)
        .ok_or("paragraph")?
    else {
        return Err("paragraph".into());
    };
    let styles = paragraph
        .content()
        .inlines()
        .filter_map(|node| {
            if let InlineKind::CrossReference(xref) = node.kind() {
                Some(xref.xrefstyle())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        styles,
        [
            XrefStyle::Basic,
            XrefStyle::Basic,
            XrefStyle::Short,
            XrefStyle::Basic
        ]
    );
    Ok(())
}
