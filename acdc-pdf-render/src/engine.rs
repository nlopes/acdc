use acdc_pdf_images::ImageMap;
use typst::text::Font;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;
use typst_pdf::{PdfOptions, pdf};

use crate::error::{Error, format_diagnostics};
use crate::resolver::ImageFileResolver;

/// Build a Typst engine for `markup` with `fonts` and `assets` registered,
/// compile it, and export the laid-out document to PDF bytes.
///
/// Images are served from disk on demand by [`ImageFileResolver`], so their
/// bytes are read only when the compiler needs them and are not retained by
/// this crate after ownership passes to Typst. The bundled syntax-highlight
/// theme is small and stays in memory.
///
/// Compilation warnings are returned alongside the document so the caller can
/// surface them without failing the build.
pub(crate) fn render(
    markup: String,
    fonts: Vec<Font>,
    assets: &ImageMap,
) -> Result<(Vec<u8>, Vec<String>), Error> {
    let engine = TypstEngine::builder()
        .main_file(markup)
        .fonts(fonts)
        .with_static_file_resolver([(
            acdc_pdf_theme::HIGHLIGHT_THEME_PATH,
            acdc_pdf_theme::highlight_theme(),
        )])
        .add_file_resolver(ImageFileResolver::new(assets))
        .build();

    let result = engine.compile::<PagedDocument>();
    let warnings = collect_warnings(&result.warnings);
    let document = result.output?;
    let pdf = pdf(&document, &PdfOptions::default())
        .map_err(|diagnostics| Error::Pdf(format_diagnostics(&diagnostics)))?;
    Ok((pdf, warnings))
}

fn collect_warnings(warnings: &[typst::diag::SourceDiagnostic]) -> Vec<String> {
    if warnings.is_empty() {
        Vec::new()
    } else {
        format_diagnostics(warnings)
            .lines()
            .map(|line| line.trim_start_matches("  - ").to_owned())
            .collect()
    }
}
