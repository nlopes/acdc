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

use std::{collections::HashSet, path::PathBuf, sync::LazyLock};

use acdc_pdf_images::ImageMap;

pub use error::Error;

static RAW_LANGUAGES: LazyLock<HashSet<String>> = LazyLock::new(|| {
    typst::text::RawElem::languages()
        .into_iter()
        .flat_map(|(name, extensions)| std::iter::once(name).chain(extensions))
        .map(str::to_ascii_lowercase)
        .collect()
});

/// Whether Typst has a bundled syntax definition for a language name or tag.
#[must_use]
pub fn supports_raw_language(language: &str) -> bool {
    RAW_LANGUAGES.contains(language)
        || language.bytes().any(|byte| byte.is_ascii_uppercase())
            && RAW_LANGUAGES.contains(&language.to_ascii_lowercase())
}

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
    fn emits_tagged_pdf_with_document_language_and_semantic_structure()
    -> Result<(), Box<dyn std::error::Error>> {
        let markup = concat!(
            "#set text(font: \"IBM Plex Sans\", lang: \"pt\", region: \"BR\")\n",
            "= Título\n\nTexto.\n",
        );
        let rendered = render_pdf(markup, &ImageMap::new(), &RenderConfig::default())?;
        let document = lopdf::Document::load_mem(&rendered.pdf)?;
        let catalog = document.catalog()?;
        let (_, mark_info) = document.dereference(catalog.get(b"MarkInfo")?)?;

        assert!(mark_info.as_dict()?.get(b"Marked")?.as_bool()?);
        assert!(catalog.get(b"StructTreeRoot").is_ok());
        assert_eq!(lopdf::decode_text_string(catalog.get(b"Lang")?)?, "pt-BR");

        let roles = document
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .filter(|dictionary| {
                dictionary
                    .get(b"Type")
                    .and_then(lopdf::Object::as_name)
                    .ok()
                    == Some(b"StructElem")
            })
            .filter_map(|dictionary| dictionary.get(b"S").and_then(lopdf::Object::as_name).ok())
            .collect::<Vec<_>>();

        for expected in [b"Document".as_slice(), b"H1", b"P"] {
            assert!(
                roles.contains(&expected),
                "missing structure role {expected:?}"
            );
        }
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
    fn identifies_typst_raw_languages_by_name_and_tag() {
        assert!(supports_raw_language("Rust"));
        assert!(supports_raw_language("rust"));
        assert!(supports_raw_language("rs"));
        assert!(!supports_raw_language("definitely-unknown"));
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
