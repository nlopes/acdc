//! Selectable HTML rendering for terminal cell-grid previews.

use std::io::Write;

use acdc_converters_core::Options;
use acdc_converters_core::code::detect_language;
use acdc_converters_terminal::cell_grid::{
    Cell, CellDecorations, CellGrid, Rgb, TerminalSize, capture_ansi,
};
use acdc_parser::{AttributeValue, BlockMetadata, DocumentAttributes, InlineNode};

use crate::Error;

const DEFAULT_COLS: usize = 80;
const AUTO_ROW_PADDING: usize = 1;
const MAX_AUTO_ROWS: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Theme {
    Dark,
    Light,
}

impl Theme {
    fn from_document_attributes(attrs: &DocumentAttributes<'_>) -> Self {
        if attrs
            .get("dark-mode")
            .is_some_and(|v| !matches!(v, AttributeValue::Bool(false) | AttributeValue::None))
        {
            Self::Dark
        } else {
            Self::Light
        }
    }

    const fn colors(self) -> (Rgb, Rgb) {
        match self {
            Self::Dark => (
                Rgb {
                    r: 13,
                    g: 17,
                    b: 23,
                },
                Rgb {
                    r: 230,
                    g: 237,
                    b: 243,
                },
            ),
            Self::Light => (
                Rgb {
                    r: 246,
                    g: 248,
                    b: 250,
                },
                Rgb {
                    r: 31,
                    g: 35,
                    b: 40,
                },
            ),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreviewOptions {
    cols: usize,
    rows: Option<usize>,
    theme: Theme,
}

impl PreviewOptions {
    fn resolve(attrs: &DocumentAttributes<'_>, metadata: Option<&BlockMetadata<'_>>) -> Self {
        let document_cols = attr_usize(attrs.get("terminal-cols"));
        let document_rows = attr_usize(attrs.get("terminal-rows"));
        let document_theme = Theme::from_document_attributes(attrs);

        Self {
            cols: metadata
                .and_then(|metadata| {
                    attr_usize(metadata.attributes.get("cols"))
                        .or_else(|| attr_usize(metadata.attributes.get("terminal-cols")))
                })
                .or(document_cols)
                .unwrap_or(DEFAULT_COLS),
            rows: metadata
                .and_then(|metadata| {
                    attr_usize(metadata.attributes.get("rows"))
                        .or_else(|| attr_usize(metadata.attributes.get("terminal-rows")))
                })
                .or(document_rows),
            theme: document_theme,
        }
    }

    fn size_for_output(self, ansi: &[u8]) -> TerminalSize {
        TerminalSize::new(
            self.cols,
            self.rows
                .unwrap_or_else(|| estimate_rows(ansi, self.cols).min(MAX_AUTO_ROWS)),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SpanStyle {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    decorations: CellDecorations,
}

impl From<&Cell> for SpanStyle {
    fn from(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            decorations: cell.decorations,
        }
    }
}

pub(crate) fn is_enabled(attrs: &DocumentAttributes<'_>) -> bool {
    attrs
        .get("terminal-preview")
        .is_some_and(|value| !matches!(value, AttributeValue::Bool(false) | AttributeValue::None))
}

pub(crate) fn is_terminal_listing(
    attrs: &DocumentAttributes<'_>,
    metadata: &BlockMetadata<'_>,
) -> bool {
    is_enabled(attrs) && detect_language(metadata).is_some_and(is_terminal_language)
}

pub(crate) fn is_terminal_session(metadata: &BlockMetadata<'_>) -> bool {
    metadata.style == Some("terminal")
}

pub(crate) fn render_listing<W: Write>(
    writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
) -> Result<(), Error> {
    let preview_options = PreviewOptions::resolve(attrs, None);
    render_with_options(writer, inlines, metadata, options, attrs, preview_options)
}

pub(crate) fn render_session<W: Write>(
    writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
) -> Result<(), Error> {
    let preview_options = PreviewOptions::resolve(attrs, Some(metadata));
    render_with_options(writer, inlines, metadata, options, attrs, preview_options)
}

fn render_with_options<W: Write>(
    mut writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
    preview_options: PreviewOptions,
) -> Result<(), Error> {
    let ansi = normalize_terminal_newlines(&acdc_converters_terminal::render_listing_to_ansi(
        options,
        attrs.clone(),
        inlines,
        metadata,
        preview_options.cols,
        preview_options.theme == Theme::Dark,
    )?);
    let size = preview_options.size_for_output(&ansi);
    let grid = capture_ansi(&ansi, size)?;

    render_grid(&mut writer, &grid, preview_options.theme)?;
    Ok(())
}

fn is_terminal_language(language: &str) -> bool {
    matches!(
        language,
        "console"
            | "terminal"
            | "shell"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "powershell"
            | "ps1"
            | "cmd"
    )
}

fn normalize_terminal_newlines(ansi: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(ansi.len());
    let mut previous = None;
    for byte in ansi {
        if *byte == b'\n' && previous != Some(b'\r') {
            normalized.push(b'\r');
        }
        normalized.push(*byte);
        previous = Some(*byte);
    }
    normalized
}

fn render_grid<W: Write>(writer: &mut W, grid: &CellGrid, theme: Theme) -> Result<(), Error> {
    let (bg, fg) = theme.colors();
    write!(
        writer,
        "<div class=\"terminal-preview terminal-preview--{}\" style=\"background-color:{};color:{}\" data-cols=\"{}\" data-rows=\"{}\">",
        theme.as_str(),
        rgb_css(bg),
        rgb_css(fg),
        grid.cols(),
        grid.rows()
    )?;
    writeln!(
        writer,
        "<pre class=\"terminal-preview__screen\" aria-label=\"Terminal preview\">"
    )?;
    for (row_index, row) in grid.rows_iter().enumerate() {
        render_row(writer, row)?;
        if row_index + 1 < grid.rows() {
            writeln!(writer)?;
        }
    }
    writeln!(writer, "</pre>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

fn render_row<W: Write>(writer: &mut W, row: &[Cell]) -> Result<(), Error> {
    let mut current_style = SpanStyle::default();
    let mut buffer = String::new();
    let mut has_open_span = false;

    for cell in row {
        let style = SpanStyle::from(cell);
        if style != current_style {
            flush_span(writer, &buffer, current_style, has_open_span)?;
            buffer.clear();
            current_style = style;
            has_open_span = !is_default_style(style);
        }
        let text = if cell.text.is_empty() {
            " "
        } else {
            cell.text.as_str()
        };
        buffer.push_str(text);
    }

    flush_span(writer, buffer.trim_end(), current_style, has_open_span)?;
    Ok(())
}

fn flush_span<W: Write>(
    writer: &mut W,
    text: &str,
    style: SpanStyle,
    has_open_span: bool,
) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }

    if has_open_span {
        write!(writer, "<span style=\"{}\">", style_attr(style))?;
    }
    write!(writer, "{}", escape_html(text))?;
    if has_open_span {
        write!(writer, "</span>")?;
    }
    Ok(())
}

fn attr_usize(value: Option<&AttributeValue<'_>>) -> Option<usize> {
    value
        .map(ToString::to_string)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn estimate_rows(ansi: &[u8], cols: usize) -> usize {
    let cols = cols.max(1);
    let mut rows = 1;
    let mut col = 0;
    let mut bytes = ansi.iter().copied().peekable();

    while let Some(byte) = bytes.next() {
        match byte {
            b'\x1b' => skip_escape_sequence(&mut bytes),
            b'\r' => col = 0,
            b'\n' => {
                rows += 1;
                col = 0;
            }
            b'\t' => {
                let width = 4 - (col % 4);
                advance_columns(&mut rows, &mut col, cols, width);
            }
            0x00..=0x1f | 0x7f => {}
            _ => advance_columns(&mut rows, &mut col, cols, 1),
        }
    }

    rows.saturating_add(AUTO_ROW_PADDING).max(1)
}

fn advance_columns(rows: &mut usize, col: &mut usize, cols: usize, width: usize) {
    let mut remaining = width;
    while remaining > 0 {
        if *col == cols {
            *rows += 1;
            *col = 0;
        }

        let available = cols - *col;
        let step = remaining.min(available);
        *col += step;
        remaining -= step;
    }
}

fn skip_escape_sequence<I>(bytes: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = u8>,
{
    match bytes.peek().copied() {
        Some(b'[') => {
            bytes.next();
            for byte in bytes.by_ref() {
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        }
        Some(b']') => {
            bytes.next();
            let mut previous = None;
            for byte in bytes.by_ref() {
                if byte == b'\x07' || (previous == Some(b'\x1b') && byte == b'\\') {
                    break;
                }
                previous = Some(byte);
            }
        }
        Some(_) | None => {}
    }
}

fn is_default_style(style: SpanStyle) -> bool {
    style.fg.is_none()
        && style.bg.is_none()
        && !style.decorations.bold
        && !style.decorations.italic
        && !style.decorations.underline
        && !style.decorations.dim
        && !style.decorations.inverse
        && !style.decorations.strikethrough
}

fn style_attr(style: SpanStyle) -> String {
    let mut css = String::new();
    if let Some(fg) = style.fg {
        push_decl(&mut css, "color", &rgb_css(fg));
    }
    if let Some(bg) = style.bg {
        push_decl(&mut css, "background-color", &rgb_css(bg));
    }
    if style.decorations.bold {
        push_decl(&mut css, "font-weight", "700");
    }
    if style.decorations.italic {
        push_decl(&mut css, "font-style", "italic");
    }
    if style.decorations.underline && style.decorations.strikethrough {
        push_decl(&mut css, "text-decoration", "underline line-through");
    } else if style.decorations.underline {
        push_decl(&mut css, "text-decoration", "underline");
    } else if style.decorations.strikethrough {
        push_decl(&mut css, "text-decoration", "line-through");
    }
    if style.decorations.dim {
        push_decl(&mut css, "opacity", "0.72");
    }
    if style.decorations.inverse {
        push_decl(&mut css, "filter", "invert(1)");
    }
    css
}

fn push_decl(css: &mut String, property: &str, value: &str) {
    if !css.is_empty() {
        css.push(';');
    }
    css.push_str(property);
    css.push(':');
    css.push_str(value);
}

fn rgb_css(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use acdc_converters_core::{
        Diagnostics, GeneratorMetadata, Options as ConverterOptions, WarningSource,
    };
    use acdc_parser::Options as ParserOptions;

    use crate::{Processor, RenderOptions};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn render(
        input: &str,
        variant: crate::HtmlVariant,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let parser_options =
            ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
        let parsed = acdc_parser::parse(input, &parser_options)?;
        let doc = parsed.document();
        let options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new_with_variant(options, doc.attributes.clone(), variant);
        let mut output = Vec::new();
        let source = WarningSource::new("html").with_variant(variant.as_str());
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.convert_to_writer(
            doc,
            &mut output,
            &RenderOptions::default(),
            &mut diagnostics,
        )?;
        Ok(String::from_utf8(output)?)
    }

    #[test]
    fn standard_html_can_include_selectable_terminal_preview() -> TestResult {
        let html = render(
            "= Example\n:terminal-preview:\n\n[source,console]\n----\n$ acdc --version\nacdc 0.2.0\n----\n\nAfter preview.\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div id=\"content\">"));
        assert!(html.contains("<div class=\"listingblock terminal-preview-block\">"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("background-color:#f6f8fa;color:#1f2328"));
        assert!(html.contains("$</span>"));
        assert!(html.contains(" acdc"));
        assert!(html.contains("--version"));
        assert!(html.contains("0.2.0"));
        let preview_offset = html
            .find("terminal-preview")
            .ok_or("missing terminal preview")?;
        let after_offset = html
            .find("After preview.")
            .ok_or("missing following paragraph")?;
        assert!(preview_offset < after_offset);
        Ok(())
    }

    #[test]
    fn semantic_html_can_include_selectable_terminal_preview() -> TestResult {
        let html = render(
            "= Example\n:terminal-preview:\n\n[source,terminal]\n----\n$ echo semantic\nsemantic\n----\n",
            crate::HtmlVariant::Semantic,
        )?;

        assert!(html.contains("<main id=\"content\">"));
        assert!(html.contains("listing-block terminal-preview-block"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("$</span>"));
        assert!(html.contains(" echo semantic"));
        Ok(())
    }

    #[test]
    fn dark_mode_uses_dark_terminal_preview_theme() -> TestResult {
        let html = render(
            "= Example\n:terminal-preview:\n:dark-mode:\n\n[source,console]\n----\n$ echo dark\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminal-preview terminal-preview--dark\""));
        assert!(html.contains("background-color:#0d1117;color:#e6edf3"));
        Ok(())
    }

    #[test]
    fn terminal_preview_preserves_syntax_colors() -> TestResult {
        let html = render(
            "= Example\n:terminal-preview:\n\n[source,bash]\n----\necho \"hello\"\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(
            html.contains("<span style=\"color:"),
            "expected terminal preview to include colored spans, got: {html}"
        );
        Ok(())
    }

    #[test]
    fn terminal_session_block_does_not_require_preview_attribute() -> TestResult {
        let html = render(
            "= Example\n\n[terminal]\n----\n$ cargo build\n\x1b[31merror\x1b[0m\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminalblock terminal-preview-block\">"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("$ cargo build"));
        assert!(html.contains(">error</span>"));
        assert!(html.contains("<span style=\"color:"));
        Ok(())
    }

    #[test]
    fn terminal_session_block_uses_block_dimensions() -> TestResult {
        let html = render(
            "= Example\n\n[terminal,cols=12,rows=4]\n----\n$ echo dimensions\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("data-cols=\"12\""));
        assert!(html.contains("data-rows=\"4\""));
        assert!(html.contains("background-color:#f6f8fa;color:#1f2328"));
        Ok(())
    }

    #[test]
    fn terminal_session_options_layer_block_dimensions_over_document_dimensions() -> TestResult {
        let html = render(
            "= Example\n:terminal-cols: 30\n:terminal-rows: 7\n:dark-mode:\n\n[terminal,cols=12]\n----\n$ echo layered\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminal-preview terminal-preview--dark\""));
        assert!(html.contains("data-cols=\"12\""));
        assert!(html.contains("data-rows=\"7\""));
        assert!(html.contains("background-color:#0d1117;color:#e6edf3"));
        Ok(())
    }

    #[test]
    fn semantic_terminal_session_block_uses_semantic_wrapper() -> TestResult {
        let html = render(
            "= Example\n\n.Terminal\n[terminal]\n----\n$ echo semantic\n----\n",
            crate::HtmlVariant::Semantic,
        )?;

        assert!(html.contains("<figure class=\"terminal-block terminal-preview-block\""));
        assert!(html.contains("<figcaption>Terminal</figcaption>"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("$ echo semantic"));
        Ok(())
    }

    #[test]
    fn literal_terminal_session_block_renders_as_terminal_preview() -> TestResult {
        let html = render(
            "= Example\n\n[terminal,cols=20]\n....\n$ echo literal\n....\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminalblock terminal-preview-block\">"));
        assert!(html.contains("data-cols=\"20\""));
        assert!(html.contains("$ echo literal"));
        Ok(())
    }

    #[test]
    fn auto_rows_follow_content_height_with_padding() {
        assert_eq!(
            super::estimate_rows(b"$ acdc --version\r\nacdc 0.2.0", 80),
            3
        );
        assert_eq!(super::estimate_rows(b"123456", 3), 3);
        assert_eq!(super::estimate_rows(b"1234567", 3), 4);
    }

    #[test]
    fn skips_terminal_preview_without_attribute() -> TestResult {
        let html = render("= Example\n\nPlain HTML\n", crate::HtmlVariant::Standard)?;

        assert!(!html.contains("terminal-preview--"));
        assert!(html.contains("Plain HTML"));
        Ok(())
    }

    #[test]
    fn linkcss_uses_built_in_stylesheet_for_terminal_preview_styles() -> TestResult {
        let html = render(
            "= Example\n:linkcss:\n:terminal-preview:\n\n[source,console]\n----\n$ echo linked\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains(r#"<link rel="stylesheet" href="./asciidoctor-light-mode.css">"#));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(!html.contains(".terminal-preview{"));
        assert!(!html.contains(".terminal-preview__screen{"));
        Ok(())
    }

    #[test]
    fn escapes_terminal_text() -> TestResult {
        let html = render(
            "= Example\n:terminal-preview:\n\n[source,console]\n----\n<&>\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;&gt;"));
        Ok(())
    }
}
