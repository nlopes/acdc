//! Custom input generators for property-based testing
//!
//! These generators create various types of input strings that help
//! test different aspects of the parser. We start simple and add
//! complexity as needed.
#![allow(clippy::expect_used)]
use proptest::prelude::*;

/// Generate any string including edge cases like empty, very long,
/// with control characters, invalid UTF-8 sequences (via surrogates), etc.
pub fn any_document_string() -> impl Strategy<Value = String> {
    prop::string::string_regex(".*").expect("Failed to create any string strategy")
}

/// Generate ASCII-safe document strings that are more likely to be
/// valid `AsciiDoc` but still exercise the parser thoroughly.
pub fn ascii_document() -> impl Strategy<Value = String> {
    prop::string::string_regex(r"[\x20-\x7E\n\t]*").expect("Failed to create ASCII string strategy")
}

/// Generate strings with AsciiDoc-like structure but potentially invalid.
/// This helps find issues with delimiter matching, nesting, etc.
pub fn structured_document() -> impl Strategy<Value = String> {
    // Start simple, we can enhance this later with actual AsciiDoc patterns
    prop::collection::vec(
        prop_oneof![
            Just("= Title\n\n".to_string()),
            Just("== Section\n\n".to_string()),
            Just("* list item\n".to_string()),
            Just("----\ncode block\n----\n".to_string()),
            Just("Some paragraph text.\n\n".to_string()),
            Just("[NOTE]\n====\nAdmonition\n====\n".to_string()),
            Just("`inline code`".to_string()),
            Just("*bold text*".to_string()),
            Just("_italic text_".to_string()),
            Just("<<reference>>".to_string()),
            prop::string::string_regex(r"[a-zA-Z0-9 .,!?\n]+")
                .expect("Failed to create text chunk"),
        ],
        0..20, // 0 to 20 chunks
    )
    .prop_map(|chunks| chunks.join(""))
}

/// Generate potentially problematic Unicode strings to test UTF-8 handling.
pub fn unicode_stress_test() -> impl Strategy<Value = String> {
    prop_oneof![
        // ASCII with newlines and tabs
        prop::string::string_regex(r"[\x20-\x7E\n\t]{0,100}")
            .expect("Failed to create ASCII string strategy"),
        // Emoji
        prop::collection::vec(
            prop_oneof![Just("😀"), Just("🎉"), Just("🎨"), Just("🚀"), Just("💻")],
            0..20
        )
        .prop_map(|v| v.join("")),
        // Right-to-left text (Hebrew)
        prop::collection::vec(
            prop_oneof![
                Just("א"),
                Just("ב"),
                Just("ג"),
                Just("ד"),
                Just("ה"),
                Just("ו"),
                Just("ז"),
                Just("ח"),
                Just("ט"),
                Just("י"),
                Just(" ")
            ],
            0..30
        )
        .prop_map(|v| v.join("")),
        // Multi-byte characters (CJK)
        prop::collection::vec(
            prop_oneof![
                Just("一"),
                Just("二"),
                Just("三"),
                Just("四"),
                Just("五"),
                Just("六"),
                Just("七"),
                Just("八"),
                Just("九"),
                Just("十")
            ],
            0..20
        )
        .prop_map(|v| v.join("")),
        // Combining characters with base chars
        prop::collection::vec(
            prop_oneof![Just("a"), Just("e"), Just("i"), Just("o"), Just("u")],
            0..10
        )
        .prop_map(|chars| {
            chars
                .into_iter()
                .flat_map(|c| [c, "\u{0301}"]) // Add combining acute accent
                .collect::<String>()
        }),
        // Zero-width and special Unicode
        prop::collection::vec(
            prop_oneof![
                Just("\u{200B}"), // Zero-width space
                Just("\u{FEFF}"), // Zero-width no-break space
                Just("\u{200C}"), // Zero-width non-joiner
                Just("\u{200D}"), // Zero-width joiner
                Just("a"),
                Just(" "),
                Just("\n")
            ],
            0..20
        )
        .prop_map(|v| v.join("")),
    ]
}
