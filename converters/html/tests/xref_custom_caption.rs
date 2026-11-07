use acdc_converters_core::Options;
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{CompatibilityMode, Document, Options as ParserOptions, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn copied_caption_content_keeps_single_ids_and_index_occurrences(
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "= Captions\n:acdc-index:\n\n\
        Before: xref:target[xrefstyle=short] / xref:target[xrefstyle=full].\n\n\
        [#target,caption='[#caption-mark]*Mark* link:https://example.org[site,id=caption-link] ((Visible)) (((Hidden))) footnote:n[One note]. ']\n\
        .Title\n====\nBody.\n====\n\n\
        After: xref:target[xrefstyle=short] / xref:target[xrefstyle=full].\n\n\
        [index]\n== Index\n";
    let parsed = parse(
        source,
        &ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let before = Document::from(parsed.document());
    let processor = Processor::new_with_variant(
        Options::default(),
        ParserOptions::builder().with_compatibility_mode(mode),
        variant,
    )?;
    let output = processor.convert_to_string(
        parsed.document(),
        &RenderOptions {
            embedded: true,
            ..RenderOptions::default()
        },
    )?;
    assert_eq!(Document::from(parsed.document()), before);
    assert_eq!(parsed.document().footnotes().count(), 1);
    for id in ["caption-mark", "caption-link", "_footnoteref_1"] {
        assert_eq!(
            output.matches(&format!("id=\"{id}\"")).count(),
            1,
            "{id}: {output}"
        );
    }
    assert_eq!(output.matches("id=\"_indexterm_").count(), 2, "{output}");
    assert!(!output.contains("href=\"#target\"></a>"), "{output}");
    Ok(())
}
