use acdc_converters_core::Options;
use acdc_converters_html::{Converter, HtmlVariant, RenderOptions};
use acdc_parser::{CompatibilityMode, Document, Options as ParserOptions, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("* [x] Checked\n* [ ] Unchecked\n* Ordinary\n", (0, 0, 2, 1))]
#[case("[%interactive]\n* [x] Checked\n* [ ] Unchecked\n* Ordinary\n", (2, 1, 0, 0))]
#[case("[%interactive]\n* [x] Parent\n** [x] Child\n** [ ] Child empty\n* [ ] Parent empty\n", (2, 1, 2, 1))]
#[case("* [x] Parent\n+\n[%interactive]\n** [x] Child\n** [ ] Child empty\n", (2, 1, 1, 1))]
#[case("[%interactive]\n* [x] Parent\n+\n[%interactive]\n** [x] Child\n** [ ] Child empty\n", (3, 2, 0, 0))]
#[case("[.interactive]\n* [x] Checked\n* [ ] Unchecked\n", (0, 0, 2, 1))]
#[case("= List\n:interactive:\n\n* [x] Checked\n* [ ] Unchecked\n", (0, 0, 2, 1))]
#[case("[%interactive]\n* [X] Uppercase\n* [*] Star\n", (2, 2, 0, 0))]
#[case("[options=interactive]\n* [x] Checked\n* [ ] Unchecked\n", (2, 1, 0, 0))]
#[case("[opts=interactive]\n* [x] Checked\n* [ ] Unchecked\n", (2, 1, 0, 0))]
#[case("[.choices%interactive]\n* [x] Checked\n* [ ] Unchecked\n* Ordinary\n", (2, 1, 0, 0))]
#[case("Term::\n+\n[%interactive]\n* [x] Attached\n* [ ] Attached empty\n", (2, 1, 0, 0))]
fn interactive_checkboxes_are_local_and_preserve_document_data(
    #[case] source: &str,
    #[case] counts: (usize, usize, usize, usize),
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let (enabled, enabled_checked, static_boxes, static_checked) = counts;
    let parsed = parse(
        source,
        &ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let before = Document::from(parsed.document());
    let converter = Converter::new_with_variant(
        Options::default(),
        ParserOptions::builder().with_compatibility_mode(mode),
        variant,
    )?;
    let output = converter.convert_to_string(
        parsed.document(),
        &RenderOptions {
            embedded: true,
            ..RenderOptions::default()
        },
    )?;
    assert_eq!(Document::from(parsed.document()), before);
    assert_eq!(output.matches("data-item-complete=").count(), enabled);
    assert_eq!(
        output.matches("data-item-complete=\"1\"").count(),
        enabled_checked
    );
    if variant == HtmlVariant::Semantic {
        assert_eq!(output.matches("<input ").count(), enabled + static_boxes);
        assert_eq!(output.matches(" disabled").count(), static_boxes);
        assert_eq!(
            output.matches(" checked>").count(),
            enabled_checked + static_checked
        );
    } else {
        assert_eq!(output.matches("<input ").count(), enabled);
        assert!(!output.contains(" disabled"));
        assert_eq!(output.matches(" checked>").count(), enabled_checked);
        assert_eq!(output.matches("&#10003;").count(), static_checked);
        assert_eq!(
            output.matches("&#10063;").count(),
            static_boxes - static_checked
        );
    }
    if source.contains("Ordinary") {
        assert!(output.contains("Ordinary"));
    }
    if source.contains(".choices") {
        assert!(output.contains(" choices\""));
    }
    Ok(())
}
