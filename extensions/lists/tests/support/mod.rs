use acdc_converters_core::{
    Converter as _, Diagnostics, Options,
    pipeline::{ExtensionCatalog, Pipeline},
};
use acdc_converters_html::{Converter, HtmlVariant};
use acdc_parser::CompatibilityMode;

pub(crate) fn render(
    source: &str,
    mode: CompatibilityMode,
    variant: HtmlVariant,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut available = ExtensionCatalog::default();
    available.register("lists", acdc_extensions_lists::registry()?)?;
    let pipeline = Pipeline::new(Converter::new_with_variant(
        Options::builder().embedded(true).build(),
        acdc_parser::Options::builder().with_compatibility_mode(mode),
        variant,
    )?)
    .with_extensions(available.select(["lists"])?.ok_or("selection")?);
    let parsed = pipeline.parse(source)?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    let before = serde_json::to_value(parsed.document())?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let warning_source = pipeline.converter().warning_source();
    for _ in 0..2 {
        let mut bytes = Vec::new();
        pipeline.write_to(
            parsed.document(),
            &mut bytes,
            None,
            None,
            &mut Diagnostics::new(&warning_source, &mut warnings),
        )?;
        if output.is_empty() {
            output = bytes;
        } else {
            assert_eq!(bytes, output, "repeated conversion");
        }
    }
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(String::from_utf8(output)?)
}
