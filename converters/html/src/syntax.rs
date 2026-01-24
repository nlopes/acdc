//! Syntax highlighting for source code blocks using syntect.
//!
//! This module provides syntax highlighting for code blocks when the
//! `highlighting` feature is enabled. It outputs HTML spans with inline
//! styles for colors.

use std::io::Write;

use acdc_parser::InlineNode;

use crate::Error;

#[cfg(feature = "highlighting")]
const CODE_HIGHLIGHT_THEME: &str = "InspiredGitHub";

/// Highlight code and write HTML output with inline styles.
///
/// When the `highlighting` feature is enabled, this uses syntect for syntax
/// highlighting with inline CSS styles. Otherwise, it outputs plain escaped text.
#[cfg(feature = "highlighting")]
pub fn highlight_code<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    language: &str,
) -> Result<(), Error> {
    use syntect::{highlighting::ThemeSet, html::highlighted_html_for_string, parsing::SyntaxSet};

    let code = extract_text_from_inlines(inlines);
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();

    // Use InspiredGitHub theme for a clean look in HTML
    let Some(theme) = theme_set.themes.get(CODE_HIGHLIGHT_THEME) else {
        return highlight_code_fallback(writer, inlines, language);
    };

    let syntax = syntax_set
        .find_syntax_by_token(language)
        .or_else(|| syntax_set.find_syntax_by_extension(language))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let html = highlighted_html_for_string(&code, &syntax_set, syntax, theme)
        .map_err(|e| Error::Io(std::io::Error::other(e)))?;

    // syntect wraps output in <pre style="..."> which we don't want
    // since we already have our own <pre> wrapper. Extract just the inner content.
    let content = extract_inner_content(&html);
    write!(writer, "{content}")?;

    Ok(())
}

/// Extract the inner content from syntect's HTML output.
/// Syntect wraps everything in `<pre style="...">...</pre>`, but we want just the spans.
#[cfg(feature = "highlighting")]
fn extract_inner_content(html: &str) -> &str {
    // Find the end of the opening <pre> tag
    let start = html.find('>').map_or(0, |i| i + 1);

    // Find the start of the closing </pre> tag
    let end = html.rfind("</pre>").unwrap_or(html.len());

    html.get(start..end).unwrap_or(html)
}

/// HTML-escape special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Fallback implementation when syntax highlighting is disabled.
/// Outputs plain HTML-escaped text without any highlighting.
fn highlight_code_fallback<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    _language: &str,
) -> Result<(), Error> {
    let code = extract_text_from_inlines(inlines);
    let escaped = html_escape(&code);
    write!(writer, "{escaped}")?;
    Ok(())
}

/// Highlight code when the highlighting feature is not enabled.
///
/// This simply calls `highlight_code_fallback`.
#[cfg(not(feature = "highlighting"))]
pub(crate) fn highlight_code<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    language: &str,
) -> Result<(), Error> {
    highlight_code_fallback(writer, inlines, language)
}

/// Extract text content from inline nodes for highlighting.
fn extract_text_from_inlines(inlines: &[InlineNode]) -> String {
    let mut result = String::new();

    for node in inlines {
        #[allow(clippy::match_same_arms)]
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
            // Code blocks should only contain verbatim/plain text - ignore other node types
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
            | InlineNode::Macro(_) => {}
            // Required for #[non_exhaustive] enum - future variants are ignored
            node => {
                tracing::warn!(
                    ?node,
                    "this type of node is not yet implemented for code highlighting"
                );
            }
        }
    }
    result
}

#[cfg(all(test, feature = "highlighting"))]
mod tests {
    use super::*;
    use acdc_parser::{Location, Verbatim};

    fn create_verbatim_inlines(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::VerbatimText(Verbatim {
            content: content.to_string(),
            location: Location::default(),
        })]
    }

    #[test]
    fn test_extract_text_from_verbatim() {
        let inlines = create_verbatim_inlines("fn main() {\n    println!(\"Hello\");\n}");
        let text = extract_text_from_inlines(&inlines);
        assert_eq!(text, "fn main() {\n    println!(\"Hello\");\n}");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_highlight_rust_code() -> Result<(), Error> {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let inlines = create_verbatim_inlines(code);

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "rust")?;

        let html = String::from_utf8(buffer).expect("valid utf8");
        // Verify it contains span elements for highlighting
        assert!(
            html.contains("<span"),
            "Should produce highlighted output with spans"
        );

        Ok(())
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_highlight_unknown_language_fallback() -> Result<(), Error> {
        let code = "some code here";
        let inlines = create_verbatim_inlines(code);

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "unknown_lang_xyz")?;

        // Should fall back to plain text and not crash
        assert!(
            !buffer.is_empty(),
            "Should produce output even with unknown language"
        );
        assert_eq!(
            std::str::from_utf8(&buffer).expect("valid utf8"),
            "\n<span style=\"color:#323232;\">some code here</span>"
        );
        Ok(())
    }
}
