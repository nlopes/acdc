mod ast_highlight;
mod editor;

use wasm_bindgen::prelude::*;

use acdc_converters_core::Options;
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{AttributeValue, DocumentAttributes};

/// Result of a single parse operation: highlighted source + rendered preview.
pub struct ParseResult {
    /// Source text with `<span class="adoc-*">` highlighting.
    pub highlight_html: String,
    /// Rendered HTML preview.
    pub preview_html: String,
    /// Whether the document has `:stem:` set (needs MathJax).
    pub has_stem: bool,
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
        String::from("source-highlighter"),
        AttributeValue::Bool(true),
    );
    let options = acdc_parser::Options::builder()
        .with_attributes(document_attributes)
        .build();
    let document = acdc_parser::parse(input, &options).map_err(|e| format!("{e}"))?;

    let highlight_html = ast_highlight::highlight_from_ast(input, &document);

    let html_options = Options::builder().embedded(true).build();
    let processor = Processor::new_with_variant(
        html_options,
        document.attributes.clone(),
        HtmlVariant::Semantic,
    );
    let render_options = RenderOptions {
        embedded: true,
        ..RenderOptions::default()
    };

    let preview_html = processor
        .convert_to_string(&document, &render_options)
        .map_err(|e| format!("{e}"))?;

    let has_stem = document.attributes.get("stem").is_some();

    Ok(ParseResult {
        highlight_html,
        preview_html,
        has_stem,
    })
}
