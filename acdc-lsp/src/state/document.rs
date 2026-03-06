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
        Self {
            text,
            version,
            ast: Some(ast),
            diagnostics: vec![],
            anchors,
            xrefs,
            includes,
        }
    }

    /// Create a new document state with parse failure
    #[must_use]
    pub fn new_failure(text: String, version: i32, diagnostics: Vec<Diagnostic>) -> Self {
        let includes = extract_includes(&text);
        // Note: We don't preserve the previous AST since Document doesn't implement Clone.
        // Navigation features will be unavailable until the document parses successfully.
        Self {
            text,
            version,
            ast: None,
            diagnostics,
            anchors: HashMap::new(),
            xrefs: vec![],
            includes,
        }
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
}
