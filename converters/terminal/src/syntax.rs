use std::io::Write;

use crate::{Converter, Error};

/// Highlight code and render to terminal.
///
/// When the `highlighting` feature is enabled, this uses giallo for syntax
/// highlighting with ANSI escape codes. Otherwise, it outputs plain text.
#[cfg(feature = "highlighting")]
pub(crate) fn highlight_text<W: Write + ?Sized>(
    writer: &mut W,
    code: &str,
    language: &str,
    converter: &Converter<'_>,
    diagnostics: Option<&mut acdc_converters_core::Diagnostics<'_>>,
) -> Result<(), Error> {
    use acdc_converters_core::code::highlighting;

    let mut diagnostics = diagnostics;
    if highlighting::skip_highlighting(code, diagnostics.as_deref_mut()) {
        writer.write_all(code.as_bytes())?;
        return Ok(());
    }
    let theme_name = converter.appearance.theme.highlight_theme();
    let theme_variant = giallo::ThemeVariant::Single(theme_name);
    let options = giallo::HighlightOptions::new(language, theme_variant).fallback_to_plain(true);

    let highlighted = match highlighting::registry()
        .map_err(|error| Error::Io(std::io::Error::other(error.to_string())))
        .and_then(|registry| {
            registry
                .highlight(code, &options)
                .map_err(|error| Error::Io(std::io::Error::other(error.to_string())))
        }) {
        Ok(h) => h,
        Err(e) => {
            let Some(diagnostics) = diagnostics else {
                return Err(e);
            };
            let message = format!("syntax highlighting failed: {e}; rendering plain source");
            if !diagnostics.warnings().iter().any(|warning| {
                warning.source == *diagnostics.source() && warning.message == message
            }) {
                diagnostics.warn_with_advice(message,
                    "Use a supported source language, or disable syntax highlighting for plain output.");
            }
            write!(writer, "{code}")?;
            return Ok(());
        }
    };

    let renderer = giallo::TerminalRenderer::default();
    let render_options = giallo::RenderOptions::default();
    // Render individual token lines so giallo's whole-block newline convention
    // cannot add or remove separators from the source.
    for (index, tokens) in highlighted.tokens.into_iter().enumerate() {
        if index > 0 {
            writer.write_all(b"\n")?;
        }
        let line = giallo::HighlightedCode {
            language: highlighted.language,
            theme: highlighted.theme,
            tokens: vec![tokens],
        };
        writer.write_all(renderer.render(&line, &render_options).as_bytes())?;
    }

    Ok(())
}

/// Fallback implementation when syntax highlighting is disabled.
/// Outputs plain text without any highlighting.
#[cfg(not(feature = "highlighting"))]
pub(crate) fn highlight_text<W: Write + ?Sized>(
    writer: &mut W,
    code: &str,
    _language: &str,
    _converter: &Converter<'_>,
    _diagnostics: Option<&mut acdc_converters_core::Diagnostics<'_>>,
) -> Result<(), Error> {
    write!(writer, "{code}")?;
    Ok(())
}

#[cfg(all(test, feature = "highlighting"))]
mod tests {
    use super::*;
    use crate::create_test_converter;

    #[test]
    fn long_source_lines_preserve_plain_source_and_warn_once() -> Result<(), Error> {
        use acdc_converters_core::{
            Diagnostics, WarningSource, code::highlighting::MAX_LINE_BYTES,
        };

        let code = format!(
            "/* {}\ncontinued comment <&> 日本語\n*/\n",
            "x".repeat(MAX_LINE_BYTES)
        );
        let converter = create_test_converter();
        let source = WarningSource::new("terminal");
        let mut warnings = Vec::new();
        for _ in 0..2 {
            let mut output = Vec::new();
            highlight_text(
                &mut output,
                &code,
                "javascript",
                &converter,
                Some(&mut Diagnostics::new(&source, &mut warnings)),
            )?;
            assert_eq!(output, code.as_bytes());
        }
        assert_eq!(warnings.len(), 1);
        assert!(warnings.iter().all(|warning| warning.advice().is_some()));
        Ok(())
    }

    #[test]
    fn highlighting_preserves_source_lines() -> Result<(), Error> {
        let converter = create_test_converter();
        for source in [
            "",
            "\n",
            "\n\n",
            "α",
            "α\n",
            "α\n\n",
            "\nα\nβ\n\n",
            " \n  \n",
        ] {
            let mut output = Vec::new();
            highlight_text(&mut output, source, "plain", &converter, None)?;
            // The plain grammar has no ANSI styles, so compare every byte.
            assert_eq!(output, source.as_bytes());
        }
        Ok(())
    }

    #[test]
    fn test_highlight_rust_code() -> Result<(), Error> {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let converter = create_test_converter();

        let mut buffer = Vec::new();
        highlight_text(&mut buffer, code, "rust", &converter, None)?;

        // Just verify it doesn't crash and produces output
        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }

    #[test]
    fn test_highlight_unknown_language_fallback() -> Result<(), Error> {
        let code = "some code here";
        let converter = create_test_converter();

        let mut buffer = Vec::new();
        highlight_text(&mut buffer, code, "unknown_lang_xyz", &converter, None)?;

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
        let converter = create_test_converter();

        let mut buffer = Vec::new();
        highlight_text(&mut buffer, code, "python", &converter, None)?;

        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }

    #[test]
    fn test_highlight_javascript_code() -> Result<(), Error> {
        let code = "function hello() {\n  console.log('Hello, world!');\n}";
        let converter = create_test_converter();

        let mut buffer = Vec::new();
        highlight_text(&mut buffer, code, "javascript", &converter, None)?;

        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }
}
