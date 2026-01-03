//! Manpage-specific parsing utilities for doctype=manpage documents.
//!
//! This module provides functions to derive manpage attributes from document content,
//! following the asciidoctor pattern where these attributes are set during parsing
//! (not conversion) so they're available for attribute substitution in the body.

use crate::{
    AttributeValue, DocumentAttributes, Header, InlineNode,
    error::{Error, Positioning, SourceLocation},
};

/// Parsed manpage title components.
#[derive(Debug, Clone)]
pub struct ManpageTitle {
    /// The program/command name (e.g., "git-commit").
    pub name: String,
    /// The volume number (e.g., "1", "3p", "8").
    pub volume: String,
}

/// Parse a manpage title in the format `name(volume)`.
///
/// Volume must be a digit optionally followed by a letter (e.g., "1", "3p", "8").
///
/// # Arguments
///
/// * `title` - The raw title text (e.g., "git-commit(1)")
///
/// # Returns
///
/// The parsed title components, or None if the format is invalid.
pub fn parse_manpage_title(title: &str) -> Option<ManpageTitle> {
    // Find the last '(' and matching ')'
    let title = title.trim();
    if !title.ends_with(')') {
        return None;
    }

    let open_paren = title.rfind('(')?;
    if open_paren == 0 {
        return None; // No name before the paren
    }

    let name = title[..open_paren].trim();
    if name.is_empty() {
        return None;
    }

    let volume = title[open_paren + 1..title.len() - 1].trim();
    // Validate volume: must be digit optionally followed by a letter
    match volume.chars().collect::<Vec<char>>().as_slice() {
        [first] if first.is_ascii_digit() => {} // valid,
        [first, second] if first.is_ascii_digit() && second.is_ascii_alphabetic() => {}
        _ => {
            tracing::warn!(%title, %volume, "invalid manpage volume format in title");
            return None;
        }
    }

    Some(ManpageTitle {
        name: name.to_string(),
        volume: volume.to_string(),
    })
}

/// Extract plain text from inline nodes (for title parsing).
pub fn extract_plain_text(nodes: &[InlineNode]) -> String {
    let mut result = String::new();
    for node in nodes {
        match node {
            InlineNode::PlainText(text) => result.push_str(&text.content),
            InlineNode::RawText(text) => result.push_str(&text.content),
            InlineNode::VerbatimText(text) => result.push_str(&text.content),
            InlineNode::BoldText(bold) => result.push_str(&extract_plain_text(&bold.content)),
            InlineNode::ItalicText(italic) => result.push_str(&extract_plain_text(&italic.content)),
            InlineNode::MonospaceText(mono) => result.push_str(&extract_plain_text(&mono.content)),
            InlineNode::HighlightText(highlight) => {
                result.push_str(&extract_plain_text(&highlight.content));
            }
            InlineNode::SubscriptText(sub) => result.push_str(&extract_plain_text(&sub.content)),
            InlineNode::SuperscriptText(sup) => result.push_str(&extract_plain_text(&sup.content)),
            InlineNode::CurvedQuotationText(quoted) => {
                result.push_str(&extract_plain_text(&quoted.content));
            }
            InlineNode::CurvedApostropheText(quoted) => {
                result.push_str(&extract_plain_text(&quoted.content));
            }
            // These nodes don't contribute plain text
            InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::CalloutRef(_)
            | InlineNode::Macro(_) => {}
        }
    }
    result
}

/// Sanitize a name for use as mantitle when the title doesn't conform to name(volume) format.
///
/// Transforms the input by:
/// - Converting to lowercase
/// - Replacing non-alphanumeric characters (except `-` and `_`) with hyphens
/// - Collapsing multiple hyphens into one
/// - Trimming leading/trailing hyphens
///
/// Used primarily to sanitize filenames (without extension) for mantitle fallback.
fn sanitize_mantitle(name: &str) -> String {
    let sanitized: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // Collapse multiple hyphens and trim
    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_hyphen && !result.is_empty() {
                result.push(c);
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    result.trim_end_matches('-').to_string()
}

/// Derive manpage attributes from the document header.
///
/// This function should be called during parsing, after the header is parsed but
/// before body blocks are processed. It sets:
/// - `mantitle`: The program name from the document title (lowercase)
/// - `manvolnum`: The volume number from the document title
///
/// These attributes are set using `insert()` which won't overwrite user-provided values.
///
/// When the title doesn't conform to `name(volume)` format:
/// - In strict mode: returns an error
/// - Otherwise: uses fallback values (filename without extension, volume "1")
///
/// # Arguments
///
/// * `header` - The parsed document header (may be None)
/// * `attrs` - Mutable reference to document attributes
/// * `strict` - Whether to fail on non-conforming titles
/// * `source_file` - Optional source filename (used for mantitle fallback)
///
/// # Returns
///
/// `Ok(true)` if manpage attributes were derived,
/// `Ok(false)` if no header was provided,
/// `Err` if strict mode and title doesn't conform
pub fn derive_manpage_header_attrs(
    header: Option<&Header>,
    attrs: &mut DocumentAttributes,
    strict: bool,
    source_file: Option<&std::path::Path>,
) -> Result<bool, Error> {
    let Some(header) = header else {
        return Ok(false);
    };

    let title_text = extract_plain_text(header.title.as_ref());

    if let Some(manpage_title) = parse_manpage_title(&title_text) {
        // Conforming title: use parsed name and volume
        attrs.insert(
            "mantitle".to_string(),
            AttributeValue::String(manpage_title.name.to_lowercase()),
        );
        attrs.insert(
            "manvolnum".to_string(),
            AttributeValue::String(manpage_title.volume),
        );

        tracing::debug!(
            mantitle = manpage_title.name,
            manvolnum = ?attrs.get("manvolnum"),
            "derived manpage attributes from header"
        );
    } else {
        // Non-conforming title
        if strict {
            return Err(Error::NonConformingManpageTitle(
                Box::new(SourceLocation {
                    file: source_file.map(std::path::Path::to_path_buf),
                    positioning: Positioning::Location(header.location.clone()),
                }),
                format!("title '{title_text}' does not match 'name(volume)' format"),
            ));
        }

        // Use fallbacks (matching asciidoctor behavior):
        // - mantitle: filename without extension (or sanitized title if no file)
        // - manvolnum: "1"
        let fallback_name = source_file
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or(&title_text);

        let sanitized = sanitize_mantitle(fallback_name);

        tracing::warn!(
            ?title_text,
            ?source_file,
            mantitle = %sanitized,
            "doctype=manpage but title doesn't match name(volume) format; using filename as fallback"
        );

        attrs.insert("mantitle".to_string(), AttributeValue::String(sanitized));
        attrs.insert(
            "manvolnum".to_string(),
            AttributeValue::String("1".to_string()),
        );

        tracing::debug!(
            mantitle = ?attrs.get("mantitle"),
            manvolnum = "1",
            "using fallback manpage attributes for non-conforming title"
        );
    }

    Ok(true)
}

/// Parse NAME section content to extract manname and manpurpose.
///
/// The NAME section format is: `name - purpose`
///
/// This should be called when the NAME section is encountered during parsing.
///
/// # Arguments
///
/// * `content` - The text content of the NAME section paragraph
/// * `attrs` - Mutable reference to document attributes
///
/// # Returns
///
/// true if manname/manpurpose were derived, false otherwise
pub fn derive_name_section_attrs(content: &str, attrs: &mut DocumentAttributes) -> bool {
    // Split on " - " (with spaces) to get name and purpose
    if let Some(idx) = content.find(" - ") {
        let name = content[..idx].trim();
        let purpose = content[idx + 3..].trim();

        if !name.is_empty() {
            attrs.insert(
                "manname".to_string(),
                AttributeValue::String(name.to_string()),
            );

            if !purpose.is_empty() {
                attrs.insert(
                    "manpurpose".to_string(),
                    AttributeValue::String(purpose.to_string()),
                );
            }

            tracing::debug!(
                manname = name,
                manpurpose = purpose,
                "derived NAME section attributes"
            );
            return true;
        }
    }

    false
}

/// Check if the document has doctype=manpage.
pub fn is_manpage_doctype(attrs: &DocumentAttributes) -> bool {
    attrs
        .get("doctype")
        .is_some_and(|v| matches!(v, AttributeValue::String(s) if s == "manpage"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn test_parse_manpage_title_simple() {
        let title = parse_manpage_title("git(1)").expect("valid manpage title");
        assert_eq!(title.name, "git");
        assert_eq!(title.volume, "1");
    }

    #[test]
    fn test_parse_manpage_title_with_hyphen() {
        let title = parse_manpage_title("git-commit(1)").expect("valid manpage title");
        assert_eq!(title.name, "git-commit");
        assert_eq!(title.volume, "1");
    }

    #[test]
    fn test_parse_manpage_title_with_letter() {
        let title = parse_manpage_title("intro(3p)").expect("valid manpage title");
        assert_eq!(title.name, "intro");
        assert_eq!(title.volume, "3p");
    }

    #[test]
    fn test_parse_manpage_title_volume_5() {
        let title = parse_manpage_title("passwd(5)").expect("valid manpage title");
        assert_eq!(title.name, "passwd");
        assert_eq!(title.volume, "5");
    }

    #[test]
    fn test_parse_manpage_title_invalid() {
        assert!(parse_manpage_title("no-volume").is_none());
        assert!(parse_manpage_title("bad()").is_none());
        assert!(parse_manpage_title("wrong(abc)").is_none());
    }

    #[test]
    fn test_derive_name_section_attrs() {
        let mut attrs = DocumentAttributes::default();
        assert!(derive_name_section_attrs(
            "myprogram - a test program",
            &mut attrs
        ));
        assert_eq!(
            attrs.get("manname"),
            Some(&AttributeValue::String("myprogram".to_string()))
        );
        assert_eq!(
            attrs.get("manpurpose"),
            Some(&AttributeValue::String("a test program".to_string()))
        );
    }

    #[test]
    fn test_derive_name_section_attrs_no_separator() {
        let mut attrs = DocumentAttributes::default();
        assert!(!derive_name_section_attrs("just a name", &mut attrs));
        assert!(attrs.get("manname").is_none());
    }

    #[test]
    fn test_sanitize_mantitle_simple() {
        assert_eq!(sanitize_mantitle("My Document"), "my-document");
    }

    #[test]
    fn test_sanitize_mantitle_with_special_chars() {
        assert_eq!(
            sanitize_mantitle("Upcoming breaking changes"),
            "upcoming-breaking-changes"
        );
    }

    #[test]
    fn test_sanitize_mantitle_collapses_hyphens() {
        assert_eq!(sanitize_mantitle("foo  bar   baz"), "foo-bar-baz");
    }

    #[test]
    fn test_sanitize_mantitle_trims_hyphens() {
        assert_eq!(
            sanitize_mantitle("  Leading and trailing  "),
            "leading-and-trailing"
        );
    }

    #[test]
    fn test_sanitize_mantitle_preserves_underscores() {
        assert_eq!(sanitize_mantitle("my_document_name"), "my_document_name");
    }

    #[test]
    fn test_sanitize_mantitle_mixed_chars() {
        assert_eq!(
            sanitize_mantitle("Git 3.0: Breaking Changes!"),
            "git-3-0-breaking-changes"
        );
    }
}
