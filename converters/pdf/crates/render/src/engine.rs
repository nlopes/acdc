use std::collections::BTreeSet;

use acdc_pdf_images::ImageMap;
use typst::{comemo::Track, introspection::Introspector, layout::FrameItem, text::Font};
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;
use typst_pdf::{PdfOptions, pdf, pdf_in_bundle};

use crate::{
    NamedDestination,
    error::{Error, format_diagnostics},
    resolver::ImageFileResolver,
};

/// Build a Typst engine for `markup` with `fonts` and `assets` registered,
/// compile it, and export the laid-out document to PDF bytes.
///
/// Images are served from disk on demand by [`ImageFileResolver`], so their
/// bytes are read only when the compiler needs them and are not retained by
/// this crate after ownership passes to Typst. The bundled syntax-highlight
/// theme is small and stays in memory.
///
/// Compilation and missing-glyph warnings are returned alongside the document
/// so the caller can surface them without failing the build.
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
    let mut warnings = collect_warnings(&result.warnings);
    let document = result.output?;
    warnings.extend(missing_glyph_warnings(&document));
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

fn missing_glyph_warnings(document: &PagedDocument) -> Vec<String> {
    let mut missing = BTreeSet::new();
    let mut frames = document
        .pages()
        .iter()
        .map(|page| &page.frame)
        .collect::<Vec<_>>();
    while let Some(frame) = frames.pop() {
        for (_, item) in frame.items() {
            match item {
                FrameItem::Group(group) => frames.push(&group.frame),
                FrameItem::Text(text) => {
                    // Inspect shaped output after font fallback. A PDF can retain
                    // extractable text while drawing the missing-glyph box.
                    for glyph in &text.glyphs {
                        if glyph.id == 0
                            && let Some(source) = text.text.get(glyph.range())
                        {
                            missing.insert(source);
                        }
                    }
                }
                FrameItem::Shape(..)
                | FrameItem::Image(..)
                | FrameItem::Link(..)
                | FrameItem::Tag(_) => {}
            }
        }
    }
    missing
        .into_iter()
        .map(|text| {
            let codes = text
                .chars()
                .map(|ch| format!("U+{:04X}", u32::from(ch)))
                .collect::<Vec<_>>()
                .join(" ");
            format!("missing glyph for {text:?} ({codes}); add a font that supports this text")
        })
        .collect()
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
