use acdc_pdf_images::ImageMap;
use typst::{comemo::Track, introspection::Introspector, text::Font};
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;
use typst_pdf::{PdfOptions, pdf, pdf_in_bundle};

use crate::resolver::ImageFileResolver;
use crate::{
    NamedDestination,
    error::{Error, format_diagnostics},
};

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
    named_destinations: &[NamedDestination],
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
    let pdf_options = PdfOptions {
        tagged: true,
        ..PdfOptions::default()
    };
    let pdf = if named_destinations.is_empty() {
        pdf(&document, &pdf_options)
    } else {
        let labelled = document.introspector().query_labelled();
        let anchors = named_destinations
            .iter()
            .filter_map(|destination| {
                labelled
                    .iter()
                    .find(|element| {
                        element
                            .label()
                            .is_some_and(|label| label.resolve().as_str() == destination.label)
                    })
                    .and_then(typst::foundations::Content::location)
                    .map(|location| (location, destination.name.clone().into()))
            })
            .collect::<Vec<_>>();
        let link_resolver =
            typst::model::LateLinkResolver::new(None, document.introspector().as_ref());
        pdf_in_bundle(&document, &pdf_options, &anchors, link_resolver.track())
    }
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
