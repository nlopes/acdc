use std::{
    error::Error,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Diagnostics, GeneratorMetadata, Options, WarningSource};
use acdc_converters_dev::fixtures::FixtureOutput;
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{DocumentAttributes, Options as ParserOptions, SafeMode, ast::DocumentRef};

pub(crate) fn has_highlighter(document: DocumentRef<'_>) -> bool {
    document.attributes().get("source-highlighter").is_some()
}

pub(crate) fn expected_fixture_path(
    directory: &Path,
    stem: &str,
    document: DocumentRef<'_>,
) -> PathBuf {
    let extension = if !cfg!(feature = "highlighting") && has_highlighter(document) {
        "no-highlighting.html"
    } else {
        "html"
    };
    directory.join(stem).with_extension(extension)
}

pub(crate) fn render_document(
    doc: DocumentRef<'_>,
    variant: HtmlVariant,
    embedded: bool,
) -> Result<FixtureOutput, Box<dyn Error>> {
    let processor = Processor::new_with_variant(
        Options::builder()
            .safe_mode(SafeMode::Unsafe)
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build(),
        ParserOptions::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_attributes(DocumentAttributes::from(doc.attributes()).into_inputs()),
        variant,
    )?;
    let render_options = RenderOptions {
        embedded,
        source_dir: doc
            .attributes()
            .get("docdir")
            .and_then(|value| value.text())
            .map(PathBuf::from),
        ..RenderOptions::default()
    };
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("html").with_variant(variant.as_str());
    processor.convert_to_writer(
        doc,
        &mut output,
        &render_options,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    Ok(FixtureOutput { output, warnings })
}
