use std::{error::Error as StdError, fs};

use acdc_converters_core::{Converter as _, Diagnostics, Options, WarningSource};
use acdc_converters_pdf::{Converter, PdfOptions};
use acdc_parser::{
    Error, Options as ParserOptions, SafeMode,
    extensions::{
        BlockBuilder, DisplayIntent, DocumentEditor, Registry, TreeProcessor,
        TreeProcessorRegistration,
    },
};

struct Snapshot;

impl TreeProcessor for Snapshot {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let targets: Vec<_> = editor
            .query()
            .blocks()
            .map(|view| view.target().clone())
            .collect();
        if let (Some(target), Some(output)) = (targets.first(), targets.get(1)) {
            let title = editor.capture(target)?;
            editor.replace_block(
                target,
                BlockBuilder::paragraph("Changed.").with_attributes("title=Changed"),
            )?;
            editor.replace_block(
                output,
                BlockBuilder::paragraph_inlines([title.display(DisplayIntent::Title)]),
            )?;
        }
        Ok(())
    }
}

#[test]
fn extension_displays_prepare_captured_title_assets_before_pdf_resolution()
-> Result<(), Box<dyn StdError>> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("mark.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12"><rect width="12" height="12" fill="blue"/></svg>"#,
    )?;
    let source = directory.path().join("display.adoc");
    fs::write(
        &source,
        ".Original image:mark.svg[Mark]\nParagraph.\n\nOutput.",
    )?;
    let mut builder = Registry::builder();
    builder.register_tree_processor(TreeProcessorRegistration::new("snapshot", || Snapshot))?;
    let parsed = builder
        .build()
        .parse_file(&source, &ParserOptions::default())?
        .finish()?;
    let typst_path = directory.path().join("display.typ");
    let converter = Converter::new(
        Options::builder().safe_mode(SafeMode::Unsafe).build(),
        ParserOptions::builder(),
    )?
    .with_pdf_options(PdfOptions {
        emit_typst: Some(typst_path.clone()),
        ..PdfOptions::default()
    });
    let warning_source = WarningSource::new("pdf");
    let mut warnings = Vec::new();
    let mut output = Vec::new();
    converter.write_to(
        parsed.document(),
        &mut output,
        Some(&source),
        None,
        &mut Diagnostics::new(&warning_source, &mut warnings),
    )?;
    assert!(output.starts_with(b"%PDF"));
    let typst = fs::read_to_string(typst_path)?;
    assert!(
        typst.contains("#box(image(\"/images/"),
        "captured image was not resolved: {typst}"
    );
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.message.contains("mark.svg")),
        "{warnings:?}"
    );
    Ok(())
}
