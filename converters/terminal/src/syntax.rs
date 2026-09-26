use std::io::Write;

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
pub(crate) fn highlight_text<W: Write + ?Sized>(
    writer: &mut W,
    code: &str,
    language: &str,
    processor: &Processor<'_>,
) -> Result<(), Error> {
    let mut code = code.to_owned();
    // Giallo's terminal renderer uses a trailing empty token line to preserve the
    // separator before the final non-empty line, then omits that sentinel line.
    if !code.ends_with('\n') {
        code.push('\n');
    }
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
pub(crate) fn highlight_text<W: Write + ?Sized>(
    writer: &mut W,
    code: &str,
    _language: &str,
    _processor: &Processor<'_>,
) -> Result<(), Error> {
    write!(writer, "{code}")?;
    Ok(())
}

#[cfg(all(test, feature = "highlighting"))]
mod tests {
    use super::*;
    use crate::create_test_processor;

    #[test]
    fn test_highlight_rust_code() -> Result<(), Error> {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let processor = create_test_processor();

        let mut buffer = Vec::new();
        highlight_text(&mut buffer, code, "rust", &processor)?;

        // Just verify it doesn't crash and produces output
        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }

    #[test]
    fn test_highlight_unknown_language_fallback() -> Result<(), Error> {
        let code = "some code here";
        let processor = create_test_processor();

        let mut buffer = Vec::new();
        highlight_text(&mut buffer, code, "unknown_lang_xyz", &processor)?;

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
        let processor = create_test_processor();

        let mut buffer = Vec::new();
        highlight_text(&mut buffer, code, "python", &processor)?;

        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }

    #[test]
    fn test_highlight_javascript_code() -> Result<(), Error> {
        let code = "function hello() {\n  console.log('Hello, world!');\n}";
        let processor = create_test_processor();

        let mut buffer = Vec::new();
        highlight_text(&mut buffer, code, "javascript", &processor)?;

        assert!(!buffer.is_empty(), "Should produce highlighted output");

        Ok(())
    }
}
