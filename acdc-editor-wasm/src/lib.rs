mod ast_highlight;
mod editor;

#[cfg(test)]
mod test_sources;

use wasm_bindgen::prelude::*;

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::AttributeValue;

/// Result of a single parse operation: highlighted source + rendered preview.
pub struct ParseResult {
    /// Source text with `<span class="adoc-*">` highlighting.
    pub highlight_html: String,
    /// Rendered HTML preview. This output is not sanitized.
    /// Sanitize or isolate it before displaying documents you do not trust.
    pub preview_html: String,
    /// Whether STEM is enabled in the document, including body and nested content.
    pub has_stem: bool,
    /// Non-fatal warnings from both the parser and the converter, normalized
    /// into a single editor-facing shape so the UI does not need to care
    /// which layer produced them.
    pub warnings: Vec<EditorWarning>,
}

/// A non-fatal parser or converter warning with its original source position.
pub struct EditorWarning {
    pub message: String,
    pub advice: Option<String>,
    /// Source filename, when known. `None` for unnamed input.
    pub file: Option<String>,
    /// One-based line number in the source file.
    pub line: Option<u32>,
    /// One-based column counted in Unicode scalar values.
    pub column: Option<u32>,
}

impl EditorWarning {
    fn new(
        message: String,
        advice: Option<String>,
        location: Option<&acdc_parser::SourceLocation>,
        parsed: &acdc_parser::ParseResult,
    ) -> Self {
        let resolved = location.map(|loc| parsed.source_location(&loc.location));
        let file = location
            .and_then(|loc| loc.file.as_ref())
            .or_else(|| resolved.as_ref().and_then(|loc| loc.file.as_ref()))
            .map(|file| file.to_string_lossy().into_owned());
        Self {
            message,
            advice,
            file,
            line: location.map(|loc| loc.location.start.line),
            column: location.map(|loc| loc.location.start.column),
        }
    }
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
    let parsed =
        acdc_parser::parse(input, processor.parser_options()).map_err(|e| format!("{e}"))?;

    render_parsed(input, parsed, &processor)
}

fn render_parsed(
    input: &str,
    mut parsed: acdc_parser::ParseResult,
    processor: &Processor,
) -> Result<ParseResult, String> {
    let mut warnings: Vec<EditorWarning> = parsed
        .take_warnings()
        .into_iter()
        .map(|w| {
            let advice = w.advice().map(str::to_owned);
            EditorWarning::new(w.kind.to_string(), advice, w.source_location(), &parsed)
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
        EditorWarning::new(
            w.message.to_string(),
            w.advice.map(std::borrow::Cow::into_owned),
            w.location.as_ref(),
            &parsed,
        )
    }));

    let has_stem = acdc_converters_html::uses_stem(document);

    Ok(ParseResult {
        highlight_html,
        preview_html,
        has_stem,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use acdc_parser::DocumentAttributeValue;

    use super::*;

    #[test]
    fn warnings_keep_nested_source_files_and_unicode_positions()
    -> Result<(), Box<dyn std::error::Error>> {
        let input = "= Entry\n\ninclude::chapters/outer.adoc[]\n\nπ 😀 <<entry-missing>>\n";
        let options = test_sources::options(&[
            (
                "chapters/outer.adoc",
                "include::inner.adoc[lines=2..4,indent=0]\n",
            ),
            (
                "chapters/inner.adoc",
                "Not selected.\n    Selected paragraph.\n\n    π 😀 <<included-missing>>\n",
            ),
        ])?;
        let processor = Processor::new_with_variant(
            Options::builder().embedded(true).build(),
            options.into_builder(),
            HtmlVariant::Semantic,
        )?;
        let parsed = acdc_parser::parse(input, processor.parser_options())?;
        let result = render_parsed(input, parsed, &processor)?;
        let included = result
            .warnings
            .iter()
            .find(|warning| warning.message.contains("included-missing"))
            .ok_or("missing included warning")?;
        assert!(
            included
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("inner.adoc"))
        );
        assert_eq!((included.line, included.column), (Some(4), Some(9)));
        let entry = result
            .warnings
            .iter()
            .find(|warning| warning.message.contains("entry-missing"))
            .ok_or("missing entry warning")?;
        assert_eq!(entry.file, None);
        assert_eq!((entry.line, entry.column), (Some(5), Some(5)));
        Ok(())
    }

    #[test]
    fn warning_locations_resolve_include_chains() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            "include::part.adoc[]",
            &test_sources::options(&[("part.adoc", "π 😀 <<missing>>\n")])?,
        )?;
        let location = parsed
            .document()
            .blocks
            .first()
            .ok_or("missing block")?
            .location();
        let unresolved = acdc_parser::SourceLocation::at_location(None, location.clone());
        let warning = EditorWarning::new(
            "Converter warning".to_string(),
            None,
            Some(&unresolved),
            &parsed,
        );
        assert!(
            warning
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("part.adoc"))
        );
        assert_eq!((warning.line, warning.column), (Some(1), Some(1)));
        let unlocated = EditorWarning::new("Global warning".to_string(), None, None, &parsed);
        assert_eq!(
            (unlocated.file, unlocated.line, unlocated.column),
            (None, None, None)
        );
        Ok(())
    }

    #[test]
    fn single_document_parsing_keeps_includes_disabled() -> Result<(), String> {
        let result = parse_and_render("include::part.adoc[]")?;
        assert!(result.preview_html.contains("part.adoc"));
        assert!(result.warnings.iter().all(|warning| warning.file.is_none()));
        Ok(())
    }

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
