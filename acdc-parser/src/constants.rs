// Default document attributes matching `asciidoctor`'s behavior
//
// These universal attributes apply across all output formats and are
// automatically set when a Document is created. They can be overridden
// by document attributes (e.g., `:note-caption: Custom Note`).
//
// Format-specific attributes (like HTML's `lang`) should be handled
// by individual converters with appropriate fallbacks.

use crate::{AttributeName, AttributeValue};

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
#[allow(clippy::too_many_lines)]
pub fn default_attributes() -> rustc_hash::FxHashMap<AttributeName, AttributeValue> {
    [
        (
            AttributeName::from("appendix-caption"),
            AttributeValue::String("Appendix".into()),
        ),
        (
            AttributeName::from("appendix-refsig"),
            AttributeValue::String("Appendix".into()),
        ),
        // Admonition captions
        (
            AttributeName::from("note-caption"),
            AttributeValue::String("Note".into()),
        ),
        (
            AttributeName::from("tip-caption"),
            AttributeValue::String("Tip".into()),
        ),
        (
            AttributeName::from("important-caption"),
            AttributeValue::String("Important".into()),
        ),
        (
            AttributeName::from("warning-caption"),
            AttributeValue::String("Warning".into()),
        ),
        (
            AttributeName::from("caution-caption"),
            AttributeValue::String("Caution".into()),
        ),
        // Block captions
        (
            AttributeName::from("example-caption"),
            AttributeValue::String("Example".into()),
        ),
        (
            AttributeName::from("figure-caption"),
            AttributeValue::String("Figure".into()),
        ),
        (
            AttributeName::from("table-caption"),
            AttributeValue::String("Table".into()),
        ),
        // UI labels
        (
            AttributeName::from("toc-title"),
            AttributeValue::String("Table of Contents".into()),
        ),
        (
            AttributeName::from("untitled-label"),
            AttributeValue::String("Untitled".into()),
        ),
        (
            AttributeName::from("version-label"),
            AttributeValue::String("Version".into()),
        ),
        (
            AttributeName::from("last-update-label"),
            AttributeValue::String("Last updated".into()),
        ),
        // Reference labels
        (
            AttributeName::from("chapter-refsig"),
            AttributeValue::String("Chapter".into()),
        ),
        (
            AttributeName::from("section-refsig"),
            AttributeValue::String("Section".into()),
        ),
        (
            AttributeName::from("part-refsig"),
            AttributeValue::String("Part".into()),
        ),
        (
            AttributeName::from("appendix-refsig"),
            AttributeValue::String("Appendix".into()),
        ),
        // Structural settings
        (
            AttributeName::from("toclevels"),
            AttributeValue::String("2".into()),
        ),
        (
            AttributeName::from("sectnumlevels"),
            AttributeValue::String("3".into()),
        ),
        // ID generation
        (
            AttributeName::from("idprefix"),
            AttributeValue::String("_".into()),
        ),
        (
            AttributeName::from("idseparator"),
            AttributeValue::String("_".into()),
        ),
        (AttributeName::from("sectids"), AttributeValue::Bool(true)),
        // Attribute processing compliance
        (
            AttributeName::from("attribute-missing"),
            AttributeValue::String("skip".into()),
        ),
        (
            AttributeName::from("attribute-undefined"),
            AttributeValue::String("drop-line".into()),
        ),
    ]
    .into_iter()
    .collect()
}
