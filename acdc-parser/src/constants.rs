// Default document attributes matching `asciidoctor`'s behavior
//
// These universal attributes apply across all output formats and are
// automatically set when a Document is created. They can be overridden
// by document attributes (e.g., `:note-caption: Custom Note`).
//
// Format-specific attributes (like HTML's `lang`) should be handled
// by individual converters with appropriate fallbacks.

use std::borrow::Cow;

use crate::{AttributeName, AttributeValue};

const fn str_attr(
    name: &'static str,
    value: &'static str,
) -> (AttributeName<'static>, AttributeValue<'static>) {
    (
        Cow::Borrowed(name),
        AttributeValue::String(Cow::Borrowed(value)),
    )
}

/// Universal default attribute entries applied to all documents.
///
/// Exposed as a raw `const` slice so the map type (`FxHashMap`, currently)
/// does not leak out of this module. Callers that need a map build one
/// from these entries in whatever storage they use internally — see
/// `AttributeMap::default()` for the cached-and-cloned fast path.
///
/// Includes:
/// - Character replacement / intrinsic attributes (empty, sp, nbsp, etc.)
/// - Admonition captions
/// - Block captions (example, figure, table, appendix)
/// - UI labels (TOC title, version label, etc.)
/// - Reference labels (chapter, section, part, appendix)
/// - Structural settings (TOC levels, section numbering depth)
/// - ID generation settings
/// - Attribute processing compliance settings
pub(crate) const DEFAULT_ATTRIBUTE_ENTRIES: &[(AttributeName<'static>, AttributeValue<'static>)] =
    &[
        // Character replacement / intrinsic attributes
        str_attr("empty", ""),
        str_attr("blank", ""),
        str_attr("sp", " "),
        str_attr("nbsp", "\u{00A0}"),
        str_attr("zwsp", "\u{200B}"),
        str_attr("wj", "\u{2060}"),
        str_attr("apos", "&#39;"),
        str_attr("quot", "&#34;"),
        str_attr("lsquo", "\u{2018}"),
        str_attr("rsquo", "\u{2019}"),
        str_attr("ldquo", "\u{201C}"),
        str_attr("rdquo", "\u{201D}"),
        str_attr("deg", "\u{00B0}"),
        str_attr("plus", "+"),
        str_attr("brvbar", "\u{00A6}"),
        str_attr("vbar", "|"),
        str_attr("amp", "&"),
        str_attr("lt", "<"),
        str_attr("gt", ">"),
        str_attr("startsb", "["),
        str_attr("endsb", "]"),
        str_attr("caret", "^"),
        str_attr("asterisk", "*"),
        str_attr("tilde", "~"),
        str_attr("backslash", "\\"),
        str_attr("backtick", "`"),
        str_attr("two-colons", "::"),
        str_attr("two-semicolons", ";;"),
        str_attr("cpp", "C++"),
        str_attr("cxx", "C++"),
        str_attr("pp", "++"),
        // Appendix
        str_attr("appendix-caption", "Appendix"),
        str_attr("appendix-refsig", "Appendix"),
        // Admonition captions
        str_attr("note-caption", "Note"),
        str_attr("tip-caption", "Tip"),
        str_attr("important-caption", "Important"),
        str_attr("warning-caption", "Warning"),
        str_attr("caution-caption", "Caution"),
        // Block captions
        str_attr("example-caption", "Example"),
        str_attr("figure-caption", "Figure"),
        str_attr("table-caption", "Table"),
        // UI labels
        str_attr("toc-title", "Table of Contents"),
        str_attr("untitled-label", "Untitled"),
        str_attr("version-label", "Version"),
        str_attr("last-update-label", "Last updated"),
        // Reference labels
        str_attr("chapter-refsig", "Chapter"),
        str_attr("section-refsig", "Section"),
        str_attr("part-refsig", "Part"),
        // Structural settings
        str_attr("toclevels", "2"),
        str_attr("sectnumlevels", "3"),
        // ID generation
        str_attr("idprefix", "_"),
        str_attr("idseparator", "_"),
        (Cow::Borrowed("sectids"), AttributeValue::Bool(true)),
        // Attribute processing compliance
        str_attr("attribute-missing", "skip"),
        str_attr("attribute-undefined", "drop-line"),
    ];
