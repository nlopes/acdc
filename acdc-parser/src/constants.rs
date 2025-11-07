// Default document attributes matching `asciidoctor`'s behavior
//
// These universal attributes apply across all output formats and are
// automatically set when a Document is created. They can be overridden
// by document attributes (e.g., `:note-caption: Custom Note`).
//
// Format-specific attributes (like HTML's `lang`) should be handled
// by individual converters with appropriate fallbacks.

use crate::AttributeValue;

/// Universal default attributes applied to all documents
///
/// These match asciidoctor's default behavior and include:
/// - Admonition captions
/// - Block captions (example, figure, table, appendix)
/// - UI labels (TOC title, version label, etc.)
/// - Reference labels (chapter, section, part, appendix)
/// - Structural settings (TOC levels, section numbering depth)
/// - ID generation settings
/// - Attribute processing compliance settings
pub fn default_attributes() -> Vec<(&'static str, AttributeValue)> {
    vec![
        // Admonition captions
        ("note-caption", AttributeValue::String("Note".into())),
        ("tip-caption", AttributeValue::String("Tip".into())),
        (
            "important-caption",
            AttributeValue::String("Important".into()),
        ),
        ("warning-caption", AttributeValue::String("Warning".into())),
        ("caution-caption", AttributeValue::String("Caution".into())),
        // Block captions
        ("example-caption", AttributeValue::String("Example".into())),
        ("figure-caption", AttributeValue::String("Figure".into())),
        ("table-caption", AttributeValue::String("Table".into())),
        (
            "appendix-caption",
            AttributeValue::String("Appendix".into()),
        ),
        // UI labels
        (
            "toc-title",
            AttributeValue::String("Table of Contents".into()),
        ),
        ("untitled-label", AttributeValue::String("Untitled".into())),
        ("version-label", AttributeValue::String("Version".into())),
        (
            "last-update-label",
            AttributeValue::String("Last updated".into()),
        ),
        // Reference labels
        ("chapter-refsig", AttributeValue::String("Chapter".into())),
        ("section-refsig", AttributeValue::String("Section".into())),
        ("part-refsig", AttributeValue::String("Part".into())),
        ("appendix-refsig", AttributeValue::String("Appendix".into())),
        // Structural settings
        ("toclevels", AttributeValue::String("2".into())),
        ("sectnumlevels", AttributeValue::String("3".into())),
        // ID generation
        ("idprefix", AttributeValue::String("_".into())),
        ("idseparator", AttributeValue::String("_".into())),
        ("sectids", AttributeValue::Bool(true)),
        // Attribute processing compliance
        ("attribute-missing", AttributeValue::String("skip".into())),
        (
            "attribute-undefined",
            AttributeValue::String("drop-line".into()),
        ),
    ]
}
