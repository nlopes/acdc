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
/// - Character replacement / intrinsic attributes (empty, sp, nbsp, etc.)
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
        // Character replacement / intrinsic attributes
        (
            AttributeName::from("empty"),
            AttributeValue::String(String::new()),
        ),
        (
            AttributeName::from("blank"),
            AttributeValue::String(String::new()),
        ),
        (
            AttributeName::from("sp"),
            AttributeValue::String(" ".into()),
        ),
        (
            AttributeName::from("nbsp"),
            AttributeValue::String("\u{00A0}".into()),
        ),
        (
            AttributeName::from("zwsp"),
            AttributeValue::String("\u{200B}".into()),
        ),
        (
            AttributeName::from("wj"),
            AttributeValue::String("\u{2060}".into()),
        ),
        (
            AttributeName::from("apos"),
            AttributeValue::String("'".into()),
        ),
        (
            AttributeName::from("quot"),
            AttributeValue::String("\"".into()),
        ),
        (
            AttributeName::from("lsquo"),
            AttributeValue::String("\u{2018}".into()),
        ),
        (
            AttributeName::from("rsquo"),
            AttributeValue::String("\u{2019}".into()),
        ),
        (
            AttributeName::from("ldquo"),
            AttributeValue::String("\u{201C}".into()),
        ),
        (
            AttributeName::from("rdquo"),
            AttributeValue::String("\u{201D}".into()),
        ),
        (
            AttributeName::from("deg"),
            AttributeValue::String("\u{00B0}".into()),
        ),
        (
            AttributeName::from("plus"),
            AttributeValue::String("+".into()),
        ),
        (
            AttributeName::from("brvbar"),
            AttributeValue::String("\u{00A6}".into()),
        ),
        (
            AttributeName::from("vbar"),
            AttributeValue::String("|".into()),
        ),
        (
            AttributeName::from("amp"),
            AttributeValue::String("&".into()),
        ),
        (
            AttributeName::from("lt"),
            AttributeValue::String("<".into()),
        ),
        (
            AttributeName::from("gt"),
            AttributeValue::String(">".into()),
        ),
        (
            AttributeName::from("startsb"),
            AttributeValue::String("[".into()),
        ),
        (
            AttributeName::from("endsb"),
            AttributeValue::String("]".into()),
        ),
        (
            AttributeName::from("caret"),
            AttributeValue::String("^".into()),
        ),
        (
            AttributeName::from("asterisk"),
            AttributeValue::String("*".into()),
        ),
        (
            AttributeName::from("tilde"),
            AttributeValue::String("~".into()),
        ),
        (
            AttributeName::from("backslash"),
            AttributeValue::String("\\".into()),
        ),
        (
            AttributeName::from("backtick"),
            AttributeValue::String("`".into()),
        ),
        (
            AttributeName::from("two-colons"),
            AttributeValue::String("::".into()),
        ),
        (
            AttributeName::from("two-semicolons"),
            AttributeValue::String(";;".into()),
        ),
        (
            AttributeName::from("cpp"),
            AttributeValue::String("C++".into()),
        ),
        (
            AttributeName::from("cxx"),
            AttributeValue::String("C++".into()),
        ),
        (
            AttributeName::from("pp"),
            AttributeValue::String("++".into()),
        ),
        // Appendix
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
