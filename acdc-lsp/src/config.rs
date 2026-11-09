//! Language-server analysis configuration.

use std::path::Path;

use acdc_converters_core::{BackendProfile, Doctype};
use acdc_parser::{CompatibilityMode, Options};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use tower_lsp_server::ls_types::Uri;

/// Options supplied by the LSP client during initialization.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct ServerOptions {
    pub(crate) backend: AnalysisBackend,
    pub(crate) compatibility_mode: CompatibilityMode,
}

/// Settings supplied by `workspace/configuration`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct WorkspaceSettings {
    pub(crate) backend: Option<AnalysisBackend>,
    pub(crate) compatibility_mode: Option<CompatibilityMode>,
}

impl WorkspaceSettings {
    fn resolve(self, fallback: ServerOptions) -> ServerOptions {
        ServerOptions {
            backend: self.backend.unwrap_or(fallback.backend),
            compatibility_mode: self
                .compatibility_mode
                .unwrap_or(fallback.compatibility_mode),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SettingUpdate<T> {
    #[default]
    Unchanged,
    Set(T),
    Reset,
}

impl<T> SettingUpdate<T> {
    fn apply(self, target: &mut Option<T>) {
        match self {
            Self::Unchanged => {}
            Self::Set(value) => *target = Some(value),
            Self::Reset => *target = None,
        }
    }
}

/// Independent partial updates from a pushed configuration notification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SettingsUpdate {
    backend: SettingUpdate<AnalysisBackend>,
    compatibility_mode: SettingUpdate<CompatibilityMode>,
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
    #[allow(
        clippy::expect_used,
        reason = "Backend profiles contain only valid built-in text attributes"
    )]
    fn parser_options(self, mode: CompatibilityMode) -> Options<'static> {
        let (backend_profile, doctype) = self.profile();
        backend_profile
            .apply(
                Options::builder()
                    .with_compatibility_mode(mode)
                    .with_safe_mode(acdc_parser::SafeMode::Server)
                    .with_include_loader(acdc_parser::IncludeLoader::Disabled),
                doctype,
                false,
            )
            .build()
            .expect("built-in backend profile must be valid")
    }

    const fn profile(self) -> (BackendProfile, Doctype) {
        match self {
            Self::Html5 => (
                BackendProfile::new("html5", "html", "html", ".html").with_htmlsyntax("html"),
                Doctype::Article,
            ),
            Self::Html5s => (
                BackendProfile::new("html5s", "html", "html", ".html").with_htmlsyntax("html"),
                Doctype::Article,
            ),
            Self::Docbook5 => (
                BackendProfile::new("docbook5", "docbook", "xml", ".xml"),
                Doctype::Article,
            ),
            Self::Manpage => (
                BackendProfile::new("manpage", "manpage", "man", ".man"),
                Doctype::Manpage,
            ),
            Self::Markdown => (
                BackendProfile::new("markdown", "markdown", "md", ".md"),
                Doctype::Article,
            ),
            Self::Pdf => (
                BackendProfile::new("pdf", "html", "pdf", ".pdf").with_htmlsyntax("html"),
                Doctype::Article,
            ),
            Self::Terminal => (
                BackendProfile::new("terminal", "terminal", "terminal", ".terminal"),
                Doctype::Article,
            ),
        }
    }
}

/// Cached parser options for each backend and compatibility mode.
pub(crate) struct ParserProfiles {
    asciidoctor: [Options<'static>; 7],
    draft: [Options<'static>; 7],
}

impl ParserProfiles {
    pub(crate) fn new() -> Self {
        let backends = [
            AnalysisBackend::Html5,
            AnalysisBackend::Html5s,
            AnalysisBackend::Docbook5,
            AnalysisBackend::Manpage,
            AnalysisBackend::Markdown,
            AnalysisBackend::Pdf,
            AnalysisBackend::Terminal,
        ];
        Self {
            asciidoctor: backends
                .map(|backend| backend.parser_options(CompatibilityMode::Asciidoctor)),
            draft: backends.map(|backend| backend.parser_options(CompatibilityMode::Draft)),
        }
    }

    pub(crate) fn get(&self, settings: ServerOptions) -> &Options<'static> {
        let [html5, html5s, docbook5, manpage, markdown, pdf, terminal] =
            if settings.compatibility_mode == CompatibilityMode::Draft {
                &self.draft
            } else {
                &self.asciidoctor
            };
        match settings.backend {
            AnalysisBackend::Html5 => html5,
            AnalysisBackend::Html5s => html5s,
            AnalysisBackend::Docbook5 => docbook5,
            AnalysisBackend::Manpage => manpage,
            AnalysisBackend::Markdown => markdown,
            AnalysisBackend::Pdf => pdf,
            AnalysisBackend::Terminal => terminal,
        }
    }
}

/// Resource-scoped settings for one workspace root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RootConfiguration {
    pub(crate) uri: Uri,
    pub(crate) settings: WorkspaceSettings,
}

/// Effective analysis settings and their precedence chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnalysisConfiguration {
    fallback: ServerOptions,
    unscoped: WorkspaceSettings,
    roots: Vec<RootConfiguration>,
}

impl AnalysisConfiguration {
    pub(crate) fn new(fallback: ServerOptions, roots: Vec<Uri>) -> Self {
        Self {
            fallback,
            unscoped: WorkspaceSettings::default(),
            roots: roots
                .into_iter()
                .map(|uri| RootConfiguration {
                    uri,
                    settings: WorkspaceSettings::default(),
                })
                .collect(),
        }
    }

    pub(crate) fn set_unscoped(&mut self, settings: WorkspaceSettings) {
        self.unscoped = settings;
    }

    pub(crate) fn apply_update(&mut self, update: SettingsUpdate) {
        update.backend.apply(&mut self.unscoped.backend);
        update
            .compatibility_mode
            .apply(&mut self.unscoped.compatibility_mode);
    }

    pub(crate) fn roots(&self) -> Vec<Uri> {
        self.roots.iter().map(|root| root.uri.clone()).collect()
    }

    pub(crate) fn replace_roots(&mut self, roots: Vec<RootConfiguration>) {
        self.roots = roots;
    }

    pub(crate) fn root_settings(&self, uri: &Uri) -> WorkspaceSettings {
        self.roots
            .iter()
            .find(|root| root.uri == *uri)
            .map(|root| root.settings)
            .unwrap_or_default()
    }

    pub(crate) fn settings_for(&self, uri: &Uri) -> ServerOptions {
        let settings = self
            .roots
            .iter()
            .filter_map(|root| root_match_len(&root.uri, uri).map(|len| (len, root.settings)))
            .max_by_key(|(len, _)| *len)
            .map(|(_, settings)| settings)
            .unwrap_or_default();
        settings.resolve(self.unscoped.resolve(self.fallback))
    }

    #[cfg(any(feature = "preview", test))]
    pub(crate) fn backend_for(&self, uri: &Uri) -> AnalysisBackend {
        self.settings_for(uri).backend
    }

    pub(crate) fn contains(&self, uri: &Uri) -> bool {
        self.roots
            .iter()
            .any(|root| root_match_len(&root.uri, uri).is_some())
    }
}

impl Default for AnalysisConfiguration {
    fn default() -> Self {
        Self::new(ServerOptions::default(), Vec::new())
    }
}

/// Parse direct or namespaced settings from `workspace/didChangeConfiguration`.
pub(crate) fn parse_settings_update(settings: &Value) -> Result<SettingsUpdate, String> {
    let settings = settings
        .get("acdc-lsp")
        .or_else(|| settings.get("acdcLsp"))
        .unwrap_or(settings);
    let Some(object) = settings.as_object() else {
        return Ok(SettingsUpdate::default());
    };
    Ok(SettingsUpdate {
        backend: parse_setting_update(object.get("backend"), "backend")?,
        compatibility_mode: parse_setting_update(
            object.get("compatibilityMode"),
            "compatibilityMode",
        )?,
    })
}

fn parse_setting_update<T: DeserializeOwned>(
    value: Option<&Value>,
    name: &str,
) -> Result<SettingUpdate<T>, String> {
    match value {
        None => Ok(SettingUpdate::Unchanged),
        Some(Value::Null) => Ok(SettingUpdate::Reset),
        Some(value) => serde_json::from_value(value.clone())
            .map(SettingUpdate::Set)
            .map_err(|error| format!("invalid acdc-lsp {name} setting: {error}")),
    }
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
    use serde_json::from_str;

    #[test]
    fn analysis_preserves_includes_without_loading_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(directory.join("Cargo.toml").is_file());
        let source = "include::Cargo.toml[]";
        let profiles = ParserProfiles::new();
        let mut options = profiles.get(ServerOptions::default()).clone();
        options.base_dir = Some(directory.to_path_buf());
        let result = acdc_parser::parse(source, &options)?;
        assert_eq!(result.preprocessed_source(), source);
        assert!(result.source_recovery().is_some());
        assert_eq!(options.safe_mode, acdc_parser::SafeMode::Server);
        Ok(())
    }

    #[test]
    fn defaults_to_asciidoctor_html5_backend() {
        let profiles = ParserProfiles::new();
        let options = profiles.get(ServerOptions::default());

        assert_eq!(
            options
                .document_attributes()
                .get("backend")
                .and_then(|value| value.text()),
            Some("html5")
        );
        assert!(options.document_attributes().contains_key("backend-html5"));
        assert!(!options.document_attributes().contains_key("backend-pdf"));
    }

    #[test]
    fn deserializes_configured_pdf_backend() -> Result<(), serde_json::Error> {
        let options: ServerOptions = from_str(r#"{"backend":"pdf"}"#)?;

        assert_eq!(options.backend, AnalysisBackend::Pdf);
        let profiles = ParserProfiles::new();
        let parser_options = profiles.get(options);
        assert_eq!(
            parser_options
                .document_attributes()
                .get("backend")
                .and_then(|value| value.text()),
            Some("pdf")
        );
        assert!(
            parser_options
                .document_attributes()
                .contains_key("backend-pdf")
        );
        Ok(())
    }

    #[test]
    fn canonicalizes_asciidoctor_backend_aliases() -> Result<(), serde_json::Error> {
        let html: ServerOptions = from_str(r#"{"backend":"html"}"#)?;
        let docbook: ServerOptions = from_str(r#"{"backend":"docbook"}"#)?;

        assert_eq!(html.backend, AnalysisBackend::Html5);
        assert_eq!(docbook.backend, AnalysisBackend::Docbook5);
        Ok(())
    }

    #[test]
    fn most_specific_workspace_root_wins() -> Result<(), Box<dyn std::error::Error>> {
        let outer = "file:///workspace".parse::<Uri>()?;
        let inner = "file:///workspace/manual".parse::<Uri>()?;
        let document = "file:///workspace/manual/index.adoc".parse::<Uri>()?;
        let mut configuration = AnalysisConfiguration::new(
            crate::config::ServerOptions {
                backend: AnalysisBackend::Html5,
                ..Default::default()
            },
            vec![outer.clone(), inner.clone()],
        );
        configuration.set_unscoped(crate::config::WorkspaceSettings {
            backend: Some(AnalysisBackend::Markdown),
            ..Default::default()
        });
        configuration.replace_roots(vec![
            RootConfiguration {
                uri: outer,
                settings: crate::config::WorkspaceSettings {
                    backend: Some(AnalysisBackend::Docbook5),
                    ..Default::default()
                },
            },
            RootConfiguration {
                uri: inner,
                settings: crate::config::WorkspaceSettings {
                    backend: Some(AnalysisBackend::Pdf),
                    ..Default::default()
                },
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
        let mut configuration = AnalysisConfiguration::new(
            crate::config::ServerOptions {
                backend: AnalysisBackend::Html5,
                ..Default::default()
            },
            Vec::new(),
        );
        configuration.set_unscoped(crate::config::WorkspaceSettings {
            backend: Some(AnalysisBackend::Markdown),
            ..Default::default()
        });
        configuration.replace_roots(vec![
            RootConfiguration {
                uri: outer,
                settings: crate::config::WorkspaceSettings {
                    backend: Some(AnalysisBackend::Docbook5),
                    ..Default::default()
                },
            },
            RootConfiguration {
                uri: inner,
                settings: crate::config::WorkspaceSettings {
                    backend: None,
                    ..Default::default()
                },
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
            parse_settings_update(&serde_json::json!({"backend": "pdf"})),
            Ok(SettingsUpdate {
                backend: SettingUpdate::Set(AnalysisBackend::Pdf),
                ..SettingsUpdate::default()
            })
        );
        assert_eq!(
            parse_settings_update(&serde_json::json!({"acdc-lsp": {"backend": "docbook"}})),
            Ok(SettingsUpdate {
                backend: SettingUpdate::Set(AnalysisBackend::Docbook5),
                ..SettingsUpdate::default()
            })
        );
        assert_eq!(
            parse_settings_update(&serde_json::json!({"acdcLsp": {"backend": null}})),
            Ok(SettingsUpdate {
                backend: SettingUpdate::Reset,
                ..SettingsUpdate::default()
            })
        );
        assert_eq!(
            parse_settings_update(&serde_json::json!({"unrelated": true})),
            Ok(SettingsUpdate::default())
        );
    }
    #[test]
    fn compatibility_mode_defaults_validation_and_independent_updates()
    -> Result<(), Box<dyn std::error::Error>> {
        let default: ServerOptions = from_str("{}")?;
        assert_eq!(default.compatibility_mode, CompatibilityMode::Asciidoctor);
        let fallback: ServerOptions = from_str(r#"{"backend":"pdf","compatibilityMode":"draft"}"#)?;
        let uri = "file:///document.adoc".parse::<Uri>()?;
        let mut configuration = AnalysisConfiguration::new(fallback, Vec::new());
        for (json, backend, mode) in [
            (
                serde_json::json!({"compatibilityMode":"asciidoctor"}),
                AnalysisBackend::Pdf,
                CompatibilityMode::Asciidoctor,
            ),
            (
                serde_json::json!({"acdc-lsp":{"backend":"markdown"}}),
                AnalysisBackend::Markdown,
                CompatibilityMode::Asciidoctor,
            ),
            (
                serde_json::json!({"acdcLsp":{"compatibilityMode":null}}),
                AnalysisBackend::Markdown,
                CompatibilityMode::Draft,
            ),
            (
                serde_json::json!({"backend":null}),
                AnalysisBackend::Pdf,
                CompatibilityMode::Draft,
            ),
        ] {
            configuration.apply_update(parse_settings_update(&json)?);
            assert_eq!(
                configuration.settings_for(&uri),
                ServerOptions {
                    backend,
                    compatibility_mode: mode
                }
            );
        }
        for invalid in [
            serde_json::json!("Draft"),
            serde_json::json!(""),
            serde_json::json!(true),
        ] {
            assert!(
                serde_json::from_value::<ServerOptions>(
                    serde_json::json!({"compatibilityMode":invalid})
                )
                .is_err()
            );
            assert!(
                serde_json::from_value::<WorkspaceSettings>(
                    serde_json::json!({"compatibilityMode":invalid})
                )
                .is_err()
            );
            assert!(
                parse_settings_update(
                    &serde_json::json!({"backend":"html5","compatibilityMode":invalid})
                )
                .is_err()
            );
            assert_eq!(configuration.settings_for(&uri), fallback);
        }
        Ok(())
    }

    #[test]
    fn compatibility_mode_resource_settings_inherit_each_field_independently()
    -> Result<(), Box<dyn std::error::Error>> {
        let outer = "file:///workspace".parse::<Uri>()?;
        let inner = "file:///workspace/manual".parse::<Uri>()?;
        let document = "file:///workspace/manual/index.adoc".parse::<Uri>()?;
        let outside = "file:///elsewhere/index.adoc".parse::<Uri>()?;
        let mut configuration = AnalysisConfiguration::new(
            ServerOptions {
                backend: AnalysisBackend::Pdf,
                compatibility_mode: CompatibilityMode::Draft,
            },
            Vec::new(),
        );
        configuration.set_unscoped(WorkspaceSettings {
            backend: Some(AnalysisBackend::Markdown),
            compatibility_mode: None,
        });
        configuration.replace_roots(vec![
            RootConfiguration {
                uri: outer,
                settings: WorkspaceSettings {
                    backend: Some(AnalysisBackend::Docbook5),
                    compatibility_mode: Some(CompatibilityMode::Asciidoctor),
                },
            },
            RootConfiguration {
                uri: inner.clone(),
                settings: WorkspaceSettings {
                    backend: Some(AnalysisBackend::Html5s),
                    compatibility_mode: None,
                },
            },
        ]);
        assert_eq!(
            configuration.settings_for(&document),
            ServerOptions {
                backend: AnalysisBackend::Html5s,
                compatibility_mode: CompatibilityMode::Draft,
            }
        );
        assert_eq!(
            configuration.settings_for(&outside),
            ServerOptions {
                backend: AnalysisBackend::Markdown,
                compatibility_mode: CompatibilityMode::Draft,
            }
        );
        configuration.replace_roots(vec![RootConfiguration {
            uri: inner,
            settings: WorkspaceSettings {
                backend: None,
                compatibility_mode: Some(CompatibilityMode::Asciidoctor),
            },
        }]);
        assert_eq!(
            configuration.settings_for(&document),
            ServerOptions {
                backend: AnalysisBackend::Markdown,
                compatibility_mode: CompatibilityMode::Asciidoctor,
            }
        );
        Ok(())
    }

    #[test]
    fn compatibility_mode_parser_options_keep_backend_and_resource_policy() {
        let profiles = ParserProfiles::new();
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            for backend in [
                AnalysisBackend::Html5,
                AnalysisBackend::Html5s,
                AnalysisBackend::Docbook5,
                AnalysisBackend::Manpage,
                AnalysisBackend::Markdown,
                AnalysisBackend::Pdf,
                AnalysisBackend::Terminal,
            ] {
                let options = profiles.get(ServerOptions {
                    backend,
                    compatibility_mode: mode,
                });
                assert_eq!(options.compatibility_mode, mode);
                assert_eq!(options.safe_mode, acdc_parser::SafeMode::Server);
                assert!(matches!(
                    options.include_loader,
                    acdc_parser::IncludeLoader::Disabled
                ));
                let expected = match backend {
                    AnalysisBackend::Html5 => "html5",
                    AnalysisBackend::Html5s => "html5s",
                    AnalysisBackend::Docbook5 => "docbook5",
                    AnalysisBackend::Manpage => "manpage",
                    AnalysisBackend::Markdown => "markdown",
                    AnalysisBackend::Pdf => "pdf",
                    AnalysisBackend::Terminal => "terminal",
                };
                assert_eq!(
                    options
                        .document_attributes()
                        .get("backend")
                        .and_then(|value| value.text()),
                    Some(expected)
                );
            }
        }
    }
}
