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
        Self {
            text,
            version,
            ast: Some(ast),
            diagnostics: vec![],
            anchors,
            xrefs,
        }
    }

    /// Create a new document state with parse failure
    #[must_use]
    pub fn new_failure(text: String, version: i32, diagnostics: Vec<Diagnostic>) -> Self {
        // Note: We don't preserve the previous AST since Document doesn't implement Clone.
        // Navigation features will be unavailable until the document parses successfully.
        Self {
            text,
            version,
            ast: None,
            diagnostics,
            anchors: HashMap::new(),
            xrefs: vec![],
        }
    }
}
