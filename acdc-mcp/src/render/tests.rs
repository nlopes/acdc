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
