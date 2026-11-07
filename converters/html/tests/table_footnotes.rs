use acdc_converters_core::Options;
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{CompatibilityMode, Options as ParserOptions, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn cell_notes_have_unique_destinations_and_repeated_links(
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = include_str!("fixtures/source/html/embedded/table_cell_footnotes.adoc");
    let parsed = parse(
        source,
        &ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let processor =
        Processor::new_with_variant(Options::default(), ParserOptions::builder(), variant)?;
    let output = processor.convert_to_string(
        parsed.document(),
        &RenderOptions {
            embedded: true,
            ..RenderOptions::default()
        },
    )?;
    let mut seen = std::collections::HashSet::new();
    for id in output
        .split(" id=\"")
        .skip(1)
        .filter_map(|tail| tail.split('"').next())
    {
        assert!(seen.insert(id.to_owned()), "duplicate {id}");
    }
    for target in output
        .split(" href=\"#")
        .skip(1)
        .filter_map(|tail| tail.split('"').next())
    {
        assert!(seen.contains(target), "missing {target}");
    }
    for number in 1..=6 {
        let prefix = if variant == HtmlVariant::Semantic {
            "_footnote_"
        } else {
            "_footnotedef_"
        };
        assert!(seen.contains(&format!("{prefix}{number}")));
        assert_eq!(
            output
                .matches(&format!("id=\"_footnoteref_{number}\""))
                .count(),
            1
        );
    }
    for body in [
        "Outernote.",
        "Firstnote.",
        "Nestednote.",
        "Secondnote.",
        "Afternote.",
        "Normalnote.",
    ] {
        assert_eq!(output.matches(body).count(), 1);
    }
    Ok(())
}
