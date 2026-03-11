//! HTML entity constants for Unicode character encoding.

/// Unicode characters that should be encoded as HTML numeric entities.
pub(crate) const HTML_ENTITY_MAPPINGS: &[(char, &str)] = &[
    ('\u{00A0}', "&#160;"),  // non-breaking space
    ('\u{200B}', "&#8203;"), // zero-width space
    ('\u{2060}', "&#8288;"), // word joiner
    ('\u{2018}', "&#8216;"), // left single quote
    ('\u{2019}', "&#8217;"), // right single quote
    ('\u{201C}', "&#8220;"), // left double quote
    ('\u{201D}', "&#8221;"), // right double quote
    ('\u{00B0}', "&#176;"),  // degree sign
    ('\u{00A6}', "&#166;"),  // broken bar
];

/// Encode Unicode characters to HTML numeric entities.
#[must_use]
pub(crate) fn encode_html_entities(text: &str) -> String {
    let mut result = text.to_string();
    for (char, entity) in HTML_ENTITY_MAPPINGS {
        result = result.replace(*char, entity);
    }
    result
}

/// Check if bytes starting at `start` (which should point to `&`) form a valid
/// HTML entity reference, returning the index of the closing `;` if so.
fn html_entity_end(bytes: &[u8], start: usize) -> Option<usize> {
    let after_amp = start + 1;
    let first = *bytes.get(after_amp)?;

    if first == b'#' {
        // Numeric character reference: &#digits; or &#xhex;
        let next = *bytes.get(after_amp + 1)?;
        if next == b'x' || next == b'X' {
            // Hex NCR: &#x[0-9a-fA-F]+;
            let mut i = after_amp + 2;
            let digit_start = i;
            while let Some(&b) = bytes.get(i) {
                if b.is_ascii_hexdigit() {
                    i += 1;
                } else {
                    break;
                }
            }
            if i > digit_start && bytes.get(i) == Some(&b';') {
                return Some(i);
            }
        } else if next.is_ascii_digit() {
            // Decimal NCR: &#[0-9]+;
            let mut i = after_amp + 1;
            while let Some(&b) = bytes.get(i) {
                if b.is_ascii_digit() {
                    i += 1;
                } else {
                    break;
                }
            }
            if bytes.get(i) == Some(&b';') {
                return Some(i);
            }
        }
    } else if first.is_ascii_alphabetic() {
        // Named entity: &[a-zA-Z][a-zA-Z0-9]*;
        let mut i = after_amp + 1;
        while let Some(&b) = bytes.get(i) {
            if b.is_ascii_alphanumeric() {
                i += 1;
            } else {
                break;
            }
        }
        if bytes.get(i) == Some(&b';') {
            return Some(i);
        }
    }

    None
}

/// Escape bare `&` characters to `&amp;`, preserving valid HTML entity references.
///
/// Named entities (`&euro;`), decimal NCRs (`&#167;`), and hex NCRs (`&#x00A7;`)
/// are passed through unchanged. Only `&` that does not begin a valid entity is escaped.
#[must_use]
pub(crate) fn escape_ampersands(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes.get(i) == Some(&b'&') {
            if let Some(end) = html_entity_end(bytes, i) {
                // Valid entity — copy it verbatim (all ASCII, safe to slice)
                result.push_str(&text[i..=end]);
                i = end + 1;
            } else {
                result.push_str("&amp;");
                i += 1;
            }
        } else if let Some(ch) = text[i..].chars().next() {
            result.push(ch);
            i += ch.len_utf8();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_html_entities() {
        // Non-breaking space
        assert_eq!(
            encode_html_entities("before\u{00A0}after"),
            "before&#160;after"
        );

        // Zero-width space
        assert_eq!(encode_html_entities("a\u{200B}b"), "a&#8203;b");

        // Curly quotes
        assert_eq!(
            encode_html_entities("\u{201C}Hello\u{201D}"),
            "&#8220;Hello&#8221;"
        );

        // Degree sign
        assert_eq!(encode_html_entities("100\u{00B0}F"), "100&#176;F");

        // Multiple characters
        assert_eq!(
            encode_html_entities("Test \u{00A0}\u{2018}quote\u{2019}"),
            "Test &#160;&#8216;quote&#8217;"
        );

        // No special characters
        assert_eq!(encode_html_entities("plain text"), "plain text");
    }

    #[test]
    fn test_escape_ampersands_bare() {
        assert_eq!(escape_ampersands("a & b"), "a &amp; b");
        assert_eq!(escape_ampersands("&"), "&amp;");
        assert_eq!(escape_ampersands("&&"), "&amp;&amp;");
        assert_eq!(escape_ampersands("trailing &"), "trailing &amp;");
        assert_eq!(escape_ampersands(""), "");
    }

    #[test]
    fn test_escape_ampersands_named_entities() {
        assert_eq!(escape_ampersands("&euro;"), "&euro;");
        assert_eq!(escape_ampersands("&amp;"), "&amp;");
        assert_eq!(escape_ampersands("&lt;"), "&lt;");
        assert_eq!(escape_ampersands("&gt;"), "&gt;");
        assert_eq!(escape_ampersands("&copy;"), "&copy;");
        assert_eq!(escape_ampersands("&mdash;"), "&mdash;");
    }

    #[test]
    fn test_escape_ampersands_decimal_ncr() {
        assert_eq!(escape_ampersands("&#167;"), "&#167;");
        assert_eq!(escape_ampersands("&#8212;"), "&#8212;");
    }

    #[test]
    fn test_escape_ampersands_hex_ncr() {
        assert_eq!(escape_ampersands("&#x00A7;"), "&#x00A7;");
        assert_eq!(escape_ampersands("&#X2014;"), "&#X2014;");
        assert_eq!(escape_ampersands("&#xABcd;"), "&#xABcd;");
    }

    #[test]
    fn test_escape_ampersands_invalid_entities() {
        // &; — empty name
        assert_eq!(escape_ampersands("&;"), "&amp;;");
        // &#; — no digits
        assert_eq!(escape_ampersands("&#;"), "&amp;#;");
        // &#x; — no hex digits
        assert_eq!(escape_ampersands("&#x;"), "&amp;#x;");
        // &123; — starts with digit, not alpha
        assert_eq!(escape_ampersands("&123;"), "&amp;123;");
    }

    #[test]
    fn test_escape_ampersands_edge_cases() {
        // & immediately before a valid entity
        assert_eq!(escape_ampersands("&&euro;"), "&amp;&euro;");
        // Mixed content with UTF-8
        assert_eq!(
            escape_ampersands("Price: 10 &euro; — résumé & CV"),
            "Price: 10 &euro; — résumé &amp; CV"
        );
        // Multiple entities in sequence
        assert_eq!(escape_ampersands("&lt;&gt;"), "&lt;&gt;");
    }
}
