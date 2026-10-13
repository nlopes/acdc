use crate::{Error, Theme, color::canonical_colour};

use super::{CompatibilityWarning, ConvertedTheme, Entry, expression, value::Value};

pub(super) fn warn(result: &mut ConvertedTheme, entry: &Entry, message: impl Into<String>) {
    if !entry.inherited {
        result.warnings.push(CompatibilityWarning {
            key: entry.path.clone(),
            message: message.into(),
        });
    }
}

fn number(entry: &Entry) -> Result<f64, Error> {
    let value = entry
        .value
        .text()
        .and_then(expression::evaluate)
        .ok_or_else(|| {
            Error::validation(
                &entry.path,
                "expected a finite measurement in pt, in, cm, mm, px, or pc",
            )
        })?;
    if value < 0.0 {
        return Err(Error::validation(
            &entry.path,
            "expected a non-negative measurement",
        ));
    }
    Ok(value)
}

pub(super) fn font_size(entry: &Entry, base: f64) -> Result<f64, Error> {
    let value = if let Some(text) = entry.value.text()
        && let Some(relative) = text.strip_suffix("em")
    {
        expression::evaluate(relative)
            .map(|value| value * base)
            .ok_or_else(|| Error::validation(&entry.path, "expected a numeric em size"))?
    } else {
        number(entry)?
    };
    if !value.is_finite() || value <= 0.0 {
        return Err(Error::validation(
            &entry.path,
            "expected a finite positive font size",
        ));
    }
    Ok(value)
}

fn colour(entry: &Entry) -> Result<String, Error> {
    let value = entry.value.colour_text().ok_or_else(|| {
        Error::validation(&entry.path, "expected a quoted RGB hexadecimal colour")
    })?;
    canonical_colour("colour", &format!("#{}", value.trim_start_matches('#'))).map_err(|_| {
        Error::validation(
            &entry.path,
            "expected an RGB colour in #rgb or #rrggbb form",
        )
    })
}

fn choice<'a>(entry: &'a Entry, values: &[&str]) -> Result<&'a str, Error> {
    let value = entry.value.text().ok_or_else(|| {
        Error::validation(
            &entry.path,
            format!("expected one of {}", values.join(", ")),
        )
    })?;
    if !values.contains(&value) {
        return Err(Error::validation(
            &entry.path,
            format!("expected one of {}", values.join(", ")),
        ));
    }
    Ok(value)
}

pub(super) fn apply(result: &mut ConvertedTheme, key: &str, entry: &Entry) -> Result<(), Error> {
    if let Some(target) = colour_target(&mut result.theme, key) {
        if unsupported_colour(&entry.value) {
            warn(
                result,
                entry,
                "unsupported CMYK, transparent, or inherited colour; acdc's default applies",
            );
            return Ok(());
        }
        *target = colour(entry)?;
        if matches!(
            key,
            "heading_font_color" | "footer_font_color" | "quote_border_color"
        ) {
            warn(
                result,
                entry,
                "mapped to a shared acdc colour; its scope differs from Asciidoctor PDF",
            );
        }
    } else if let Some(target) = measurement_target(&mut result.theme, key) {
        *target = number(entry)?;
        if key == "block_margin_bottom" {
            warn(
                result,
                entry,
                "acdc applies this spacing to all document blocks; separate prose spacing is not supported",
            );
        }
    } else if key.starts_with("heading_h") && key.ends_with("_font_size") {
        let level = key
            .strip_prefix("heading_h")
            .and_then(|key| key.strip_suffix("_font_size"))
            .and_then(|level| level.parse::<usize>().ok())
            .filter(|level| (1..=6).contains(level));
        if let Some(level) = level {
            let size = font_size(entry, result.theme.typography.body_size_pt)?;
            if level == 1 {
                warn(
                    result,
                    entry,
                    "separate document-title and book-part sizes are unsupported; acdc keeps its title size and uses the first section size for parts",
                );
            } else if let Some(target) = result.theme.typography.heading_pt.get_mut(level - 2) {
                // Asciidoctor h2 is a section or chapter; acdc starts sections at level 1.
                *target = size;
            }
        } else {
            warn(result, entry, "unsupported heading level");
        }
    } else {
        apply_other(result, key, entry)?;
    }
    Ok(())
}

fn colour_target<'a>(theme: &'a mut Theme, key: &str) -> Option<&'a mut String> {
    Some(match key {
        "page_background_color" => &mut theme.palette.page_bg,
        "base_font_color" => &mut theme.palette.body_text,
        "heading_font_color" => &mut theme.palette.heading,
        "link_font_color" => &mut theme.palette.link,
        "header_font_color" => &mut theme.palette.accent,
        "footer_font_color" => &mut theme.palette.counter,
        "quote_font_color" => &mut theme.palette.quote_text,
        "quote_border_color" => &mut theme.palette.quote_rule,
        "code_background_color" => &mut theme.palette.code_bg,
        "code_font_color" => &mut theme.palette.code_fg,
        "table_border_color" => &mut theme.table.border_color,
        "table_body_stripe_background_color" => &mut theme.table.stripe_background,
        _ => return None,
    })
}

fn unsupported_colour(value: &Value) -> bool {
    matches!(value, Value::Sequence(_) | Value::Null) || value.text() == Some("transparent")
}

fn measurement_target<'a>(theme: &'a mut Theme, key: &str) -> Option<&'a mut f64> {
    Some(match key {
        "block_margin_bottom" => &mut theme.spacing.block_margin_bottom_pt,
        "code_border_radius" => &mut theme.spacing.code_radius_pt,
        "quote_border_left_width" => &mut theme.spacing.quote_rule_pt,
        "caption_margin_inside" => &mut theme.caption.margin_inside_pt,
        "caption_margin_outside" => &mut theme.caption.margin_outside_pt,
        "table_border_width" => &mut theme.table.border_width_pt,
        "table_head_border_bottom_width" => &mut theme.table.header_divider_width_pt,
        _ => return None,
    })
}

pub(super) fn set_font(result: &mut ConvertedTheme, key: &str, entry: &Entry) -> Result<(), Error> {
    let Value::String(family) = &entry.value else {
        return Err(Error::validation(
            &entry.path,
            "expected a font family name",
        ));
    };
    if family.trim().is_empty() || family.trim() != family || family.chars().any(char::is_control) {
        return Err(Error::validation(
            &entry.path,
            "expected a non-empty font family without control characters or surrounding whitespace",
        ));
    }
    let stack = match key {
        "base_font_family" => &mut result.theme.typography.body_font,
        "heading_font_family" => &mut result.theme.typography.heading_font,
        _ => &mut result.theme.typography.mono_font,
    };
    if [
        "IBM Plex Serif",
        "IBM Plex Sans",
        "IBM Plex Mono",
        "IBM Plex Math",
    ]
    .contains(&family.as_str())
    {
        stack.brand = None;
        stack.fallback.retain(|fallback| fallback != family);
        stack.fallback.insert(0, family.clone());
    } else {
        stack.brand = Some(family.clone());
        warn(
            result,
            entry,
            "font family requires --font-dir; acdc uses bundled fallbacks when no font directory is supplied",
        );
    }
    if key == "code_font_family" {
        warn(
            result,
            entry,
            "acdc shares this font with inline monospace text and monospace table cells",
        );
    }
    Ok(())
}

fn apply_other(result: &mut ConvertedTheme, key: &str, entry: &Entry) -> Result<(), Error> {
    if apply_page(result, key, entry)? {
        return Ok(());
    }
    if apply_typography(result, key, entry)? {
        return Ok(());
    }
    if matches!(
        key,
        "caption_font_color" | "table_head_background_color" | "table_foot_background_color"
    ) && unsupported_colour(&entry.value)
    {
        if matches!(entry.value, Value::Null) {
            match key {
                "caption_font_color" => result.theme.caption.font_color = None,
                "table_head_background_color" => result.theme.table.header_background = None,
                _ => result.theme.table.footer_background = None,
            }
        } else {
            warn(
                result,
                entry,
                "unsupported CMYK or transparent colour; acdc's default applies",
            );
        }
        return Ok(());
    }
    match key {
        "caption_font_color" => result.theme.caption.font_color = Some(colour(entry)?),
        "table_head_background_color" => {
            result.theme.table.header_background = Some(colour(entry)?);
        }
        "table_foot_background_color" => {
            result.theme.table.footer_background = Some(colour(entry)?);
        }
        "caption_align" | "caption_text_align" => {
            result.theme.caption.align = match choice(entry, &["left", "center", "right"])? {
                "center" => crate::CaptionAlignment::Center,
                "right" => crate::CaptionAlignment::Right,
                _ => crate::CaptionAlignment::Left,
            };
            warn(
                result,
                entry,
                "acdc uses one alignment for both caption placement and text",
            );
        }
        "table_align" => {
            result.theme.table.align = match choice(entry, &["left", "center", "right"])? {
                "center" => crate::TableAlignment::Center,
                "right" => crate::TableAlignment::Right,
                _ => crate::TableAlignment::Left,
            };
        }
        "heading_part_break_before" => {
            result.theme.heading.part.break_before = break_before(entry)?;
        }
        "heading_part_break_after" => {
            result.theme.heading.part.break_after =
                match choice(entry, &["auto", "always", "avoid"])? {
                    "always" => crate::PartBreakAfter::Always,
                    "avoid" => crate::PartBreakAfter::Avoid,
                    _ => crate::PartBreakAfter::Auto,
                };
        }
        "heading_chapter_break_before" => {
            result.theme.heading.chapter.break_before = break_before(entry)?;
        }
        "index_columns" => {
            result.theme.index.columns = entry
                .value
                .text()
                .and_then(|value| value.parse().ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| Error::validation(&entry.path, "expected a positive integer"))?;
        }
        "index_column_gap" => result.theme.index.column_gap_pt = Some(number(entry)?),
        "code_padding" => {
            let [top, right, bottom, left] = margins(entry)?;
            if [right, bottom, left]
                .into_iter()
                .all(|value| (value - top).abs() < f64::EPSILON)
            {
                result.theme.spacing.code_pad_pt = top;
            } else {
                warn(
                    result,
                    entry,
                    "unsupported asymmetric code padding; acdc's code padding is uniform",
                );
            }
        }
        _ => warn(
            result,
            entry,
            "unsupported style setting; acdc's default applies (the value remains available to theme references)",
        ),
    }
    Ok(())
}

fn apply_typography(result: &mut ConvertedTheme, key: &str, entry: &Entry) -> Result<bool, Error> {
    match key {
        "base_font_family" | "heading_font_family" | "code_font_family" => {
            set_font(result, key, entry)?;
        }
        "codespan_font_family" => warn(
            result,
            entry,
            "acdc shares the code-block font; code.font_family takes precedence when supplied",
        ),
        "code_font_size" => {
            result.theme.typography.code_size_em =
                font_size(entry, result.theme.typography.body_size_pt)?
                    / result.theme.typography.body_size_pt;
        }
        "code_font_size_min" => {
            result.theme.typography.code_min_size_em =
                font_size(entry, result.theme.typography.body_size_pt)?
                    / result.theme.typography.body_size_pt;
        }
        "caption_font_size" => {
            result.theme.caption.font_size_em =
                font_size(entry, result.theme.typography.body_size_pt)?
                    / result.theme.typography.body_size_pt;
        }
        "header_font_size" => {
            result.theme.header.font_size_pt =
                font_size(entry, result.theme.typography.body_size_pt)?;
        }
        "footer_font_size" => {
            result.theme.footer.font_size_pt =
                font_size(entry, result.theme.typography.body_size_pt)?;
        }
        "base_font_style"
        | "heading_font_style"
        | "table_head_font_style"
        | "caption_font_style" => set_style(result, key, entry)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn apply_page(result: &mut ConvertedTheme, key: &str, entry: &Entry) -> Result<bool, Error> {
    match key {
        "page_margin" => result.theme.page.margin_pt = Some(margins(entry)?),
        "page_layout" => {
            result.theme.page.layout = Some(match choice(entry, &["portrait", "landscape"])? {
                "landscape" => crate::PageOrientation::Landscape,
                _ => crate::PageOrientation::Portrait,
            });
        }
        "page_size" => set_page_size(result, entry)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn set_style(result: &mut ConvertedTheme, key: &str, entry: &Entry) -> Result<(), Error> {
    let style = choice(
        entry,
        &["normal", "bold", "italic", "bold_italic", "normal_italic"],
    )?;
    let weight = if style.contains("bold") { 700 } else { 400 };
    match key {
        "base_font_style" => result.theme.typography.body_weight = weight,
        "heading_font_style" => result.theme.typography.heading_weight = weight,
        "table_head_font_style" => result.theme.typography.table_header_weight = weight,
        _ => {
            result.theme.caption.font_weight = weight;
            result.theme.caption.font_style = if style.contains("italic") {
                crate::CaptionFontStyle::Italic
            } else {
                crate::CaptionFontStyle::Normal
            };
        }
    }
    if key != "caption_font_style" && style.contains("italic") {
        warn(
            result,
            entry,
            "unsupported italic style for this category; font weight was preserved",
        );
    }
    Ok(())
}

fn break_before(entry: &Entry) -> Result<crate::PageBreakBefore, Error> {
    Ok(if choice(entry, &["auto", "always"])? == "always" {
        crate::PageBreakBefore::Always
    } else {
        crate::PageBreakBefore::Auto
    })
}

fn margins(entry: &Entry) -> Result<[f64; 4], Error> {
    let values = match &entry.value {
        Value::Sequence(values) => values
            .iter()
            .map(|value| {
                number(&Entry {
                    value: value.clone(),
                    ..entry.clone()
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Value::Null | Value::Bool | Value::Number(..) | Value::String(_) | Value::Mapping(_) => {
            vec![number(entry)?]
        }
    };
    match values.as_slice() {
        [all] => Ok([*all; 4]),
        [vertical, horizontal] => Ok([*vertical, *horizontal, *vertical, *horizontal]),
        [top, horizontal, bottom] => Ok([*top, *horizontal, *bottom, *horizontal]),
        [top, right, bottom, left] => Ok([*top, *right, *bottom, *left]),
        _ => Err(Error::validation(
            &entry.path,
            "expected one to four margin measurements",
        )),
    }
}

fn set_page_size(result: &mut ConvertedTheme, entry: &Entry) -> Result<(), Error> {
    if let Value::Sequence(values) = &entry.value {
        let [width, height] = values.as_slice() else {
            return Err(Error::validation(
                &entry.path,
                "expected two page dimensions",
            ));
        };
        let width = number(&Entry {
            value: width.clone(),
            ..entry.clone()
        })?;
        let height = number(&Entry {
            value: height.clone(),
            ..entry.clone()
        })?;
        if width <= 0.0 || height <= 0.0 {
            return Err(Error::validation(
                &entry.path,
                "page dimensions must be positive",
            ));
        }
        result.theme.page.size_pt = Some([width, height]);
    } else {
        let name = entry.value.text().ok_or_else(|| {
            Error::validation(&entry.path, "expected a page name or two dimensions")
        })?;
        result.theme.page.size_pt = match name.to_ascii_lowercase().as_str() {
            "a3" => Some([841.89, 1190.551]),
            "a4" => Some([595.276, 841.89]),
            "a5" => Some([419.528, 595.276]),
            "letter" | "us-letter" => Some([612.0, 792.0]),
            "legal" | "us-legal" => Some([612.0, 1008.0]),
            "executive" | "us-executive" => Some([522.0, 756.0]),
            "tabloid" | "us-tabloid" => Some([792.0, 1224.0]),
            _ => None,
        };
        if result.theme.page.size_pt.is_none() {
            warn(
                result,
                entry,
                "unsupported page size; use --page or :pdf-page-size: with a supported name or explicit dimensions",
            );
        }
    }
    Ok(())
}
