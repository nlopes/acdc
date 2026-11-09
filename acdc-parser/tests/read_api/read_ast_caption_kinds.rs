use acdc_parser::{CaptionKind, CompatibilityMode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("== Heading\n\nText.")]
#[case("[discrete]\n== Heading")]
#[case("NOTE: Text.")]
#[case("* Item")]
#[case(". Item")]
#[case("<1> Callout")]
#[case("Term:: Description")]
#[case("audio::sound.ogg[]")]
#[case("video::movie.mp4[]")]
#[case("toc::[]")]
#[case("<<<")]
#[case("'''")]
#[case("// Comment")]
#[case("Before.\n\n:setting: value")]
fn other_block_kinds_do_not_gain_a_caption_context(
    #[case] body: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("= Doc\n\n{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document().blocks().last().ok_or("block")?;
    assert_eq!(CaptionKind::for_block(block), None);
    Ok(())
}
