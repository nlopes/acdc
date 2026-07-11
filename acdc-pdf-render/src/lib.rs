//! Compiles a Typst markup string into PDF bytes.
//!
//! This is the only crate that depends on the Typst compiler. Everything
//! upstream produces a plain `String`, keeping the heavy dependency isolated
//! behind a `&str` boundary.
#![forbid(unsafe_code)]

mod engine;
mod error;
mod fonts;
mod resolver;

use std::path::PathBuf;

use acdc_pdf_images::ImageMap;

pub use error::Error;

/// Options controlling how markup is compiled to a PDF.
#[derive(Debug, Clone, Default)]
pub struct RenderConfig {
    /// Extra directories to search for fonts (ttf/otf/ttc/otc). Fonts found
    /// here are registered alongside the bundled fonts, so a brand family
    /// supplied at runtime is used wherever the markup asks for it.
    pub font_dirs: Vec<PathBuf>,
}

/// A successfully rendered document.
#[derive(Debug)]
pub struct Rendered {
    /// The PDF file contents.
    pub pdf: Vec<u8>,
    /// Non-fatal Typst compilation warnings, if any.
    pub warnings: Vec<String>,
}

/// Compile a Typst markup string into PDF bytes, embedding the resolved images
/// referenced by the markup.
///
/// # Errors
/// Returns [`Error`] if a font directory can't be read, the markup fails to
/// compile, or PDF export fails.
pub fn render_pdf(
    markup: &str,
    assets: &ImageMap,
    config: &RenderConfig,
) -> Result<Rendered, Error> {
    let fonts = fonts::load(&config.font_dirs)?;
    let (pdf, warnings) = engine::render(markup.to_owned(), fonts, assets)?;
    Ok(Rendered { pdf, warnings })
}

#[cfg(test)]
mod tests {
    use acdc_pdf_images::{ResolveConfig, resolve};

    use super::*;

    const PNG_1X1_DATA_URI: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABAQMAAAAl21bKAAAAA1BMVEXyVTNpJlJjAAAACklEQVQI12NgAAAAAgAB4iG8MwAAAABJRU5ErkJggg==";

    #[test]
    fn compiles_minimal_document_to_valid_pdf() -> Result<(), Box<dyn std::error::Error>> {
        let markup = "#set text(font: \"IBM Plex Sans\")\n= Hello\n\nThe quick brown fox.";
        let rendered = render_pdf(markup, &ImageMap::new(), &RenderConfig::default())?;

        assert!(
            rendered.pdf.starts_with(b"%PDF-"),
            "output is not a PDF (starts with {:?})",
            rendered
                .pdf
                .get(..rendered.pdf.len().min(8))
                .unwrap_or_default()
        );
        assert!(rendered.pdf.len() > 1000, "PDF suspiciously small");

        let doc = lopdf::Document::load_mem(&rendered.pdf)?;
        assert!(!doc.get_pages().is_empty(), "expected at least one page");
        Ok(())
    }

    #[test]
    fn reports_compile_errors() -> Result<(), Box<dyn std::error::Error>> {
        // `#foo` calls an undefined function.
        let Err(err) = render_pdf(
            "#undefined_function()",
            &ImageMap::new(),
            &RenderConfig::default(),
        ) else {
            return Err(std::io::Error::other("render unexpectedly succeeded").into());
        };
        assert!(matches!(err, Error::Compile(_)));
        Ok(())
    }

    #[test]
    fn bundled_syntax_theme_loads_in_typst() -> Result<(), Box<dyn std::error::Error>> {
        let markup = format!(
            "#set raw(theme: \"{}\")\n```rust\nfn main() {{ println!(\"hello\"); }}\n```",
            acdc_pdf_theme::HIGHLIGHT_THEME_PATH
        );
        let rendered = render_pdf(&markup, &ImageMap::new(), &RenderConfig::default())?;

        assert!(rendered.pdf.starts_with(b"%PDF-"));
        assert!(rendered.warnings.is_empty(), "{:?}", rendered.warnings);
        Ok(())
    }

    #[test]
    fn renders_a_resolved_image() -> Result<(), Box<dyn std::error::Error>> {
        let spool = tempfile::tempdir()?;
        let resolved = resolve(&[PNG_1X1_DATA_URI], &ResolveConfig::new(".", spool.path()));
        assert!(resolved.failures.is_empty(), "{:?}", resolved.failures);
        let image = resolved
            .assets
            .get(PNG_1X1_DATA_URI)
            .ok_or_else(|| std::io::Error::other("resolved image is missing"))?;
        let markup = format!("#image(\"{}\")", image.virtual_path);

        let rendered = render_pdf(&markup, &resolved.assets, &RenderConfig::default())?;

        assert!(rendered.pdf.starts_with(b"%PDF-"));
        assert!(rendered.warnings.is_empty(), "{:?}", rendered.warnings);
        Ok(())
    }
}
