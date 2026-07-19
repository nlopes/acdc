//! Language-server analysis configuration.

use std::path::Path;

use acdc_converters_core::{BackendTraits, Doctype};
use acdc_parser::{DocumentAttributes, Options};
use serde::Deserialize;
use serde_json::Value;
use tower_lsp_server::ls_types::Uri;

/// Options supplied by the LSP client during initialization.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct ServerOptions {
    pub(crate) backend: AnalysisBackend,
}

/// Settings supplied by `workspace/configuration`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct WorkspaceSettings {
    pub(crate) backend: Option<AnalysisBackend>,
}

/// A pushed update to the unscoped analysis backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackendUpdate {
    /// The notification did not contain an acdc-lsp backend setting.
    Unchanged,
    /// Select this backend for unscoped analysis.
    Set(AnalysisBackend),
    /// Remove the unscoped override and return to the initialization fallback.
    Reset,
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
    fn parser_options(self) -> Options<'static> {
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

/// Cached parser option profiles for every analysis backend.
pub(crate) struct ParserProfiles {
    html5: Options<'static>,
    html5s: Options<'static>,
    docbook5: Options<'static>,
    manpage: Options<'static>,
    markdown: Options<'static>,
    pdf: Options<'static>,
    terminal: Options<'static>,
}

impl ParserProfiles {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            html5: AnalysisBackend::Html5.parser_options(),
            html5s: AnalysisBackend::Html5s.parser_options(),
            docbook5: AnalysisBackend::Docbook5.parser_options(),
            manpage: AnalysisBackend::Manpage.parser_options(),
            markdown: AnalysisBackend::Markdown.parser_options(),
            pdf: AnalysisBackend::Pdf.parser_options(),
            terminal: AnalysisBackend::Terminal.parser_options(),
        }
    }

    #[must_use]
    pub(crate) const fn get(&self, backend: AnalysisBackend) -> &Options<'static> {
        match backend {
            AnalysisBackend::Html5 => &self.html5,
            AnalysisBackend::Html5s => &self.html5s,
            AnalysisBackend::Docbook5 => &self.docbook5,
            AnalysisBackend::Manpage => &self.manpage,
            AnalysisBackend::Markdown => &self.markdown,
            AnalysisBackend::Pdf => &self.pdf,
            AnalysisBackend::Terminal => &self.terminal,
        }
    }
}

impl Default for ParserProfiles {
    fn default() -> Self {
        Self::new()
    }
}

/// A resource-scoped backend override for one workspace root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RootConfiguration {
    pub(crate) uri: Uri,
    pub(crate) backend: Option<AnalysisBackend>,
}

/// Effective analysis settings and their precedence chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnalysisConfiguration {
    fallback: AnalysisBackend,
    unscoped: Option<AnalysisBackend>,
    roots: Vec<RootConfiguration>,
}

impl AnalysisConfiguration {
    #[must_use]
    pub(crate) fn new(fallback: AnalysisBackend, roots: Vec<Uri>) -> Self {
        Self {
            fallback,
            unscoped: None,
            roots: roots
                .into_iter()
                .map(|uri| RootConfiguration { uri, backend: None })
                .collect(),
        }
    }

    pub(crate) fn set_unscoped(&mut self, backend: Option<AnalysisBackend>) {
        self.unscoped = backend;
    }

    #[must_use]
    pub(crate) fn roots(&self) -> Vec<Uri> {
        self.roots.iter().map(|root| root.uri.clone()).collect()
    }

    pub(crate) fn replace_roots(&mut self, roots: Vec<RootConfiguration>) {
        self.roots = roots;
    }

    #[must_use]
    pub(crate) fn root_backend(&self, uri: &Uri) -> Option<AnalysisBackend> {
        self.roots
            .iter()
            .find(|root| root.uri == *uri)
            .and_then(|root| root.backend)
    }

    /// Resolve the effective backend for a document URI.
    #[must_use]
    pub(crate) fn backend_for(&self, uri: &Uri) -> AnalysisBackend {
        let root_backend = self
            .roots
            .iter()
            .filter_map(|root| root_match_len(&root.uri, uri).map(|len| (len, root.backend)))
            .max_by_key(|(len, _)| *len)
            .and_then(|(_, backend)| backend);
        root_backend.or(self.unscoped).unwrap_or(self.fallback)
    }

    #[must_use]
    pub(crate) fn contains(&self, uri: &Uri) -> bool {
        self.roots
            .iter()
            .any(|root| root_match_len(&root.uri, uri).is_some())
    }
}

impl Default for AnalysisConfiguration {
    fn default() -> Self {
        Self::new(AnalysisBackend::default(), Vec::new())
    }
}

/// Parse direct or namespaced settings from `workspace/didChangeConfiguration`.
pub(crate) fn parse_backend_update(settings: &Value) -> Result<BackendUpdate, String> {
    let settings = settings
        .get("acdc-lsp")
        .or_else(|| settings.get("acdcLsp"))
        .unwrap_or(settings);
    let Some(object) = settings.as_object() else {
        return Ok(BackendUpdate::Unchanged);
    };
    let Some(value) = object.get("backend") else {
        return Ok(BackendUpdate::Unchanged);
    };
    if value.is_null() {
        return Ok(BackendUpdate::Reset);
    }
    serde_json::from_value(value.clone())
        .map(BackendUpdate::Set)
        .map_err(|error| format!("invalid acdc-lsp backend setting: {error}"))
}

fn root_match_len(root: &Uri, document: &Uri) -> Option<usize> {
    match (root.to_file_path(), document.to_file_path()) {
        (Some(root), Some(document)) => path_match_len(root.as_ref(), document.as_ref()),
        _ => uri_match_len(root.as_str(), document.as_str()),
    }
}

fn path_match_len(root: &Path, document: &Path) -> Option<usize> {
    document
        .starts_with(root)
        .then(|| root.components().count())
}

fn uri_match_len(root: &str, document: &str) -> Option<usize> {
    let root = root.trim_end_matches('/');
    (document == root
        || document
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/')))
    .then_some(root.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_asciidoctor_html5_backend() {
        let profiles = ParserProfiles::new();
        let options = profiles.get(AnalysisBackend::default());

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
        let profiles = ParserProfiles::new();
        let parser_options = profiles.get(options.backend);
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

    #[test]
    fn most_specific_workspace_root_wins() -> Result<(), Box<dyn std::error::Error>> {
        let outer = "file:///workspace".parse::<Uri>()?;
        let inner = "file:///workspace/manual".parse::<Uri>()?;
        let document = "file:///workspace/manual/index.adoc".parse::<Uri>()?;
        let mut configuration =
            AnalysisConfiguration::new(AnalysisBackend::Html5, vec![outer.clone(), inner.clone()]);
        configuration.set_unscoped(Some(AnalysisBackend::Markdown));
        configuration.replace_roots(vec![
            RootConfiguration {
                uri: outer,
                backend: Some(AnalysisBackend::Docbook5),
            },
            RootConfiguration {
                uri: inner,
                backend: Some(AnalysisBackend::Pdf),
            },
        ]);

        assert_eq!(configuration.backend_for(&document), AnalysisBackend::Pdf);
        Ok(())
    }

    #[test]
    fn selected_root_without_override_uses_unscoped_backend()
    -> Result<(), Box<dyn std::error::Error>> {
        let outer = "file:///workspace".parse::<Uri>()?;
        let inner = "file:///workspace/manual".parse::<Uri>()?;
        let document = "file:///workspace/manual/index.adoc".parse::<Uri>()?;
        let mut configuration = AnalysisConfiguration::new(AnalysisBackend::Html5, Vec::new());
        configuration.set_unscoped(Some(AnalysisBackend::Markdown));
        configuration.replace_roots(vec![
            RootConfiguration {
                uri: outer,
                backend: Some(AnalysisBackend::Docbook5),
            },
            RootConfiguration {
                uri: inner,
                backend: None,
            },
        ]);

        assert_eq!(
            configuration.backend_for(&document),
            AnalysisBackend::Markdown
        );
        Ok(())
    }

    #[test]
    fn pushed_settings_support_direct_namespaced_and_reset_forms() {
        assert_eq!(
            parse_backend_update(&serde_json::json!({"backend": "pdf"})),
            Ok(BackendUpdate::Set(AnalysisBackend::Pdf))
        );
        assert_eq!(
            parse_backend_update(&serde_json::json!({"acdc-lsp": {"backend": "docbook"}})),
            Ok(BackendUpdate::Set(AnalysisBackend::Docbook5))
        );
        assert_eq!(
            parse_backend_update(&serde_json::json!({"acdcLsp": {"backend": null}})),
            Ok(BackendUpdate::Reset)
        );
        assert_eq!(
            parse_backend_update(&serde_json::json!({"unrelated": true})),
            Ok(BackendUpdate::Unchanged)
        );
    }
}
