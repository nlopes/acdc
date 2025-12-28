//! Workspace-level state management

use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use tower_lsp::lsp_types::Url;

use crate::capabilities::{definition, diagnostics};
use crate::state::DocumentState;

/// Workspace-level state management
pub struct Workspace {
    /// Open documents: URI -> `DocumentState`
    documents: DashMap<Url, DocumentState>,
}

impl Workspace {
    /// Create a new workspace
    #[must_use]
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
        }
    }

    /// Update document on open/change
    pub fn update_document(&self, uri: Url, text: String, version: i32) {
        let state = Self::parse_and_index(text, version);
        self.documents.insert(uri, state);
    }

    /// Get a reference to a document's state
    #[must_use]
    pub fn get_document(&self, uri: &Url) -> Option<Ref<'_, Url, DocumentState>> {
        self.documents.get(uri)
    }

    /// Remove a document from the workspace
    pub fn remove_document(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// Parse document and extract all navigation data
    fn parse_and_index(text: String, version: i32) -> DocumentState {
        let options = acdc_parser::Options::default();
        let result = acdc_parser::parse(&text, &options);

        match result {
            Ok(doc) => {
                let anchors = definition::collect_anchors(&doc);
                let xrefs = definition::collect_xrefs(&doc);

                // Compute validation warnings (unresolved xrefs, etc.)
                let warnings = diagnostics::compute_warnings(&anchors, &xrefs);

                let mut state = DocumentState::new_success(text, version, doc, anchors, xrefs);
                state.diagnostics = warnings;
                state
            }
            Err(error) => {
                let diags = vec![diagnostics::error_to_diagnostic(&error)];
                DocumentState::new_failure(text, version, diags)
            }
        }
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}
