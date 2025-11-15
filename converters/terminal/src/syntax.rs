use std::io::Write;

use acdc_parser::InlineNode;
use crossterm::{
    QueueableCommand,
    style::{Color, PrintStyledContent, Stylize},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

use crate::{Error, Processor};

/// Highlight code using syntect and render to terminal with crossterm colors.
///
/// This function takes inline nodes (which may contain verbatim or plain text),
/// extracts the text content, and applies syntax highlighting based on the
/// specified language. The theme is chosen based on the terminal background.
pub fn highlight_code<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    language: &str,
    processor: &Processor,
) -> Result<(), Error> {
    let code = extract_text_from_inlines(inlines);
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme_name = processor.appearance.theme.syntect_theme();
    let theme = &theme_set
        .themes
        .get(theme_name)
        .ok_or(Error::InvalidTheme(theme_name.to_string()))?;
    let syntax = syntax_set
        .find_syntax_by_token(language)
        .or_else(|| syntax_set.find_syntax_by_extension(language))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, theme);
    for line in LinesWithEndings::from(&code) {
        let ranges = highlighter
            .highlight_line(line, &syntax_set)
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        for (style, text) in ranges {
            let styled_text = apply_syntect_style(text, style);
            QueueableCommand::queue(writer, PrintStyledContent(styled_text))?;
        }
    }

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
                let processed = process_callouts(&verbatim.content);
                result.push_str(&processed);
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

/// Process callout markers in verbatim text, replacing <.> with auto-numbered callouts
fn process_callouts(text: &str) -> String {
    use std::fmt::Write;

    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut auto_number = 1;

    while let Some(c) = chars.next() {
        if c == '<' {
            // Check for <.> pattern first
            if chars.peek() == Some(&'.') {
                chars.next(); // consume the '.'
                if chars.peek() == Some(&'>') {
                    chars.next(); // consume the '>'
                    let _ = write!(result, "<{auto_number}>");
                    auto_number += 1;
                    continue;
                }
                // Not a valid <.> pattern, output what we consumed
                result.push('<');
                result.push('.');
                continue;
            }
        }
        result.push(c);
    }

    result
}

/// Convert syntect's Style to crossterm styled content.
///
/// Maps RGB colors from syntect to crossterm's `Color::Rgb` for true color support.
fn apply_syntect_style(text: &str, style: Style) -> crossterm::style::StyledContent<&str> {
    let fg = style.foreground;

    // Apply bold if the style indicates it
    // Syntect doesn't provide bold/italic info in the Style struct directly,
    // but some themes use specific colors to indicate emphasis
    text.with(Color::Rgb {
        r: fg.r,
        g: fg.g,
        b: fg.b,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_converters_common::Options;
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
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        let appearance = Appearance::detect();
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
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
