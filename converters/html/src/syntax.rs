//! Syntax highlighting for source code blocks using syntect.
//!
//! This module provides syntax highlighting for code blocks when the
//! `highlighting` feature is enabled. It outputs HTML spans with inline
//! styles for colors.
//!
//! # Callout Handling
//!
//! Code blocks may contain callout references (e.g., `<1>`, `<2>`) that need
//! to be rendered as HTML elements, not as part of the highlighted code.
//! We track callout positions during text extraction and inject the proper
//! HTML (`<i class="conum" data-value="N"></i><b>(N)</b>`) after highlighting.

use std::{collections::HashMap, io::Write};

use acdc_parser::InlineNode;

use crate::Error;

#[cfg(feature = "highlighting")]
const CODE_HIGHLIGHT_THEME_LIGHT: &str = "InspiredGitHub";
#[cfg(feature = "highlighting")]
const CODE_HIGHLIGHT_THEME_DARK: &str = "base16-eighties.dark";

/// Highlight code and write HTML output with inline styles.
///
/// When the `highlighting` feature is enabled, this uses syntect for syntax
/// highlighting with inline CSS styles. Otherwise, it outputs plain escaped text.
///
/// Callout references are preserved and rendered as proper HTML elements
/// after the highlighted code on each line.
#[cfg(feature = "highlighting")]
pub(crate) fn highlight_code<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    language: &str,
    dark_mode: bool,
) -> Result<(), Error> {
    use syntect::{highlighting::ThemeSet, html::highlighted_html_for_string, parsing::SyntaxSet};

    let (code, callouts) = extract_text_and_callouts(inlines);
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();

    let theme_name = if dark_mode {
        CODE_HIGHLIGHT_THEME_DARK
    } else {
        CODE_HIGHLIGHT_THEME_LIGHT
    };
    let Some(theme) = theme_set.themes.get(theme_name) else {
        return write_escaped_code_with_callouts(writer, inlines);
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

    // If no callouts, output directly
    if callouts.is_empty() {
        write!(writer, "{content}")?;
    } else {
        // Insert callout HTML at the end of each line that has one
        let output = insert_callouts_into_highlighted_html(content, &callouts);
        write!(writer, "{output}")?;
    }

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

/// Insert callout HTML at the appropriate line endings in highlighted code.
///
/// This function processes the highlighted HTML line-by-line and appends
/// callout markers at the end of lines that have callouts.
///
/// Note: syntect's output may have a leading newline which we need to skip
/// when counting lines, so actual code lines start at index 0 even if the
/// first split element is empty.
#[cfg(feature = "highlighting")]
fn insert_callouts_into_highlighted_html(html: &str, callouts: &HashMap<usize, usize>) -> String {
    let mut result = String::with_capacity(html.len() + callouts.len() * 50);

    // syntect adds a leading newline to its output - we need to handle this
    // when counting lines for callout placement
    let has_leading_newline = html.starts_with('\n');

    for (i, line) in html.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }

        // Adjust line number: if there's a leading newline, the first split result
        // is empty and actual code starts at index 1, so we subtract 1 from index
        // to get the correct code line number
        let code_line_num = if has_leading_newline && i > 0 {
            i - 1
        } else if has_leading_newline {
            // First element is empty (the leading newline), skip it for callouts
            result.push_str(line);
            continue;
        } else {
            i
        };

        // Check if this line has a callout
        result.push_str(line);
        if let Some(&callout_num) = callouts.get(&code_line_num) {
            // Append callout HTML matching asciidoctor's format
            use std::fmt::Write;
            let _ = write!(
                result,
                " <i class=\"conum\" data-value=\"{callout_num}\"></i><b>({callout_num})</b>"
            );
        }
    }

    result
}

/// HTML-escape special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Write HTML-escaped code with callout markers.
///
/// This is the fallback when syntax highlighting is disabled or unavailable.
/// Outputs plain HTML-escaped text without any highlighting, but preserves callouts.
fn write_escaped_code_with_callouts<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
) -> Result<(), Error> {
    let (code, callouts) = extract_text_and_callouts(inlines);

    if callouts.is_empty() {
        let escaped = html_escape(&code);
        write!(writer, "{escaped}")?;
    } else {
        // Process line-by-line to insert callouts
        for (i, line) in code.split('\n').enumerate() {
            if i > 0 {
                writeln!(writer)?;
            }
            let escaped = html_escape(line);
            write!(writer, "{escaped}")?;
            if let Some(&callout_num) = callouts.get(&i) {
                write!(
                    writer,
                    "<i class=\"conum\" data-value=\"{callout_num}\"></i><b>({callout_num})</b>"
                )?;
            }
        }
    }
    Ok(())
}

/// Highlight code when the highlighting feature is not enabled.
///
/// Outputs HTML-escaped code with proper callout HTML rendering.
#[cfg(not(feature = "highlighting"))]
pub(crate) fn highlight_code<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    _language: &str,
    _dark_mode: bool,
) -> Result<(), Error> {
    write_escaped_code_with_callouts(writer, inlines)
}

/// Extract text content and callout positions from inline nodes for highlighting.
///
/// Returns the code text (without callout markers) and a map of line numbers
/// to callout numbers. Callouts are NOT included in the text - they'll be
/// rendered separately after syntax highlighting.
fn extract_text_and_callouts(inlines: &[InlineNode]) -> (String, HashMap<usize, usize>) {
    let mut result = String::new();
    let mut callouts: HashMap<usize, usize> = HashMap::new();
    let mut current_line = 0;

    for node in inlines {
        #[allow(clippy::match_same_arms)]
        match node {
            InlineNode::VerbatimText(verbatim) => {
                // Count newlines in this text to track current line
                for ch in verbatim.content.chars() {
                    result.push(ch);
                    if ch == '\n' {
                        current_line += 1;
                    }
                }
            }
            InlineNode::RawText(raw) => {
                for ch in raw.content.chars() {
                    result.push(ch);
                    if ch == '\n' {
                        current_line += 1;
                    }
                }
            }
            InlineNode::PlainText(plain) => {
                for ch in plain.content.chars() {
                    result.push(ch);
                    if ch == '\n' {
                        current_line += 1;
                    }
                }
            }
            InlineNode::LineBreak(_) => {
                result.push('\n');
                current_line += 1;
            }
            InlineNode::CalloutRef(callout) => {
                // Don't add callout text to the code - track it for later insertion
                callouts.insert(current_line, callout.number);
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
    (result, callouts)
}

#[cfg(all(test, feature = "highlighting"))]
mod tests {
    use super::*;
    use acdc_parser::{CalloutRef, CalloutRefKind, Location, Verbatim};

    fn create_verbatim_inlines(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::VerbatimText(Verbatim {
            content: content.to_string(),
            location: Location::default(),
        })]
    }

    fn create_callout_ref(number: usize) -> InlineNode {
        InlineNode::CalloutRef(CalloutRef {
            kind: CalloutRefKind::Explicit,
            number,
            location: Location::default(),
        })
    }

    #[test]
    fn test_extract_text_and_callouts_from_verbatim() {
        let inlines = create_verbatim_inlines("fn main() {\n    println!(\"Hello\");\n}");
        let (text, callouts) = extract_text_and_callouts(&inlines);
        assert_eq!(text, "fn main() {\n    println!(\"Hello\");\n}");
        assert!(callouts.is_empty());
    }

    #[test]
    fn test_extract_text_and_callouts_with_callouts() {
        // Simulate: "let x = 1; <1>\nlet y = 2; <2>\n"
        let inlines = vec![
            InlineNode::VerbatimText(Verbatim {
                content: "let x = 1; ".to_string(),
                location: Location::default(),
            }),
            create_callout_ref(1),
            InlineNode::VerbatimText(Verbatim {
                content: "\nlet y = 2; ".to_string(),
                location: Location::default(),
            }),
            create_callout_ref(2),
            InlineNode::VerbatimText(Verbatim {
                content: "\n".to_string(),
                location: Location::default(),
            }),
        ];

        let (text, callouts) = extract_text_and_callouts(&inlines);
        // Callouts should NOT appear in the extracted text
        assert_eq!(text, "let x = 1; \nlet y = 2; \n");
        // Line 0 should have callout 1, line 1 should have callout 2
        assert_eq!(callouts.get(&0), Some(&1));
        assert_eq!(callouts.get(&1), Some(&2));
        assert_eq!(callouts.len(), 2);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_highlight_code_with_callouts() -> Result<(), Error> {
        // Simulate code with callouts
        let inlines = vec![
            InlineNode::VerbatimText(Verbatim {
                content: "let x = 1; ".to_string(),
                location: Location::default(),
            }),
            create_callout_ref(1),
            InlineNode::VerbatimText(Verbatim {
                content: "\nlet y = 2; ".to_string(),
                location: Location::default(),
            }),
            create_callout_ref(2),
        ];

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "rust", false)?;

        let html = String::from_utf8(buffer).expect("valid utf8");
        // Verify callout HTML is present
        assert!(
            html.contains("<i class=\"conum\" data-value=\"1\"></i><b>(1)</b>"),
            "Should contain callout 1 HTML, got: {html}"
        );
        assert!(
            html.contains("<i class=\"conum\" data-value=\"2\"></i><b>(2)</b>"),
            "Should contain callout 2 HTML, got: {html}"
        );
        // Verify the callouts appear at the end of their respective lines
        // (not embedded in the code text)
        assert!(
            !html.contains("&lt;1&gt;"),
            "Callouts should not appear as escaped text: {html}"
        );

        Ok(())
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_highlight_rust_code() -> Result<(), Error> {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let inlines = create_verbatim_inlines(code);

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "rust", false)?;

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
        highlight_code(&mut buffer, &inlines, "unknown_lang_xyz", false)?;

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
