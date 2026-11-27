//! A reusable backend starts each pipeline conversion with fresh render state.

use acdc_converters_core::{Converter as _, Diagnostics, Options, pipeline::Pipeline};
use acdc_converters_html::{Converter, HtmlVariant};
use acdc_parser::{Options as ParserOptions, edit::Document};

#[test]
fn pipeline_repeated_html_conversions_start_fresh_index_counters()
-> Result<(), Box<dyn std::error::Error>> {
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let pipeline = Pipeline::new(Converter::new_with_variant(
            Options::builder().embedded(true).build(),
            ParserOptions::builder(),
            variant,
        )?);
        let source = "= Document\n:acdc-index:\n\n((Term)). indexterm:[Alias,see=Term]\n\n[index]\n== Index\n\n[index]\n== Another Index";
        let parsed = pipeline.parse(source)?;
        let original = Document::from(parsed.document());
        let mut outputs = Vec::new();
        for document in [
            parsed.document(),
            parsed.document(),
            pipeline.parse(source)?.document(),
        ] {
            let mut output = Vec::new();
            let mut warnings = Vec::new();
            let warning_source = pipeline.converter().warning_source();
            pipeline.write_to(
                document,
                &mut output,
                None,
                None,
                &mut Diagnostics::new(&warning_source, &mut warnings),
            )?;
            assert!(warnings.is_empty(), "{warnings:?}");
            outputs.push(String::from_utf8(output)?);
        }
        let first = outputs.first().ok_or("missing output")?;
        assert!(first.contains("id=\"_indexterm_0\""));
        for output in &outputs {
            assert_eq!(first, output);
        }
        assert_eq!(Document::from(parsed.document()), original);
    }
    Ok(())
}
