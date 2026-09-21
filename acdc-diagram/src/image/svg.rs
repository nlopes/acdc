//! Normalising generated SVG.
//!
//! Diagram tools emit wildly different root elements: some omit the SVG
//! namespace, some give a `viewBox` but no size, some give a size in points.
//! Browsers and the HTML backend both behave better with all three present,
//! so the root start tag is rewritten to carry `xmlns`, `preserveAspectRatio`
//! and a `viewBox`, and the intrinsic size is derived in pixels.
//!
//! Only the root start tag is rewritten — the rest of the document is passed
//! through byte-for-byte, apart from comment stripping when optimisation is
//! on. That keeps generator output (gradients, fonts, embedded scripts) exactly
//! as the tool produced it.

use super::ProcessedImage;
use crate::error::{Error, Result};

/// Normalise and measure an SVG document.
///
/// # Errors
///
/// Returns [`Error::Image`] when the bytes are not valid UTF-8 or contain no
/// `<svg>` root element.
pub(super) fn post_process(data: Vec<u8>, optimise: bool) -> Result<ProcessedImage> {
    let text = String::from_utf8(data)
        .map_err(|_| Error::Image("SVG output is not valid UTF-8".to_string()))?;
    let text = if optimise {
        strip_comments(&text)
    } else {
        text
    };

    let Some(tag) = find_root_tag(&text) else {
        return Err(Error::Image(
            "generated output does not contain an <svg> root element".to_string(),
        ));
    };

    let mut attributes = parse_attributes(&text[tag.attributes_start..tag.attributes_end]);

    if !has(&attributes, "xmlns") {
        attributes.push((
            "xmlns".to_string(),
            "http://www.w3.org/2000/svg".to_string(),
        ));
    }
    if !has(&attributes, "preserveAspectRatio") {
        attributes.push((
            "preserveAspectRatio".to_string(),
            "xMidYMid meet".to_string(),
        ));
    }

    let declared = get(&attributes, "width")
        .and_then(parse_length)
        .zip(get(&attributes, "height").and_then(parse_length));
    let view_box = get(&attributes, "viewBox").and_then(parse_view_box);

    // A declared width/height wins; otherwise the viewBox extent is the
    // intrinsic size, measured from its origin so a shifted box still reports
    // the drawing's own dimensions.
    let (width, height) = match (declared, view_box.as_ref()) {
        (Some((w, h)), _) => (Some(w), Some(h)),
        (None, Some(view_box)) => (
            Some(view_box.width - view_box.min_x),
            Some(view_box.height - view_box.min_y),
        ),
        (None, None) => (None, None),
    };

    if view_box.is_none()
        && let (Some(width), Some(height)) = (width, height)
    {
        attributes.push((
            "viewBox".to_string(),
            format!("0 0 {} {}", number(width), number(height)),
        ));
    }

    let mut out = String::with_capacity(text.len() + 96);
    out.push_str(&text[..tag.attributes_start]);
    for (name, value) in &attributes {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        out.push_str(value);
        out.push('"');
    }
    out.push_str(&text[tag.attributes_end..]);

    Ok(ProcessedImage {
        data: out.into_bytes(),
        width,
        height,
    })
}

/// Byte offsets delimiting the attribute run inside the root `<svg …>` tag.
struct RootTag {
    attributes_start: usize,
    attributes_end: usize,
}

/// Locate the root `<svg>` start tag and the span holding its attributes.
fn find_root_tag(text: &str) -> Option<RootTag> {
    let bytes = text.as_bytes();
    let mut search = 0;
    let open = loop {
        let found = text.get(search..)?.find("<svg")? + search;
        // Guard against `<svgfoo`: a real start tag ends the name here.
        let ends_name = bytes
            .get(found + 4)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if ends_name {
            break found;
        }
        search = found + 4;
    };

    let attributes_start = open + 4;
    // Scan to the end of the start tag, ignoring anything inside an attribute
    // value so that a `>` in a style or a font name does not end it early.
    let mut index = attributes_start;
    let mut quote: Option<u8> = None;
    while let Some(&byte) = bytes.get(index) {
        if let Some(open_quote) = quote {
            if byte == open_quote {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' || (byte == b'/' && bytes.get(index + 1) == Some(&b'>')) {
            break;
        }
        index += 1;
    }

    Some(RootTag {
        attributes_start,
        attributes_end: index,
    })
}

/// Split an attribute run into name/value pairs, preserving source order.
fn parse_attributes(run: &str) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    let mut rest = run;

    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let name_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let (name, after_name) = rest.split_at(name_end);
        if name.is_empty() {
            break;
        }
        let after_name = after_name.trim_start();
        let Some(after_equals) = after_name.strip_prefix('=') else {
            // A valueless attribute is not legal in SVG, but tolerate it
            // rather than dropping everything that follows.
            attributes.push((name.to_string(), String::new()));
            rest = after_name;
            continue;
        };
        let after_equals = after_equals.trim_start();
        let quoted = after_equals
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\''));
        let (value, remainder) = if let Some(quote) = quoted {
            let body = &after_equals[quote.len_utf8()..];
            body.find(quote).map_or((body, ""), |end| {
                (&body[..end], &body[end + quote.len_utf8()..])
            })
        } else {
            let end = after_equals
                .find(char::is_whitespace)
                .unwrap_or(after_equals.len());
            after_equals.split_at(end)
        };
        attributes.push((name.to_string(), value.to_string()));
        rest = remainder;
    }

    attributes
}

fn has(attributes: &[(String, String)], name: &str) -> bool {
    attributes.iter().any(|(key, _)| key == name)
}

fn get<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// A parsed `viewBox`, in user units.
struct ViewBox {
    min_x: f64,
    min_y: f64,
    width: f64,
    height: f64,
}

fn parse_view_box(value: &str) -> Option<ViewBox> {
    let numbers: Vec<f64> = value
        .split([' ', ',', '\t', '\n', '\r'])
        .filter(|part| !part.is_empty())
        .map(str::parse::<f64>)
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    let [min_x, min_y, width, height] = numbers.as_slice() else {
        return None;
    };
    Some(ViewBox {
        min_x: *min_x,
        min_y: *min_y,
        width: *width,
        height: *height,
    })
}

/// Convert a CSS length such as `120`, `120px` or `90pt` to pixels.
///
/// Anything with a unit other than `pt` is taken as already being in pixels,
/// and percentages are rejected because they say nothing about intrinsic size.
fn parse_length(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    let split = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let number: f64 = number.parse().ok()?;
    match unit.trim() {
        "" | "px" => Some(number),
        // The factor asciidoctor-diagram uses: 96dpi / 72pt-per-inch.
        "pt" => Some(number * 1.33),
        _ => None,
    }
}

/// Render a measurement without a trailing `.0` on whole numbers.
fn number(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

/// Remove XML comments.
fn strip_comments(text: &str) -> String {
    if !text.contains("<!--") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        // An unterminated comment: keep what is left verbatim rather than
        // silently truncating the document.
        let Some(end) = rest[start..].find("-->") else {
            out.push_str(&rest[start..]);
            return out;
        };
        rest = &rest[start + end + 3..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    fn process(input: &str) -> ProcessedImage {
        post_process(input.as_bytes().to_vec(), true).expect("valid svg")
    }

    #[test]
    fn adds_namespace_and_viewbox_from_size() {
        let result = process(r#"<svg width="100" height="50"><rect/></svg>"#);
        let text = String::from_utf8(result.data).expect("utf-8");
        assert!(text.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
        assert!(text.contains(r#"preserveAspectRatio="xMidYMid meet""#));
        assert!(text.contains(r#"viewBox="0 0 100 50""#));
        assert_eq!(result.width, Some(100.0));
        assert_eq!(result.height, Some(50.0));
    }

    #[test]
    fn derives_size_from_viewbox() {
        let result = process(r#"<svg viewBox="0 0 640 480"><rect/></svg>"#);
        assert_eq!(result.width, Some(640.0));
        assert_eq!(result.height, Some(480.0));
        let text = String::from_utf8(result.data).expect("utf-8");
        assert_eq!(text.matches("viewBox").count(), 1);
    }

    #[test]
    fn converts_points_to_pixels() {
        let result = process(r#"<svg width="100pt" height="50pt"/>"#);
        assert_eq!(result.width, Some(133.0));
        assert_eq!(result.height, Some(66.5));
    }

    #[test]
    fn keeps_existing_attributes_and_body() {
        let result = process(
            r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><!-- hi --><g id="a"/></svg>"#,
        );
        let text = String::from_utf8(result.data).expect("utf-8");
        assert!(text.starts_with(r#"<?xml version="1.0"?><svg "#));
        assert!(text.contains(r#"<g id="a"/>"#));
        assert!(!text.contains("<!--"));
        assert_eq!(text.matches("xmlns=").count(), 1);
    }

    #[test]
    fn rejects_non_svg_output() {
        let error = post_process(b"parse error on line 3".to_vec(), true);
        assert!(error.is_err());
    }
}
