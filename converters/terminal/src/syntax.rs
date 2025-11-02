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

use crate::Error;

/// Highlight code using syntect and render to terminal with crossterm colors.
///
/// This function takes inline nodes (which may contain verbatim or plain text),
/// extracts the text content, and applies syntax highlighting based on the
/// specified language.
pub fn highlight_code<W: Write + ?Sized>(
    writer: &mut W,
    inlines: &[InlineNode],
    language: &str,
) -> Result<(), Error> {
    let code = extract_text_from_inlines(inlines);
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme = &theme_set.themes["Solarized (light)"];
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
            // For other node types, recurse or ignore
            // In practice, code blocks should only contain verbatim/plain text
            _ => {}
        }
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
    fn test_highlight_rust_code() -> Result<(), Error> {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let inlines = create_verbatim_inlines(code);

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "rust")?;

        // Just verify it doesn't crash and produces output
        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }

    #[test]
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

        Ok(())
    }

    #[test]
    fn test_highlight_python_code() -> Result<(), Error> {
        let code = "def hello():\n    print('Hello, world!')";
        let inlines = create_verbatim_inlines(code);

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "python")?;

        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }

    #[test]
    fn test_highlight_javascript_code() -> Result<(), Error> {
        let code = "function hello() {\n  console.log('Hello, world!');\n}";
        let inlines = create_verbatim_inlines(code);

        let mut buffer = Vec::new();
        highlight_code(&mut buffer, &inlines, "javascript")?;

        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }
}
