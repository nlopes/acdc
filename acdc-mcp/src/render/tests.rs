use super::{Format, render_many};

#[test]
fn media_and_ui_content_use_the_current_parser_model() -> Result<(), acdc_parser::Error> {
    let source = "= Rendering\n:experimental:\n\nbtn:[*Save*] kbd:[Ctrl+S] menu:File[Open > *Recent*] image:icon.svg[*Inline*]\n\nimage::figure.svg[*Block*]\n\nvideo::first,second,third[youtube]";
    let parsed = acdc_parser::parse(source, &acdc_parser::Options::default())?;
    let plain = render_many(&parsed.document().blocks, Format::Plain);
    assert_eq!(
        plain,
        "Save Ctrl+S File > Open > Recent Inline (icon.svg)\n\n*Block* (figure.svg)\n\nfirst,second,third"
    );
    let adoc = render_many(&parsed.document().blocks, Format::Asciidoc);
    for expected in [
        "**Save**",
        "Ctrl+S",
        "File > Open > **Recent**",
        "image:icon.svg[Inline]",
        "image::figure.svg[*Block*]",
        "video::first,second,third[]",
    ] {
        assert!(adoc.contains(expected), "missing {expected:?} in {adoc:?}");
    }
    Ok(())
}

#[test]
fn resolved_symbols_retain_text_in_both_formats() {
    let content = [acdc_parser::InlineNode::Symbol(acdc_parser::Symbol {
        kind: acdc_parser::SymbolKind::Copyright,
        location: acdc_parser::Location::default(),
    })];
    for format in [Format::Plain, Format::Asciidoc] {
        assert_eq!(super::inline_text(&content, format), "©");
    }
}

#[test]
fn character_references_preserve_asciidoc_and_decode_plain_text() -> Result<(), &'static str> {
    use acdc_parser::{CharacterReference, CharacterReferenceKind, InlineNode, Location};
    for (kind, spelling, text) in [
        (CharacterReferenceKind::Numeric(169), "&#169;", "©"),
        (CharacterReferenceKind::Named("amp"), "&amp;", "&"),
        (
            CharacterReferenceKind::Named("NotEqualTilde"),
            "&NotEqualTilde;",
            "≂̸",
        ),
        (
            CharacterReferenceKind::Named("unknown"),
            "&unknown;",
            "&unknown;",
        ),
        (
            CharacterReferenceKind::Numeric(0xd800),
            "&#55296;",
            "&#55296;",
        ),
    ] {
        let content = [InlineNode::CharacterReference(
            CharacterReference::new(kind, Location::default()).ok_or("invalid reference")?,
        )];
        assert_eq!(super::inline_text(&content, Format::Asciidoc), spelling);
        assert_eq!(super::inline_text(&content, Format::Plain), text);
    }
    Ok(())
}
