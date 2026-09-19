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

/// Default include recursion limit, as the numeric policy and as the attribute
/// value documents see. `preprocessor::tests::max_include_depth_fallback_uses_the_canonical_default`
/// keeps the two spellings in sync.
pub(crate) const DEFAULT_MAX_INCLUDE_DEPTH: usize = 64;
pub(crate) const DEFAULT_MAX_INCLUDE_DEPTH_STR: &str = "64";

/// Name of the `max-include-depth` document attribute. It is an API-only
/// attribute, so document content must not be able to change it.
pub(crate) const MAX_INCLUDE_DEPTH_ATTR: &str = "max-include-depth";

pub(crate) static DEFAULT_MAX_INCLUDE_DEPTH_VALUE: AttributeValue<'static> =
    AttributeValue::String(Cow::Borrowed(DEFAULT_MAX_INCLUDE_DEPTH_STR));

/// How document content is allowed to change a built-in attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentAttributePolicy {
    /// Document content can set or unset the attribute.
    Modifiable,
    /// Only parser options or command-line arguments can set the attribute.
    ApiOnly,
    /// The processor owns the attribute value.
    ReadOnly,
}

/// Return the modification policy for a document attribute name.
///
/// The reserved names come from the `AsciiDoc` document attribute reference and
/// Asciidoctor's processor identity values. Prefix checks cover the backend,
/// base-backend, doctype, filetype, and safe mode convenience attributes that
/// processors generate dynamically.
pub(crate) fn document_attribute_policy(name: &str) -> DocumentAttributePolicy {
    if matches!(
        name,
        "asciidoctor"
            | "asciidoctor-version"
            | "basebackend"
            | "embedded"
            | "htmlsyntax"
            | "outdir"
            | "outfile"
            | "user-home"
    ) || [
        "backend-",
        "basebackend-",
        "doctype-",
        "filetype-",
        "safe-mode-",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
    {
        DocumentAttributePolicy::ReadOnly
    } else if matches!(
        name,
        "allow-uri-read"
            | "docdir"
            | "docfile"
            | "docfilesuffix"
            | "docname"
            | "filetype"
            | "max-attribute-value-size"
            | MAX_INCLUDE_DEPTH_ATTR
    ) {
        DocumentAttributePolicy::ApiOnly
    } else {
        DocumentAttributePolicy::Modifiable
    }
}

/// Whether document entries are prohibited from changing a built-in attribute.
///
/// This name-based policy protects read-only processor values and API-only
/// controls in the header and body, even when the caller did not supply the
/// attribute. `Options::is_document_attribute_locked` combines this policy
/// with caller-supplied attribute locks.
pub(crate) fn is_builtin_attribute_protected(name: &str) -> bool {
    document_attribute_policy(name) != DocumentAttributePolicy::Modifiable
}

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
        // ID generation
        str_attr("idprefix", "_"),
        str_attr("idseparator", "_"),
        (Cow::Borrowed("sectids"), AttributeValue::Bool(true)),
        // Author metadata (overridden when an author line / :author: is present)
        str_attr("authorcount", "0"),
        // Attribute processing compliance
        str_attr("attribute-missing", "skip"),
        str_attr("attribute-undefined", "drop-line"),
    ];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_attribute_fixture_matches_internal_policy() -> Result<(), String> {
        let fixture = include_str!("../fixtures/document_attributes/policy.tsv");

        for (index, line) in fixture.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((policy, name)) = line.split_once('\t') else {
                return Err(format!("invalid policy fixture line {}", index + 1));
            };
            let name = name.replace('*', "probe");
            let expected = match policy {
                "read_only" => DocumentAttributePolicy::ReadOnly,
                "api_only" => DocumentAttributePolicy::ApiOnly,
                "header" | "body" => DocumentAttributePolicy::Modifiable,
                _ => {
                    return Err(format!(
                        "unknown policy {policy} on fixture line {}",
                        index + 1
                    ));
                }
            };

            assert_eq!(
                document_attribute_policy(&name),
                expected,
                "fixture line {} ({name})",
                index + 1
            );
        }
        Ok(())
    }
}
