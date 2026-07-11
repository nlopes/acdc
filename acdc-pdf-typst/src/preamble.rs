//! Generates the document preamble: the `#set`/`#show`/`#let` rules that encode
//! the theme's design tokens.

use std::fmt::Write as _;

use acdc_pdf_theme::{EMOJI_FONT_FAMILY, FontStack, Palette, Theme};

use crate::{EmitOptions, PageSize, escape::escape_markup, writer::Writer};

/// Write the whole preamble (page setup, text/heading rules, and the `#let`
/// helpers) into `writer`.
pub fn write(writer: &mut Writer, theme: &Theme, options: &EmitOptions) {
    let mut out = String::new();
    write_page(&mut out, theme, options);
    write_text(&mut out, theme, options);
    write_code(&mut out, theme, options);
    write_helpers(&mut out, theme);
    out.push('\n');
    writer.raw(&out);
}

/// `#set page(...)`: geometry, plus (when branded) background, header, footer,
/// and any watermark.
fn write_page(out: &mut String, theme: &Theme, options: &EmitOptions) {
    let palette = &theme.palette;
    let spacing = &theme.spacing;

    let _ = write!(
        out,
        "#set page(paper: \"{}\", margin: (x: {}cm, y: {}cm)",
        options.page.paper(),
        spacing.margin_x_cm,
        spacing.margin_y_cm,
    );
    if !options.plain {
        let _ = write!(out, ", fill: {}", color(&palette.page_bg));
        if let Some(header) = header_content(options, palette) {
            let _ = write!(out, ", header: {header}");
        }
    }
    // The watermark annotations show even under `--plain`.
    if let Some(footer) = footer_content(
        palette,
        options.watermark.as_deref(),
        options.watermark_timestamp.as_deref(),
        !options.plain,
    ) {
        let _ = write!(out, ", footer: {footer}");
    }
    if let Some(watermark) = &options.watermark {
        let _ = write!(
            out,
            ", background: {}",
            watermark_background(watermark, palette)
        );
    }
    out.push_str(")\n");
}

/// Base text, paragraph, heading, and inline-span styling.
fn write_text(out: &mut String, theme: &Theme, options: &EmitOptions) {
    let palette = &theme.palette;
    let typography = &theme.typography;

    let _ = writeln!(
        out,
        "#set text(font: {}, size: {}pt, weight: {}, fill: {}, tracking: {}em, lang: \"en\")",
        font_tuple(&typography.body_font, options.brand_fonts),
        typography.body_size_pt,
        typography.body_weight,
        color(&palette.body_text),
        typography.tracking_em,
    );
    let _ = writeln!(
        out,
        "#set par(leading: {}em, justify: false)",
        typography.body_leading_em,
    );
    // pulldown already emitted curly quotes/dashes; don't let Typst re-process.
    out.push_str("#set smartquote(enabled: false)\n");

    let _ = writeln!(
        out,
        "#show heading: set text(font: {}, weight: {}, fill: {})",
        font_tuple(&typography.heading_font, options.brand_fonts),
        typography.heading_weight,
        color(&palette.heading),
    );
    for (index, size) in typography.heading_pt.iter().enumerate() {
        let level = index + 1;
        let _ = writeln!(
            out,
            "#show heading.where(level: {level}): set text(size: {size}pt)",
        );
    }

    let _ = writeln!(out, "#show link: set text(fill: {})", color(&palette.link));
    let _ = writeln!(
        out,
        "#show strong: set text(fill: {}, weight: {})",
        color(&palette.heading),
        typography.strong_weight,
    );
}

/// Code styling: monospace on a dark card, syntax-highlighted.
fn write_code(out: &mut String, theme: &Theme, options: &EmitOptions) {
    let palette = &theme.palette;
    let typography = &theme.typography;
    let spacing = &theme.spacing;

    let _ = writeln!(
        out,
        "#show raw: set text(font: {})",
        font_tuple(&typography.mono_font, options.brand_fonts),
    );
    let _ = writeln!(
        out,
        "#set raw(theme: \"{}\")",
        acdc_pdf_theme::HIGHLIGHT_THEME_PATH
    );
    let _ = writeln!(
        out,
        "#show raw.where(block: false): set text(fill: {})",
        color(&palette.heading),
    );
    let _ = writeln!(
        out,
        "#show raw.where(block: true): it => block(width: 100%, fill: {}, radius: {}pt, inset: {}pt, text(fill: {}, it))",
        color(&palette.code_bg),
        spacing.code_radius_pt,
        spacing.code_pad_pt,
        color(&palette.code_fg),
    );
}

/// The `#let` helpers and list/table styling.
fn write_helpers(out: &mut String, theme: &Theme) {
    let palette = &theme.palette;
    let typography = &theme.typography;
    let spacing = &theme.spacing;

    let _ = writeln!(
        out,
        "#let blockquote(body) = block(inset: (left: {}pt), stroke: (left: {}pt + {}), text(style: \"italic\", fill: {}, body))",
        spacing.quote_indent_pt,
        spacing.quote_rule_pt,
        color(&palette.quote_rule),
        color(&palette.quote_text),
    );
    // Callouts are drawn as an icon badge beside the body, laid out in two
    // columns so the content is indented past the icon. Each kind gets a glyph
    // in the accent colour; `success` draws a check, the rest use a letter.
    let _ = writeln!(
        out,
        "#let _cbadge(body) = box(circle(radius: 0.6em, fill: {title}, inset: 0pt, align(center + horizon, body)))",
        title = color(&palette.callout_title),
    );
    out.push_str(
        "#let _cico(glyph) = _cbadge(text(fill: white, weight: 700, size: 0.82em)[#glyph])\n",
    );
    out.push_str(
        "#let _ccheck = _cbadge(box(width: 0.62em, height: 0.62em, place(curve(stroke: (paint: white, thickness: 1.5pt, cap: \"round\", join: \"round\"), curve.move((0em, 0.34em)), curve.line((0.21em, 0.55em)), curve.line((0.58em, 0.08em))))))\n",
    );
    out.push_str(concat!(
        "#let _cicon(kind) = (\"note\": _cico(\"i\"), \"tip\": _cico(\"i\"), ",
        "\"important\": _cico(\"!\"), \"warning\": _cico(\"!\"), ",
        "\"caution\": _cico(\"!\"), \"success\": _ccheck).at(kind, default: _cico(\"i\"))\n",
    ));
    let _ = writeln!(
        out,
        "#let callout(kind, body) = pad(left: {indent}pt, block(width: 100%, fill: {bg}, radius: {radius}pt, inset: (x: {px}pt, y: {py}pt), grid(columns: (auto, 1fr), column-gutter: {gutter}pt, align: top, _cicon(kind), body)))",
        indent = spacing.callout_indent_pt,
        bg = color(&palette.callout_bg),
        radius = spacing.callout_radius_pt,
        px = spacing.callout_pad_x_pt,
        py = spacing.callout_pad_y_pt,
        gutter = spacing.callout_pad_x_pt * 0.8,
    );
    let _ = writeln!(
        out,
        "#let checkbox(checked) = box(height: 0.85em, width: 0.85em, baseline: 0.15em, radius: 2pt, stroke: 0.75pt + {counter}, fill: if checked {{ {accent} }} else {{ white }})",
        counter = color(&palette.counter),
        accent = color(&palette.accent),
    );
    let _ = writeln!(
        out,
        "#let hr() = block(above: 1.2em, below: 1.2em, line(length: 100%, stroke: {}pt + {}))",
        spacing.border_pt,
        color(&palette.border),
    );
    let _ = writeln!(
        out,
        "#let docimage(path) = block(radius: {}pt, clip: true, image(path, width: 100%))",
        spacing.image_radius_pt,
    );

    // List markers are drawn as shapes (disc / hollow circle / square) so they
    // never depend on a font having bullet glyphs, and take the brand colour.
    let bullet = color(&palette.bullet);
    // Negative baselines lift the markers up to sit against the top of the
    // first line (a positive shift would drop them toward the baseline).
    let _ = writeln!(
        out,
        "#set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: {bullet})), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + {bullet})), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: {bullet}))))",
    );
    let _ = writeln!(
        out,
        "#set enum(numbering: (..n) => text(fill: {})[#numbering(\"1.\", ..n.pos())])",
        color(&palette.counter),
    );

    // Tables: horizontal rules only, in the border colour. The converter wraps
    // cells from declared header rows with `tableheader`.
    let _ = writeln!(
        out,
        "#set table(stroke: (_, y) => (bottom: {border}pt + {color}), inset: (x: 0.6em, y: 0.45em))",
        border = spacing.border_pt,
        color = color(&palette.border),
    );
    let _ = writeln!(
        out,
        "#let tableheader(body) = text(weight: {}, body)",
        typography.table_header_weight,
    );
}

/// Build the running-header content: the logo (if supplied) and/or the title,
/// left-aligned in the accent colour. Returns `None` when there is nothing to
/// show, so no header is set.
fn header_content(options: &EmitOptions, palette: &Palette) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(logo) = &options.logo {
        let mut path = String::new();
        // The virtual path is a controlled string, but escape it defensively.
        path.push('"');
        crate::escape::escape_string(&mut path, logo);
        path.push('"');
        parts.push(format!("#box(baseline: 30%, image({path}, height: 22pt))"));
    }
    if let Some(title) = &options.title {
        let mut escaped = String::new();
        escape_markup(&mut escaped, title, true);
        parts.push(format!(
            "#text(fill: {}, weight: 500, size: 11pt)[{escaped}]",
            color(&palette.accent),
        ));
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "align(left + horizon)[{}]",
        parts.join(" #h(0.6em) ")
    ))
}

/// Build a diagonal, semi-transparent gray watermark placed behind the page
/// body (via `page(background: …)`), centred and rotated across the page.
fn watermark_background(text: &str, palette: &Palette) -> String {
    let mut escaped = String::new();
    escape_markup(&mut escaped, text, true);
    let paint = color_with_alpha(&palette.counter, "40");
    format!(
        "align(center + horizon, rotate(-45deg, text(size: 48pt, weight: 700, fill: {paint})[{escaped}]))"
    )
}

/// Build the running footer: an optional watermark label (left), the page number
/// (centre, only when branded), and an optional timestamp (right), all muted.
/// Returns `None` when there is nothing to show.
fn footer_content(
    palette: &Palette,
    watermark: Option<&str>,
    timestamp: Option<&str>,
    show_page: bool,
) -> Option<String> {
    if watermark.is_none() && timestamp.is_none() && !show_page {
        return None;
    }
    let cell = |alignment: &str, body: String| format!("align({alignment})[{body}]");
    let left = cell("left", escaped_or_empty(watermark));
    let center = cell(
        "center",
        if show_page {
            "#context counter(page).display()".to_string()
        } else {
            String::new()
        },
    );
    let right = cell("right", escaped_or_empty(timestamp));
    Some(format!(
        "text(fill: {}, size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), {left}, {center}, {right})]",
        color(&palette.counter),
    ))
}

/// Escape optional text for a content block, or the empty string if absent.
fn escaped_or_empty(text: Option<&str>) -> String {
    match text {
        Some(text) => {
            let mut escaped = String::new();
            escape_markup(&mut escaped, text, true);
            escaped
        }
        None => String::new(),
    }
}

/// Format a colour string as a safely quoted Typst `rgb(…)` call.
fn color(hex: &str) -> String {
    format!("rgb({})", string_literal(hex))
}

fn color_with_alpha(hex: &str, alpha: &str) -> String {
    let mut rgba = String::with_capacity(9);
    rgba.push('#');
    match hex.strip_prefix('#') {
        Some(short) if short.len() == 3 => {
            for character in short.chars() {
                rgba.push(character);
                rgba.push(character);
            }
        }
        Some(value) => rgba.push_str(value),
        None => rgba.push_str(hex),
    }
    rgba.push_str(alpha);
    color(&rgba)
}

/// Format a font stack as a Typst tuple of string literals. The brand family is
/// only included when `brand_fonts` is set, so the default output never names a
/// font that isn't loaded. The bundled emoji family is always appended last so
/// emoji fall through to it after the text faces, whatever the theme.
fn font_tuple(stack: &FontStack, brand_fonts: bool) -> String {
    let mut families: Vec<&str> = Vec::new();
    if brand_fonts {
        families.extend(stack.brand.as_deref());
    }
    families.extend(stack.fallback.iter().map(String::as_str));
    families.push(EMOJI_FONT_FAMILY);
    let inner = families
        .iter()
        .map(|family| string_literal(family))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({inner})")
}

fn string_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    crate::escape::escape_string(&mut literal, value);
    literal.push('"');
    literal
}

impl PageSize {
    /// The Typst paper name for this page size.
    pub(crate) fn paper(self) -> &'static str {
        match self {
            PageSize::A4 => "a4",
            PageSize::Letter => "us-letter",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(brand: Option<&str>, fallback: &[&str]) -> FontStack {
        FontStack {
            brand: brand.map(str::to_owned),
            fallback: fallback.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn emoji_family_is_always_the_last_fallback() {
        // Even a theme that never mentions an emoji font gets one appended, so
        // emoji resolve after the text faces instead of becoming tofu.
        let s = stack(Some("Brand Sans"), &["IBM Plex Serif"]);
        assert_eq!(
            font_tuple(&s, false),
            format!("(\"IBM Plex Serif\", \"{EMOJI_FONT_FAMILY}\")")
        );
        assert_eq!(
            font_tuple(&s, true),
            format!("(\"Brand Sans\", \"IBM Plex Serif\", \"{EMOJI_FONT_FAMILY}\")")
        );
    }

    #[test]
    fn font_family_names_are_emitted_as_string_literals() {
        let hostile = r#"Acme"), size: 1pt)#undefined_function()//"#;
        let stack = stack(None, &[hostile, r"Back\slash"]);

        assert_eq!(
            font_tuple(&stack, false),
            format!(
                r#"("Acme\"), size: 1pt)#undefined_function()//", "Back\\slash", "{EMOJI_FONT_FAMILY}")"#
            )
        );
    }

    #[test]
    fn body_weight_is_applied_to_base_text() {
        let mut theme = Theme::default();
        theme.typography.body_weight = 500;
        let mut out = String::new();

        write_text(&mut out, &theme, &EmitOptions::default());

        assert!(out.contains("size: 11pt, weight: 500, fill:"));
    }

    #[test]
    fn table_header_helper_uses_the_theme_weight() {
        let mut theme = Theme::default();
        theme.typography.table_header_weight = 600;
        let mut out = String::new();

        write_helpers(&mut out, &theme);

        assert!(out.contains("#let tableheader(body) = text(weight: 600, body)"));
        assert!(!out.contains("table.cell.where(y: 0)"));
    }
}
