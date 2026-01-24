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
}
