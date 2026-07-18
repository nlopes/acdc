//! Language-server analysis configuration.

use acdc_converters_core::{BackendTraits, Doctype};
use acdc_parser::{DocumentAttributes, Options};
use serde::Deserialize;

/// Options supplied by the LSP client during initialization.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct ServerOptions {
    pub(crate) backend: AnalysisBackend,
}

/// Converter backend whose intrinsic attributes guide language-server analysis.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AnalysisBackend {
    #[default]
    #[serde(alias = "html")]
    Html5,
    Html5s,
    #[serde(alias = "docbook")]
    Docbook5,
    Manpage,
    Markdown,
    Pdf,
    Terminal,
}

impl AnalysisBackend {
    /// Build parser options containing the selected backend's intrinsic attributes.
    pub(crate) fn parser_options(self) -> Options<'static> {
        let (traits, doctype) = self.profile();
        let mut attributes = DocumentAttributes::default();
        traits.apply(&mut attributes, doctype);
        Options::with_attributes(attributes)
    }

    const fn profile(self) -> (BackendTraits, Doctype) {
        match self {
            Self::Html5 => (
                BackendTraits::new("html5", "html", "html", ".html").with_htmlsyntax("html"),
                Doctype::Article,
            ),
            Self::Html5s => (
                BackendTraits::new("html5s", "html", "html", ".html").with_htmlsyntax("html"),
                Doctype::Article,
            ),
            Self::Docbook5 => (
                BackendTraits::new("docbook5", "docbook", "xml", ".xml"),
                Doctype::Article,
            ),
            Self::Manpage => (
                BackendTraits::new("manpage", "manpage", "man", ".man"),
                Doctype::Manpage,
            ),
            Self::Markdown => (
                BackendTraits::new("markdown", "markdown", "md", ".md"),
                Doctype::Article,
            ),
            Self::Pdf => (
                BackendTraits::new("pdf", "html", "pdf", ".pdf").with_htmlsyntax("html"),
                Doctype::Article,
            ),
            Self::Terminal => (
                BackendTraits::new("terminal", "terminal", "terminal", ".terminal"),
                Doctype::Article,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_asciidoctor_html5_backend() {
        let options = AnalysisBackend::default().parser_options();

        assert_eq!(
            options.document_attributes.get_string("backend").as_deref(),
            Some("html5")
        );
        assert!(options.document_attributes.contains_key("backend-html5"));
        assert!(!options.document_attributes.contains_key("backend-pdf"));
    }

    #[test]
    fn deserializes_configured_pdf_backend() -> Result<(), serde_json::Error> {
        let options: ServerOptions = serde_json::from_str(r#"{"backend":"pdf"}"#)?;

        assert_eq!(options.backend, AnalysisBackend::Pdf);
        let parser_options = options.backend.parser_options();
        assert_eq!(
            parser_options
                .document_attributes
                .get_string("backend")
                .as_deref(),
            Some("pdf")
        );
        assert!(
            parser_options
                .document_attributes
                .contains_key("backend-pdf")
        );
        Ok(())
    }

    #[test]
    fn canonicalizes_asciidoctor_backend_aliases() -> Result<(), serde_json::Error> {
        let html: ServerOptions = serde_json::from_str(r#"{"backend":"html"}"#)?;
        let docbook: ServerOptions = serde_json::from_str(r#"{"backend":"docbook"}"#)?;

        assert_eq!(html.backend, AnalysisBackend::Html5);
        assert_eq!(docbook.backend, AnalysisBackend::Docbook5);
        Ok(())
    }
}
