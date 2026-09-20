mod ast_highlight;
mod editor;

use wasm_bindgen::prelude::*;

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::AttributeValue;

/// Result of a single parse operation: highlighted source + rendered preview.
pub struct ParseResult {
    /// Source text with `<span class="adoc-*">` highlighting.
    pub highlight_html: String,
    /// Rendered HTML preview.
    pub preview_html: String,
    /// Whether STEM is enabled in the document, including body and nested content.
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
    pub line: Option<u32>,
    pub column: Option<u32>,
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
/// # Errors
///
/// Returns an error string if parsing or conversion fails.
pub fn parse_and_render(input: &str) -> Result<ParseResult, String> {
    parse_and_render_with_attributes(input, [("source-highlighter", AttributeValue::Bool(true))])
}

fn parse_and_render_with_attributes<'a, N, V>(
    input: &str,
    document_attributes: impl IntoIterator<Item = (N, V)>,
) -> Result<ParseResult, String>
where
    N: Into<std::borrow::Cow<'a, str>>,
    V: Into<AttributeValue<'a>>,
{
    let processor = Processor::new_with_variant(
        Options::builder().embedded(true).build(),
        acdc_parser::Options::builder().with_attributes(document_attributes),
        HtmlVariant::Semantic,
    )
    .map_err(|e| e.to_string())?;
    let mut parsed =
        acdc_parser::parse(input, processor.parser_options()).map_err(|e| format!("{e}"))?;

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

    let has_stem = acdc_converters_html::uses_stem(document);

    Ok(ParseResult {
        highlight_html,
        preview_html,
        has_stem,
        warnings,
    })
}

fn location_line_col(loc: Option<&acdc_parser::SourceLocation>) -> (Option<u32>, Option<u32>) {
    let Some(loc) = loc else {
        return (None, None);
    };
    (
        Some(loc.location.start.line),
        Some(loc.location.start.column),
    )
}

#[cfg(test)]
mod tests {
    use acdc_parser::DocumentAttributeValue;

    use super::*;

    #[test]
    fn editor_uses_resolved_flags() -> Result<(), String> {
        assert!(parse_and_render(":stem:\n\n$x$")?.has_stem);
        assert!(!parse_and_render(":stem!:\n\n$x$")?.has_stem);
        Ok(())
    }

    #[test]
    fn editor_enables_math_after_body_and_cell_assignments() -> Result<(), String> {
        for source in [
            "= Math\n\nBefore.\n\n:stem: latexmath\n\nstem:[x^2]\n",
            "= Math\n\n[cols=a]\n|===\n|\n:stem: latexmath\n\nstem:[x^2]\n|===\n",
        ] {
            let rendered = parse_and_render(source)?;
            assert!(rendered.has_stem);
            assert!(rendered.preview_html.contains("x^2"));
        }
        Ok(())
    }

    #[test]
    fn editor_preserves_original_numeric_text() -> Result<(), String> {
        let attributes = acdc_parser::Options::with_attributes([("max-include-depth", "064")])
            .map_err(|e| e.to_string())?
            .into_document_attributes();
        assert_eq!(
            attributes
                .get("max-include-depth")
                .and_then(DocumentAttributeValue::as_integer),
            Some(64)
        );
        assert_eq!(
            attributes
                .get("max-include-depth")
                .and_then(DocumentAttributeValue::text),
            Some("064")
        );

        let result = parse_and_render_with_attributes(
            "depth={max-include-depth}",
            attributes.into_inputs(),
        )?;
        assert!(result.preview_html.contains("depth=064"));
        Ok(())
    }
    #[test]
    fn editor_parsing_uses_the_selected_backend() -> Result<(), String> {
        let result = parse_and_render(
            "ifdef::backend-html5s[]\nSemantic preview.\nendif::[]\n\nbackend={backend}\n",
        )?;
        assert!(result.preview_html.contains("Semantic preview."));
        assert!(result.preview_html.contains("backend=html5s"));
        Ok(())
    }
}
