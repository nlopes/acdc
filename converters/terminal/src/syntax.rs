use std::io::Write;

use acdc_parser::InlineNode;

use crate::{Error, Processor};

/// Get or initialize the shared giallo `Registry`.
///
/// The registry is loaded once from the builtin dump on first use and
/// then reused for all subsequent calls.  This is thread-safe via
/// `OnceLock`.
#[cfg(feature = "highlighting")]
fn get_registry() -> &'static giallo::Registry {
    use std::sync::OnceLock;

    static REGISTRY: OnceLock<giallo::Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = giallo::Registry::builtin().unwrap_or_else(|e| {
            tracing::error!("failed to load builtin giallo registry: {e}");
            giallo::Registry::default()
        });
        registry.link_grammars();
        registry
    })
}

/// Highlight code and render to terminal.
///
/// When the `highlighting` feature is enabled, this uses giallo for syntax
/// highlighting with ANSI escape codes. Otherwise, it outputs plain text.
#[cfg(feature = "highlighting")]
pub(crate) fn highlight_code<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    language: &str,
    processor: &Processor,
) -> Result<(), Error> {
    let code = extract_text_from_inlines(inlines);
    let registry = get_registry();

    let theme_name = processor.appearance.theme.highlight_theme();
    let theme_variant = giallo::ThemeVariant::Single(theme_name);
    let options = giallo::HighlightOptions::new(language, theme_variant).fallback_to_plain(true);

    let highlighted = match registry.highlight(&code, &options) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("giallo highlighting failed for language '{language}': {e}");
            write!(writer, "{code}")?;
            return Ok(());
        }
    };

    let renderer = giallo::TerminalRenderer::default();
    let render_options = giallo::RenderOptions::default();
    let ansi = renderer.render(&highlighted, &render_options);
    write!(writer, "{ansi}")?;

    Ok(())
}

/// Fallback implementation when syntax highlighting is disabled.
/// Outputs plain text without any highlighting.
#[cfg(not(feature = "highlighting"))]
pub(crate) fn highlight_code<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    _language: &str,
    _processor: &Processor,
) -> Result<(), Error> {
    let code = extract_text_from_inlines(inlines);
    write!(writer, "{code}")?;
    Ok(())
}

/// Extract text content from inline nodes.
///
/// This handles `VerbatimText` (from literal/listing blocks) and `PlainText` nodes.
fn extract_text_from_inlines(inlines: &[InlineNode]) -> String {
    let mut result = String::new();

    for node in inlines {
        match node {
            InlineNode::VerbatimText(verbatim) => {
                result.push_str(&verbatim.content);
            }
            InlineNode::RawText(raw) => {
                result.push_str(&raw.content);
            }
            InlineNode::PlainText(plain) => {
                result.push_str(&plain.content);
            }
            InlineNode::LineBreak(_) => {
                result.push('\n');
            }
            InlineNode::CalloutRef(callout) => {
                use std::fmt::Write;
                let _ = write!(result, "<{}>", callout.number);
            }
            InlineNode::BoldText(_)
            | InlineNode::ItalicText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::SuperscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::InlineAnchor(_)
            | _ => {
                // For other node types, recurse or ignore
                // In practice, code blocks should only contain verbatim/plain text
            }
        }
    }
    result
}

#[cfg(all(test, feature = "highlighting"))]
mod tests {
    use super::*;
    use acdc_converters_core::Options;
    use acdc_parser::{DocumentAttributes, Location, Verbatim};
    use std::{cell::Cell, rc::Rc};

    fn create_verbatim_inlines(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::VerbatimText(Verbatim {
            content: content.to_string(),
            location: Location::default(),
        })]
    }

    fn create_test_processor() -> Processor {
        use crate::Appearance;
        use acdc_converters_core::section::{
            AppendixTracker, PartNumberTracker, SectionNumberTracker,
        };
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        let appearance = Appearance::detect();
        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker =
            PartNumberTracker::new(&document_attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width: crate::FALLBACK_TERMINAL_WIDTH,
            index_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }

    #[test]
    fn test_extract_text_from_verbatim() {
        let inlines = create_verbatim_inlines("fn main() {\n    println!(\"Hello\");\n}");
        let text = extract_text_from_inlines(&inlines);
        assert_eq!(text, "fn main() {\n    println!(\"Hello\");\n}");
    }

    #[test]
    fn test_highlight_rust_code() -> Result<(), Error> {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let inlines = create_verbatim_inlines(code);
        let processor = create_test_processor();

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "rust", &processor)?;

        // Just verify it doesn't crash and produces output
        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }

    #[test]
    fn test_highlight_unknown_language_fallback() -> Result<(), Error> {
        let code = "some code here";
        let inlines = create_verbatim_inlines(code);
        let processor = create_test_processor();

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "unknown_lang_xyz", &processor)?;

        // Should fall back to plain text and not crash
        assert!(
            !buffer.is_empty(),
            "Should produce output even with unknown language"
        );

        Ok(())
    }

    #[test]
    fn test_highlight_python_code() -> Result<(), Error> {
        let code = "def hello():\n    print('Hello, world!')";
        let inlines = create_verbatim_inlines(code);
        let processor = create_test_processor();

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "python", &processor)?;

        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }

    #[test]
    fn test_highlight_javascript_code() -> Result<(), Error> {
        let code = "function hello() {\n  console.log('Hello, world!');\n}";
        let inlines = create_verbatim_inlines(code);
        let processor = create_test_processor();

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "javascript", &processor)?;

        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }
}
