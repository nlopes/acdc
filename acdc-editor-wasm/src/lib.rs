mod ast_highlight;
mod editor;

use wasm_bindgen::prelude::*;

use acdc_converters_core::{Diagnostics, Options, WarningSource};
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{AttributeValue, DocumentAttributes, Positioning};

/// Result of a single parse operation: highlighted source + rendered preview.
pub struct ParseResult {
    /// Source text with `<span class="adoc-*">` highlighting.
    pub highlight_html: String,
    /// Rendered HTML preview.
    pub preview_html: String,
    /// Whether the document has `:stem:` set (needs `MathJax`).
    pub has_stem: bool,
    /// Non-fatal warnings from both the parser and the converter, normalized
    /// into a single editor-facing shape so the UI does not need to care
    /// which layer produced them.
    pub warnings: Vec<EditorWarning>,
}

/// Parser- and converter-agnostic view of a non-fatal warning, ready to
/// render in the editor's status badge.
pub struct EditorWarning {
    pub message: String,
    pub advice: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

/// Initialize panic hook and set up the editor DOM orchestration.
///
/// # Errors
///
/// Returns a `JsValue` error if any required DOM element is missing.
#[wasm_bindgen(start)]
pub fn init() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    editor::setup()
}

/// Parse `AsciiDoc` source once and produce both the syntax-highlighted source
/// HTML and the rendered preview HTML.
///
/// On parse error, returns escaped (unhighlighted) source and an empty preview.
///
/// # Errors
///
/// Returns an error string if parsing fails (the caller can use cached output).
pub fn parse_and_render(input: &str) -> Result<ParseResult, String> {
    let mut document_attributes = DocumentAttributes::default();
    document_attributes.insert(
        std::borrow::Cow::Borrowed("source-highlighter"),
        AttributeValue::Bool(true),
    );
    let options = acdc_parser::Options::builder()
        .with_attributes(document_attributes)
        .build();
    let mut parsed = acdc_parser::parse(input, &options).map_err(|e| format!("{e}"))?;

    let mut warnings: Vec<EditorWarning> = parsed
        .take_warnings()
        .into_iter()
        .map(|w| {
            let advice = w.advice().map(str::to_owned);
            let (line, column) = location_line_col(w.source_location());
            EditorWarning {
                message: w.kind.to_string(),
                advice,
                line,
                column,
            }
        })
        .collect();

    let document = parsed.document();
    let highlight_html = ast_highlight::highlight_from_ast(input, document);

    let html_options = Options::builder().embedded(true).build();
    let processor = Processor::new_with_variant(
        html_options,
        document.attributes.to_static(),
        HtmlVariant::Semantic,
    );
    let render_options = RenderOptions {
        embedded: true,
        ..RenderOptions::default()
    };

    let source = WarningSource::new("html").with_variant(HtmlVariant::Semantic.as_str());
    let mut converter_warnings = Vec::new();
    let mut diagnostics = Diagnostics::new(&source, &mut converter_warnings);
    let mut output = Vec::new();
    processor
        .convert_to_writer(document, &mut output, &render_options, &mut diagnostics)
        .map_err(|e| format!("{e}"))?;
    let preview_html = String::from_utf8(output).map_err(|e| format!("{e}"))?;

    warnings.extend(converter_warnings.into_iter().map(|w| {
        let (line, column) = location_line_col(w.source_location());
        EditorWarning {
            message: w.message.into_owned(),
            advice: w.advice.map(std::borrow::Cow::into_owned),
            line,
            column,
        }
    }));

    let has_stem = document.attributes.get("stem").is_some();

    Ok(ParseResult {
        highlight_html,
        preview_html,
        has_stem,
        warnings,
    })
}

fn location_line_col(loc: Option<&acdc_parser::SourceLocation>) -> (Option<usize>, Option<usize>) {
    let Some(loc) = loc else {
        return (None, None);
    };
    match &loc.positioning {
        Positioning::Location(l) => (Some(l.start.line), Some(l.start.column)),
        Positioning::Position(p) => (Some(p.line), Some(p.column)),
    }
}
