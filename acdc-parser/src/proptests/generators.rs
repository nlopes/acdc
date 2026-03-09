//! Custom input generators for property-based testing
//!
//! These generators create various types of input strings that help
//! test different aspects of the parser. They are organized in layers:
//! - Primitive helpers (private): small building blocks
//! - General generators: `any_document_string`, `ascii_document`, etc.
//! - Structured generators: `structured_document` with broad construct variety
//! - Targeted generators: specific complex constructs (tables, lists, etc.)
//! - Composite generators: realistic multi-construct documents
#![allow(clippy::expect_used)]
use std::fmt::Write as _;

use proptest::prelude::*;

// ====================================================================
// Primitive helpers (private)
// ====================================================================

fn any_word() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z][a-zA-Z0-9]{0,10}").expect("Failed to create word strategy")
}

fn any_text_line() -> impl Strategy<Value = String> {
    prop::collection::vec(any_word(), 1..=8).prop_map(|words| words.join(" "))
}

// ====================================================================
// General generators
// ====================================================================

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

// ====================================================================
// Structured generator (broad construct variety)
// ====================================================================

/// Generate strings with AsciiDoc-like structure but potentially invalid.
/// This helps find issues with delimiter matching, nesting, etc.
pub fn structured_document() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            // --- Original entries ---
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
            // --- Additional block constructs ---
            Just(". ordered item\n".to_string()),
            Just("term:: description\n".to_string()),
            Just(":author: Name\n".to_string()),
            Just("'''\n".to_string()),
            Just("<<<\n\n".to_string()),
            Just("image::photo.jpg[]\n\n".to_string()),
            Just("// a comment\n".to_string()),
            Just("[discrete]\n=== Discrete\n\n".to_string()),
            Just(".Block Title\n".to_string()),
            Just("=== Level 3\n\n".to_string()),
            Just("==== Level 4\n\n".to_string()),
            // --- Additional inline constructs ---
            Just("#highlighted#".to_string()),
            Just("~sub~".to_string()),
            Just("^sup^".to_string()),
            // Short text adjacent to sub/sup/highlight (SC-181 regression)
            Just("?~sub~".to_string()),
            Just("a^sup^".to_string()),
            Just("x#h#".to_string()),
            Just("footnote:[a note]".to_string()),
            Just("kbd:[Ctrl+C]".to_string()),
            Just("btn:[OK]".to_string()),
            Just("menu:File[Save]".to_string()),
            Just("https://example.com".to_string()),
            Just("((index))".to_string()),
            Just("text +\n".to_string()),
            // --- Small compound blocks ---
            Just("|===\n| a | b\n| c | d\n|===\n\n".to_string()),
            Just("****\nSidebar content.\n****\n\n".to_string()),
            Just("____\nQuote content.\n____\n\n".to_string()),
        ],
        0..20,
    )
    .prop_map(|chunks| chunks.join(""))
}

// ====================================================================
// Targeted construct generators
// ====================================================================

/// Generate PSV tables with varying dimensions.
/// Tables have the most complex grammar rules (~600 lines) with
/// recursive cell parsing, delimiter matching, and column handling.
pub fn table_document() -> impl Strategy<Value = String> {
    let cell = any_word();
    let col_count = 1..=4usize;
    let row_count = 1..=5usize;

    (col_count, row_count, prop::collection::vec(cell, 1..=20)).prop_map(|(cols, rows, words)| {
        let mut doc = String::from("|===\n");
        let mut word_idx = 0;
        for _ in 0..rows {
            let mut row = String::new();
            for col in 0..cols {
                if col > 0 {
                    row.push(' ');
                }
                row.push_str("| ");
                if let Some(word) = words.get(word_idx % words.len()) {
                    row.push_str(word);
                }
                word_idx += 1;
            }
            row.push('\n');
            doc.push_str(&row);
        }
        doc.push_str("|===\n");
        doc
    })
}

/// Generate description lists with varying delimiter styles.
/// Description lists have complex term/delimiter/description separation
/// and support multiple delimiter variants.
pub fn description_list_document() -> impl Strategy<Value = String> {
    let delimiter = prop_oneof![Just(":: "), Just("::: "), Just(":::: "), Just(";; "),];
    let item_count = 1..=5usize;

    (
        item_count,
        prop::collection::vec((any_word(), delimiter, any_text_line()), 1..=5),
    )
        .prop_map(|(count, items)| {
            let mut doc = String::new();
            for (i, (term, delim, desc)) in items.into_iter().enumerate() {
                if i >= count {
                    break;
                }
                doc.push_str(&term);
                doc.push_str(delim);
                doc.push_str(&desc);
                doc.push('\n');
            }
            doc.push('\n');
            doc
        })
}

/// Generate nested unordered and ordered lists (up to 2 levels).
/// List nesting is "inherently difficult with PEG" per project docs.
pub fn nested_list_document() -> impl Strategy<Value = String> {
    let list_type = prop_oneof![Just("ul"), Just("ol")];
    let item_count = 1..=5usize;

    (
        list_type,
        item_count,
        prop::collection::vec((prop::bool::ANY, any_text_line()), 1..=8),
    )
        .prop_map(|(lt, count, items)| {
            let (marker_l1, marker_l2) = if lt == "ul" {
                ("* ", "** ")
            } else {
                (". ", ".. ")
            };
            let mut doc = String::new();
            for (i, (nested, text)) in items.into_iter().enumerate() {
                if i >= count {
                    break;
                }
                if nested && i > 0 {
                    doc.push_str(marker_l2);
                } else {
                    doc.push_str(marker_l1);
                }
                doc.push_str(&text);
                doc.push('\n');
            }
            doc.push('\n');
            doc
        })
}

/// Generate various delimited blocks with paragraph content inside.
/// Tests delimiter matching and content recursion for all block types.
pub fn delimited_block_document() -> impl Strategy<Value = String> {
    let block_type = prop_oneof![
        Just(("****", "sidebar")),
        Just(("____", "quote")),
        Just(("====", "example")),
        Just(("----", "listing")),
        Just(("++++", "passthrough")),
        Just(("////", "comment")),
    ];

    (block_type, any_text_line())
        .prop_map(|((delim, _kind), content)| format!("{delim}\n{content}\n{delim}\n\n"))
}

/// Generate text with inline macros and formatting.
/// Exercises the two-pass inline processing system with passthrough
/// placeholder replacement and location remapping.
pub fn inline_formatted_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            any_word(),
            any_word().prop_map(|w| format!("*{w}*")),
            any_word().prop_map(|w| format!("_{w}_")),
            any_word().prop_map(|w| format!("`{w}`")),
            any_word().prop_map(|w| format!("#{w}#")),
            any_word().prop_map(|w| format!("~{w}~")),
            any_word().prop_map(|w| format!("^{w}^")),
            any_text_line().prop_map(|t| format!("footnote:[{t}]")),
            Just("kbd:[Ctrl+C]".to_string()),
            Just("btn:[OK]".to_string()),
            any_word().prop_map(|w| format!("menu:{w}[Save]")),
            any_word().prop_map(|w| format!("icon:{w}[]")),
            any_word().prop_map(|w| format!("pass:[{w}]")),
            any_word().prop_map(|w| format!("link:https://example.com[{w}]")),
            any_word().prop_map(|w| format!("(({w}))")),
            any_word().prop_map(|w| format!("((({w})))")),
        ],
        1..=6,
    )
    .prop_map(|parts| {
        let line = parts.join(" ");
        format!("{line}\n")
    })
}

/// Generate realistic multi-construct documents.
/// Tests cross-construct interactions with sections, paragraphs,
/// lists, tables, and delimited blocks composed together.
pub fn rich_document() -> impl Strategy<Value = String> {
    let block = prop_oneof![
        any_text_line().prop_map(|t| format!("{t}\n\n")),
        prop::collection::vec(any_text_line(), 1..=3).prop_map(|items| {
            let mut list = String::new();
            for i in items {
                let _ = writeln!(list, "* {i}");
            }
            list.push('\n');
            list
        }),
        prop::collection::vec(any_text_line(), 1..=3).prop_map(|items| {
            let mut list = String::new();
            for i in items {
                let _ = writeln!(list, ". {i}");
            }
            list.push('\n');
            list
        }),
        any_text_line().prop_map(|t| format!("----\n{t}\n----\n\n")),
        any_text_line().prop_map(|t| format!("____\n{t}\n____\n\n")),
    ];

    let heading_level = prop_oneof![Just("=="), Just("==="), Just("====")];

    let section = (
        heading_level,
        any_text_line(),
        prop::collection::vec(block, 1..=3),
    )
        .prop_map(|(level, title, blocks)| {
            let mut s = format!("{level} {title}\n\n");
            for b in blocks {
                s.push_str(&b);
            }
            s
        });

    (
        prop::bool::ANY,
        any_text_line(),
        prop::collection::vec(section, 1..=3),
    )
        .prop_map(|(has_title, title, sections)| {
            let mut doc = String::new();
            if has_title {
                let _ = write!(doc, "= {title}\n\n");
            }
            for s in sections {
                doc.push_str(&s);
            }
            doc
        })
}
