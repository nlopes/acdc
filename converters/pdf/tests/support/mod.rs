use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_converters_dev::fixtures::FixtureOutput;
use acdc_converters_pdf::{PdfOptions, Processor};
use acdc_parser::{
    DocumentAttributes, Options as ParserOptions, OptionsBuilder, SafeMode, ast::DocumentRef,
    parse_file,
};

pub(crate) fn processor(builder: OptionsBuilder<'_>) -> Result<Processor<'_>, Box<dyn Error>> {
    Ok(Processor::new(
        Options::builder().safe_mode(SafeMode::Unsafe).build(),
        builder.with_safe_mode(SafeMode::Unsafe),
    )?)
}

fn fixture_theme(doc: DocumentRef<'_>) -> Option<PathBuf> {
    doc.attributes()
        .get("acdc-pdf-test-theme")
        .and_then(|value| value.text())
        .map(|name| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/themes")
                .join(name)
                .with_extension("yaml")
        })
}

pub(crate) fn render(path: &Path) -> Result<(FixtureOutput, String), Box<dyn Error>> {
    let bootstrap = processor(ParserOptions::builder())?;
    let parsed = parse_file(path, bootstrap.parser_options())?;
    let doc = parsed.document();
    let directory = tempfile::tempdir()?;
    let typst_path = directory.path().join("fixture.typ");
    let processor = processor(
        ParserOptions::builder()
            .with_attributes(DocumentAttributes::from(doc.attributes()).into_inputs()),
    )?
    .with_pdf_options(PdfOptions {
        emit_typst: Some(typst_path.clone()),
        theme: fixture_theme(doc),
        ..PdfOptions::default()
    });
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("pdf");
    processor.write_to(
        doc,
        &mut output,
        Some(path),
        None,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    Ok((
        FixtureOutput { output, warnings },
        fs::read_to_string(typst_path)?,
    ))
}
