//! On-demand previews of editor buffers in the system's external viewer.

use std::{
    borrow::Cow,
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use acdc_converters_core::{Converter, Diagnostics, Options};
use acdc_converters_html::{HtmlVariant, Processor};
use acdc_parser::{IncludeLoader, SafeMode};
use lol_html::{RewriteStrSettings, element, html_content::ContentType, rewrite_str};
use tempfile::{NamedTempFile, TempDir};
use tower_lsp_server::{
    jsonrpc::{Error, ErrorCode},
    ls_types::{
        CodeAction, CodeActionContext, CodeActionKind, Command, Uri, error_codes::REQUEST_FAILED,
    },
};

use crate::{config::AnalysisBackend, convert::file_path_to_uri};

pub(crate) const COMMAND: &str = "acdc.preview";
pub(crate) const HTML_COMMAND: &str = "acdc.previewHtml";
const PDF_UNAVAILABLE: &str = "PDF preview requires an acdc-lsp build with the preview-pdf feature";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PreviewFormat {
    Html,
    SemanticHtml,
    Pdf,
}

impl PreviewFormat {
    pub(crate) fn for_backend(backend: AnalysisBackend) -> Option<Self> {
        match backend {
            AnalysisBackend::Html5 => Some(Self::Html),
            AnalysisBackend::Html5s => Some(Self::SemanticHtml),
            AnalysisBackend::Pdf => Some(Self::Pdf),
            AnalysisBackend::Docbook5
            | AnalysisBackend::Manpage
            | AnalysisBackend::Markdown
            | AnalysisBackend::Terminal => None,
        }
    }

    const fn available(self) -> bool {
        !matches!(self, Self::Pdf) || cfg!(feature = "preview-pdf")
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Html => "Preview AsciiDoc as HTML",
            Self::SemanticHtml => "Preview AsciiDoc as semantic HTML",
            Self::Pdf => "Preview AsciiDoc as PDF",
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::Html | Self::SemanticHtml => "html",
            Self::Pdf => "pdf",
        }
    }
}

pub(crate) fn request_failed(message: impl Into<Cow<'static, str>>) -> Error {
    Error {
        code: ErrorCode::ServerError(REQUEST_FAILED),
        message: message.into(),
        data: None,
    }
}

pub(crate) fn code_actions(
    uri: &Uri,
    context: &CodeActionContext,
    backend: AnalysisBackend,
) -> Vec<CodeAction> {
    if context.only.as_ref().is_some_and(|kinds| {
        !kinds
            .iter()
            .any(|kind| kind.as_str().is_empty() || *kind == CodeActionKind::SOURCE)
    }) {
        return Vec::new();
    }
    let mut actions = Vec::new();
    if let Some(format) = PreviewFormat::for_backend(backend)
        && format.available()
    {
        actions.push(code_action(uri, COMMAND, format.title()));
    }
    if backend != AnalysisBackend::Html5 {
        actions.push(code_action(uri, HTML_COMMAND, PreviewFormat::Html.title()));
    }
    actions
}

fn code_action(uri: &Uri, command: &str, title: &str) -> CodeAction {
    CodeAction {
        title: title.into(),
        kind: Some(CodeActionKind::SOURCE),
        command: Some(Command {
            title: title.into(),
            command: command.into(),
            arguments: Some(vec![serde_json::json!(uri)]),
        }),
        ..CodeAction::default()
    }
}

/// Keeps preview URLs valid until shutdown, including after a buffer is closed.
#[derive(Default)]
pub(crate) struct PreviewStore {
    directory: Option<TempDir>,
    files: HashMap<(Uri, PreviewFormat), PathBuf>,
}

pub(crate) struct RenderedPreview {
    pub(crate) uri: Uri,
    pub(crate) warnings: Vec<String>,
}

pub(crate) struct PreviewInput {
    pub(crate) format: PreviewFormat,
    pub(crate) source_uri: Uri,
    pub(crate) text: Arc<String>,
    pub(crate) base_dir: Option<PathBuf>,
    pub(crate) include_loader: IncludeLoader,
}

impl PreviewStore {
    pub(crate) fn render(&mut self, input: &PreviewInput) -> Result<RenderedPreview, String> {
        if !input.format.available() {
            return Err(PDF_UNAVAILABLE.into());
        }
        if self.directory.is_none() {
            self.directory = Some(
                tempfile::Builder::new()
                    .prefix("acdc-lsp-preview-")
                    .tempdir()
                    .map_err(|error| format!("Could not create preview directory: {error}"))?,
            );
        }
        let directory = self.directory.as_ref().ok_or("Missing preview directory")?;
        // Browser launchers resolve symlinks; fragment links must use that same URL.
        let directory_path = directory
            .path()
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let key = (input.source_uri.clone(), input.format);
        let path = self.files.get(&key).cloned().unwrap_or_else(|| {
            directory_path.join(format!(
                "preview-{}.{}",
                self.files.len(),
                input.format.extension()
            ))
        });
        let uri = file_path_to_uri(&path).ok_or("Could not create preview file URI")?;
        let (bytes, warnings) = match input.format {
            PreviewFormat::Html => render_html(input, &uri, HtmlVariant::Standard)?,
            PreviewFormat::SemanticHtml => render_html(input, &uri, HtmlVariant::Semantic)?,
            PreviewFormat::Pdf => render_pdf(input)?,
        };

        // Replace the previous snapshot only after conversion and writing succeed.
        let mut file =
            NamedTempFile::new_in(directory.path()).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.persist(&path).map_err(|error| error.to_string())?;
        self.files.insert(key, path);
        Ok(RenderedPreview { uri, warnings })
    }
}

fn parser_options(input: &PreviewInput) -> acdc_parser::OptionsBuilder<'static> {
    let mut parser_options = acdc_parser::Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_include_loader(input.include_loader.clone());
    if let Some(directory) = &input.base_dir {
        parser_options = parser_options.with_base_dir(directory);
    }
    parser_options
}

fn convert<'a>(
    input: &PreviewInput,
    processor: &impl Converter<'a>,
    source_file: Option<&Path>,
) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut parsed = acdc_parser::parse(&input.text, processor.parser_options())
        .map_err(|error| format!("Could not parse preview: {error}"))?;
    let mut warnings: Vec<String> = parsed
        .take_warnings()
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut bytes = Vec::new();
    let mut converter_warnings = Vec::new();
    let source = processor.warning_source();
    processor
        .write_to(
            parsed.document(),
            &mut bytes,
            source_file,
            None,
            &mut Diagnostics::new(&source, &mut converter_warnings),
        )
        .map_err(|error| format!("Could not render preview: {error}"))?;
    warnings.extend(converter_warnings.iter().map(ToString::to_string));
    Ok((bytes, warnings))
}

#[cfg(feature = "preview-pdf")]
fn render_pdf(input: &PreviewInput) -> Result<(Vec<u8>, Vec<String>), String> {
    // Untitled buffers have no directory to confine image reads to.
    let safe_mode = if input.base_dir.is_some() {
        SafeMode::Server
    } else {
        SafeMode::Secure
    };
    let processor = acdc_converters_pdf::Processor::new(
        Options::builder().safe_mode(safe_mode).build(),
        parser_options(input).with_attribute("allow-uri-read", false),
    )
    .map_err(|error| error.to_string())?;
    let source_file = input
        .base_dir
        .as_ref()
        .and_then(|_| input.source_uri.to_file_path());
    convert(input, &processor, source_file.as_deref())
}

#[cfg(not(feature = "preview-pdf"))]
fn render_pdf(_input: &PreviewInput) -> Result<(Vec<u8>, Vec<String>), String> {
    Err(PDF_UNAVAILABLE.into())
}

fn render_html(
    input: &PreviewInput,
    preview_uri: &Uri,
    variant: HtmlVariant,
) -> Result<(Vec<u8>, Vec<String>), String> {
    let parser_options = parser_options(input)
        .with_attribute("stylesheet", "")
        .with_attribute("linkcss", false)
        .with_attribute("docinfo", false)
        .with_attribute("csp", true);
    let processor = Processor::new_with_variant(
        Options::builder().safe_mode(SafeMode::Secure).build(),
        parser_options,
        variant,
    )
    .map_err(|error| error.to_string())?;
    let (bytes, warnings) = convert(input, &processor, None)?;
    if input.base_dir.is_none() {
        return Ok((bytes, warnings));
    }
    let html = String::from_utf8(bytes).map_err(|error| error.to_string())?;
    let base = input
        .source_uri
        .as_str()
        .replace('&', "&amp;")
        .replace('"', "&quot;");
    let html = rewrite_str(
        &html,
        RewriteStrSettings::new()
            .append_element_content_handler(element!("head", |element| {
                element.prepend(&format!("<base href=\"{base}\">\n"), ContentType::Html);
                Ok(())
            }))
            // A base URL also affects #fragment links; those must stay in the preview.
            .append_element_content_handler(element!("[href]", |element| {
                if let Some(href) = element.get_attribute("href")
                    && (href.is_empty() || href.starts_with('#'))
                {
                    element.set_attribute("href", &format!("{}{href}", preview_uri.as_str()))?;
                }
                Ok(())
            })),
    )
    .map_err(|error| error.to_string())?;
    Ok((html.into_bytes(), warnings))
}
