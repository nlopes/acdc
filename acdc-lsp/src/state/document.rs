//! Single document state management

use std::collections::HashMap;

use acdc_parser::{Document, Location};
use tower_lsp::lsp_types::Diagnostic;

/// Represents a parsed document's state
#[derive(Debug)]
pub struct DocumentState {
    /// The source text (needed for re-parsing on change)
    pub text: String,
    /// Version from the editor (for sync validation)
    pub version: i32,
    /// Successfully parsed AST (None if parse failed completely)
    pub ast: Option<Document>,
    /// Parse errors converted to diagnostics
    pub diagnostics: Vec<Diagnostic>,
    /// Anchor definitions: id -> Location
    pub anchors: HashMap<String, Location>,
    /// Cross-references: (`target_id`, location)
    pub xrefs: Vec<(String, Location)>,
    /// Include directives: (`target_path`, location)
    pub includes: Vec<(String, Location)>,
    /// Attribute references: (`attr_name`, location) extracted from source text
    pub attribute_refs: Vec<(String, Location)>,
    /// Attribute definitions: (`attr_name`, location) extracted from source text
    pub attribute_defs: Vec<(String, Location)>,
}

impl DocumentState {
    /// Create a new document state with successful parse
    #[must_use]
    pub fn new_success(
        text: String,
        version: i32,
        ast: Document,
        anchors: HashMap<String, Location>,
        xrefs: Vec<(String, Location)>,
    ) -> Self {
        let includes = extract_includes(&text);
        let attribute_refs = extract_attribute_refs(&text);
        Self {
            attribute_defs: extract_attribute_defs(&text),
            text,
            version,
            ast: Some(ast),
            diagnostics: vec![],
            anchors,
            xrefs,
            includes,
            attribute_refs,
        }
    }

    /// Create a new document state with parse failure
    #[must_use]
    pub fn new_failure(text: String, version: i32, diagnostics: Vec<Diagnostic>) -> Self {
        let includes = extract_includes(&text);
        let attribute_refs = extract_attribute_refs(&text);
        // Note: We don't preserve the previous AST since Document doesn't implement Clone.
        // Navigation features will be unavailable until the document parses successfully.
        Self {
            attribute_defs: extract_attribute_defs(&text),
            text,
            version,
            ast: None,
            diagnostics,
            anchors: HashMap::new(),
            xrefs: vec![],
            includes,
            attribute_refs,
        }
    }
}

/// Extract attribute definitions (`:name: value`) from raw text.
fn extract_attribute_defs(text: &str) -> Vec<(String, Location)> {
    let mut defs = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        // Match :name: or :!name: (unset)
        let after_colon = if let Some(rest) = trimmed.strip_prefix(":!") {
            rest
        } else if let Some(rest) = trimmed.strip_prefix(':') {
            rest
        } else {
            continue;
        };

        let Some(end) = after_colon.find(':') else {
            continue;
        };
        if let Some(name_candidate) = after_colon.get(..end)
            && !name_candidate.is_empty()
            && name_candidate
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            let col_offset = line.find(':').unwrap_or(0);
            let line_end = line.len();

            let line_start: usize = text.lines().take(line_idx).map(|l| l.len() + 1).sum();

            let mut location = Location::default();
            location.start.line = line_idx + 1;
            location.start.column = col_offset + 1;
            location.end.line = line_idx + 1;
            location.end.column = line_end;
            location.absolute_start = line_start + col_offset;
            location.absolute_end = line_start + line_end;

            defs.push((name_candidate.to_string(), location));
        }
    }

    defs
}

/// Extract attribute references (`{name}`) from raw text.
///
/// Scans for `{name}` patterns, skipping escaped references (`\{name}`)
/// and attribute definition lines (`:name:`).
fn extract_attribute_refs(text: &str) -> Vec<(String, Location)> {
    let mut refs = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        // Check if this is an attribute definition: :name: value
        if let Some(after_colon) = trimmed.strip_prefix(':')
            && let Some(end) = after_colon.find(':')
            && let Some(name_candidate) = after_colon.get(..end)
            && !name_candidate.is_empty()
            && name_candidate
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            // Attribute definition line — scan only the value part for refs
            if let Some(value_part) = after_colon.get(end + 1..) {
                extract_refs_from_line(text, line, line_idx, value_part, &mut refs);
            }
            continue;
        }
        extract_refs_from_line(text, line, line_idx, line, &mut refs);
    }

    refs
}

/// Extract `{name}` references from a text segment within a line.
fn extract_refs_from_line(
    full_text: &str,
    line: &str,
    line_idx: usize,
    segment: &str,
    refs: &mut Vec<(String, Location)>,
) {
    // segment is always a substring of line (either line itself or a suffix)
    let segment_offset_in_line = segment.as_ptr() as usize - line.as_ptr() as usize;

    let mut search_start = 0;
    while let Some(open) = segment.get(search_start..).and_then(|s| s.find('{')) {
        let open = search_start + open;

        // Check for escape: \{
        if open > 0 && segment.as_bytes().get(open - 1) == Some(&b'\\') {
            search_start = open + 1;
            continue;
        }

        let Some(close) = segment.get(open + 1..).and_then(|s| s.find('}')) else {
            break;
        };
        let close = open + 1 + close;

        if let Some(name) = segment.get(open + 1..close)
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            let col_in_line = segment_offset_in_line + open;
            let col_end = segment_offset_in_line + close + 1;

            let line_start: usize = full_text
                .lines()
                .take(line_idx)
                .map(|l| l.len() + 1) // +1 for newline
                .sum();

            let mut location = Location::default();
            location.start.line = line_idx + 1;
            location.start.column = col_in_line + 1;
            location.end.line = line_idx + 1;
            location.end.column = col_end;
            location.absolute_start = line_start + col_in_line;
            location.absolute_end = line_start + col_end;

            refs.push((name.to_string(), location));
        }

        search_start = close + 1;
    }
}

/// Extract include directives from raw text via line-by-line scan.
///
/// The preprocessor consumes `include::` directives so they don't appear in the AST.
/// We scan the raw text to find them for document link support.
fn extract_includes(text: &str) -> Vec<(String, Location)> {
    let mut includes = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("include::")
            && let Some(bracket_pos) = rest.find('[')
        {
            let target = &rest[..bracket_pos];
            if !target.is_empty() {
                // Find the column offset of the include directive in the original line
                let col_offset = line.find("include::").unwrap_or(0);
                let target_start = col_offset + "include::".len();
                let target_end = target_start + target.len();

                let mut location = Location::default();
                // Location uses 1-indexed lines, 1-indexed columns
                location.start.line = line_idx + 1;
                location.start.column = target_start + 1;
                location.end.line = line_idx + 1;
                location.end.column = target_end;

                // Calculate absolute positions
                let line_start: usize = text
                    .lines()
                    .take(line_idx)
                    .map(|l| l.len() + 1) // +1 for newline
                    .sum();
                location.absolute_start = line_start + target_start;
                location.absolute_end = line_start + target_end;

                includes.push((target.to_string(), location));
            }
        }
    }

    includes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_includes_basic() {
        let text = "= Document\n\ninclude::chapter1.adoc[]\n\nSome text.\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 1);
        assert_eq!(
            includes.first().map(|(t, _)| t.as_str()),
            Some("chapter1.adoc")
        );
    }

    #[test]
    fn test_extract_includes_with_attributes() {
        let text = "include::partial.adoc[leveloffset=+1]\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 1);
        assert_eq!(
            includes.first().map(|(t, _)| t.as_str()),
            Some("partial.adoc")
        );
    }

    #[test]
    fn test_extract_includes_multiple() {
        let text = "include::a.adoc[]\nSome text\ninclude::b.adoc[]\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 2);
        assert_eq!(includes.first().map(|(t, _)| t.as_str()), Some("a.adoc"));
        assert_eq!(includes.get(1).map(|(t, _)| t.as_str()), Some("b.adoc"));
    }

    #[test]
    fn test_extract_includes_with_path() {
        let text = "include::docs/chapters/intro.adoc[]\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 1);
        assert_eq!(
            includes.first().map(|(t, _)| t.as_str()),
            Some("docs/chapters/intro.adoc")
        );
    }

    #[test]
    fn test_extract_includes_no_includes() {
        let text = "= Document\n\nJust regular text.\n";
        let includes = extract_includes(text);
        assert!(includes.is_empty());
    }

    #[test]
    fn test_extract_includes_location() -> Result<(), Box<dyn std::error::Error>> {
        let text = "= Doc\n\ninclude::file.adoc[]\n";
        let includes = extract_includes(text);
        assert_eq!(includes.len(), 1);
        let (_, loc) = includes.first().ok_or("expected at least one include")?;
        // Line 3 (index 2), 1-indexed = 3
        assert_eq!(loc.start.line, 3);
        Ok(())
    }

    #[test]
    fn test_extract_attribute_refs_basic() {
        let text = "== Section\n\nSee {my-attr} here.\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("my-attr"));
    }

    #[test]
    fn test_extract_attribute_refs_multiple_on_same_line() {
        let text = "The {foo} and {bar} values.\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("foo"));
        assert_eq!(refs.get(1).map(|(n, _)| n.as_str()), Some("bar"));
    }

    #[test]
    fn test_extract_attribute_refs_escaped() {
        let text = "Not a ref: \\{escaped} but {real} is.\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("real"));
    }

    #[test]
    fn test_extract_attribute_refs_skips_definition_name() {
        let text = ":my-attr: some value\n\n{my-attr} is used here.\n";
        let refs = extract_attribute_refs(text);
        // Should find the ref on line 3, not the definition on line 1
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("my-attr"));
        assert_eq!(refs.first().map(|(_, l)| l.start.line), Some(3));
    }

    #[test]
    fn test_extract_attribute_refs_in_definition_value() {
        let text = ":derived: prefix-{base}\n";
        let refs = extract_attribute_refs(text);
        // Should find {base} in the value part of the definition
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("base"));
    }

    #[test]
    fn test_extract_attribute_refs_location() {
        let text = "= Doc\n\n{version} is the version.\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("version"));
        assert_eq!(refs.first().map(|(_, l)| l.start.line), Some(3));
        assert_eq!(refs.first().map(|(_, l)| l.start.column), Some(1));
    }

    #[test]
    fn test_extract_attribute_refs_ignores_invalid_names() {
        let text = "{} and {with spaces} and {valid-name}\n";
        let refs = extract_attribute_refs(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs.first().map(|(n, _)| n.as_str()), Some("valid-name"));
    }
}
