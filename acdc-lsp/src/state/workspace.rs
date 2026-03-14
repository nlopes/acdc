//! Workspace-level state management

use std::sync::RwLock;

use acdc_parser::Location;
use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use tower_lsp::lsp_types::Url;

use crate::capabilities::{
    definition, diagnostics,
    workspace_symbols::{IndexedSymbol, extract_workspace_symbols},
};
use crate::state::DocumentState;

/// Workspace-level state management
pub struct Workspace {
    /// Open documents: URI -> `DocumentState`
    documents: DashMap<Url, DocumentState>,
    /// Global anchor index: `anchor_id` -> [(`file_uri`, location)]
    anchor_index: DashMap<String, Vec<(Url, Location)>>,
    /// Workspace root directories
    roots: RwLock<Vec<Url>>,
    /// Cached symbols for non-open files (populated by workspace scan)
    symbol_index: DashMap<Url, Vec<IndexedSymbol>>,
}

impl Workspace {
    /// Create a new workspace
    #[must_use]
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
            anchor_index: DashMap::new(),
            roots: RwLock::new(Vec::new()),
            symbol_index: DashMap::new(),
        }
    }

    /// Set workspace root directories (from initialize params)
    pub fn set_workspace_roots(&self, roots: Vec<Url>) {
        if let Ok(mut w) = self.roots.write() {
            *w = roots;
        }
    }

    /// Update document on open/change
    pub fn update_document(&self, uri: Url, text: String, version: i32) {
        // Remove from symbol_index — live AST takes over
        self.symbol_index.remove(&uri);

        // Remove old anchors for this URI from the global index
        self.remove_anchors_for_uri(&uri);

        let mut state = Self::parse_and_index(text, version);

        // Insert new anchors into the global index
        for (id, loc) in &state.anchors {
            self.anchor_index
                .entry(id.clone())
                .or_default()
                .push((uri.clone(), loc.clone()));
        }

        // Compute diagnostics with workspace context for cross-file xref validation
        let cross_file_resolver = |parsed: &crate::state::XrefTarget| -> bool {
            // Check global anchor index first (open documents)
            if let Some(anchor_id) = &parsed.anchor
                && self.anchor_index.contains_key(anchor_id.as_str())
            {
                return true;
            }
            // Try resolving the file and checking on disk
            if let Some(file_path) = &parsed.file
                && let Some(target_uri) = self.resolve_xref_file(&uri, file_path)
            {
                if let Some(anchor_id) = &parsed.anchor {
                    return Self::find_anchor_in_file_on_disk(&target_uri, anchor_id).is_some();
                }
                // File-only reference (no anchor) — just check file exists
                return target_uri.to_file_path().ok().is_some_and(|p| p.exists());
            }
            false
        };
        state.diagnostics =
            diagnostics::compute_warnings(&state.anchors, &state.xrefs, Some(&cross_file_resolver));

        self.documents.insert(uri, state);
    }

    /// Get a reference to a document's state
    #[must_use]
    pub fn get_document(&self, uri: &Url) -> Option<Ref<'_, Url, DocumentState>> {
        self.documents.get(uri)
    }

    /// Remove a document from the workspace
    pub fn remove_document(&self, uri: &Url) {
        self.remove_anchors_for_uri(uri);
        self.documents.remove(uri);
        // Re-index from disk for workspace symbols
        self.reindex_file_from_disk(uri);
    }

    /// Resolve a relative file path from a referring document's URI
    #[must_use]
    pub fn resolve_xref_file(&self, from_uri: &Url, relative_path: &str) -> Option<Url> {
        // Get the directory of the referring document
        let mut base = from_uri.clone();
        // Remove the file name to get the directory
        {
            let mut segments = base.path_segments_mut().ok()?;
            segments.pop();
            // Add empty segment to ensure trailing slash for correct join behavior
            segments.push("");
        }
        base.join(relative_path).ok()
    }

    /// Find an anchor across all open documents
    #[must_use]
    pub fn find_anchor_globally(&self, anchor_id: &str) -> Vec<(Url, Location)> {
        self.anchor_index
            .get(anchor_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default()
    }

    /// Find an anchor in a specific document (open or on-disk)
    ///
    /// First checks open documents. If the document isn't open, attempts to read
    /// and parse the file from disk to resolve the anchor.
    #[must_use]
    pub fn find_anchor_in_document(&self, uri: &Url, anchor_id: &str) -> Option<Location> {
        // Check open documents first
        if let Some(loc) = self
            .documents
            .get(uri)
            .and_then(|doc| doc.anchors.get(anchor_id).cloned())
        {
            return Some(loc);
        }

        // Try reading from disk if not open
        Self::find_anchor_in_file_on_disk(uri, anchor_id)
    }

    /// Check if a file on disk contains a given anchor (without full indexing)
    #[must_use]
    pub fn file_on_disk_has_anchor(uri: &Url, anchor_id: &str) -> bool {
        Self::find_anchor_in_file_on_disk(uri, anchor_id).is_some()
    }

    /// Read a file from disk and search for an anchor without indexing it
    fn find_anchor_in_file_on_disk(uri: &Url, anchor_id: &str) -> Option<Location> {
        let path = uri.to_file_path().ok()?;
        tracing::info!(?path, anchor_id, "reading file from disk for anchor lookup");
        let text = std::fs::read_to_string(&path).ok()?;
        let doc = acdc_parser::parse(&text, &acdc_parser::Options::default()).ok()?;
        let anchors = definition::collect_anchors(&doc);
        let result = anchors.get(anchor_id).cloned();
        tracing::info!(
            ?path,
            anchor_id,
            found = result.is_some(),
            all_anchors = ?anchors.keys().collect::<Vec<_>>(),
            "disk anchor lookup result"
        );
        result
    }

    /// Get all anchors across all open documents (for completion)
    #[must_use]
    pub fn all_anchors(&self) -> Vec<(String, Url)> {
        let mut result = Vec::new();
        for entry in &self.anchor_index {
            for (uri, _loc) in entry.value() {
                result.push((entry.key().clone(), uri.clone()));
            }
        }
        result
    }

    /// Scan workspace roots for `AsciiDoc` files and populate the symbol index.
    pub fn scan_workspace_files(&self) {
        let roots: Vec<Url> = self.roots.read().map(|r| r.clone()).unwrap_or_default();
        let files = discover_adoc_files(&roots);

        for path in files {
            let Ok(uri) = Url::from_file_path(&path) else {
                continue;
            };
            // Skip files that are already open in the editor
            if self.documents.contains_key(&uri) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(doc) = acdc_parser::parse(&text, &acdc_parser::Options::default())
            {
                let symbols = extract_workspace_symbols(&doc);
                self.symbol_index.insert(uri, symbols);
            }
        }
    }

    /// Check if a URI has cached symbols in the index
    #[must_use]
    pub fn has_indexed_symbols(&self, uri: &Url) -> bool {
        self.symbol_index.contains_key(uri)
    }

    /// Number of files in the symbol index
    #[must_use]
    pub fn symbol_index_len(&self) -> usize {
        self.symbol_index.len()
    }

    /// Insert symbols for a URI into the index (for testing and manual insertion)
    pub fn insert_indexed_symbols(&self, uri: Url, symbols: Vec<IndexedSymbol>) {
        self.symbol_index.insert(uri, symbols);
    }

    /// Query workspace symbols across all documents (open + indexed).
    ///
    /// Returns `(Url, IndexedSymbol)` pairs matching the query. Empty query
    /// returns all symbols. Matching is case-insensitive substring.
    #[must_use]
    pub fn query_workspace_symbols(&self, query: &str) -> Vec<(Url, IndexedSymbol)> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Symbols from open documents (live AST)
        for entry in &self.documents {
            let uri = entry.key();
            if let Some(ast) = &entry.value().ast {
                let symbols = extract_workspace_symbols(ast);
                for symbol in symbols {
                    if query.is_empty() || symbol.name.to_lowercase().contains(&query_lower) {
                        results.push((uri.clone(), symbol));
                    }
                }
            }
        }

        // Symbols from indexed (non-open) files
        for entry in &self.symbol_index {
            let uri = entry.key();
            for symbol in entry.value() {
                if query.is_empty() || symbol.name.to_lowercase().contains(&query_lower) {
                    results.push((uri.clone(), symbol.clone()));
                }
            }
        }

        results
    }

    fn reindex_file_from_disk(&self, uri: &Url) {
        if let Ok(path) = uri.to_file_path()
            && let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(doc) = acdc_parser::parse(&text, &acdc_parser::Options::default())
        {
            let symbols = extract_workspace_symbols(&doc);
            self.symbol_index.insert(uri.clone(), symbols);
        }
    }

    /// Iterate over all open documents
    pub fn for_each_document<F>(&self, mut f: F)
    where
        F: FnMut(&Url, &DocumentState),
    {
        for entry in &self.documents {
            f(entry.key(), entry.value());
        }
    }

    /// Remove all anchor entries for a given URI from the global index
    fn remove_anchors_for_uri(&self, uri: &Url) {
        // Collect keys to remove empty entries afterward
        let mut empty_keys = Vec::new();

        for mut entry in self.anchor_index.iter_mut() {
            entry.value_mut().retain(|(u, _)| u != uri);
            if entry.value().is_empty() {
                empty_keys.push(entry.key().clone());
            }
        }

        for key in empty_keys {
            // Only remove if still empty (no race with concurrent insert)
            self.anchor_index.remove_if(&key, |_, v| v.is_empty());
        }
    }

    /// Parse document and extract all navigation data
    fn parse_and_index(text: String, version: i32) -> DocumentState {
        let options = acdc_parser::Options::default();
        let result = acdc_parser::parse(&text, &options);

        match result {
            Ok(doc) => {
                let anchors = definition::collect_anchors(&doc);
                let xrefs = definition::collect_xrefs(&doc);

                // Warnings are computed later in update_document with workspace context
                DocumentState::new_success(text, version, doc, anchors, xrefs)
            }
            Err(error) => {
                let diags = vec![diagnostics::error_to_diagnostic(&error)];
                DocumentState::new_failure(text, version, diags)
            }
        }
    }
}

/// Directories to skip during file discovery
const SKIP_DIRS: &[&str] = &[".git", ".svn", ".hg", "target", "node_modules", ".build"];

/// File extensions recognized as `AsciiDoc`
const ADOC_EXTENSIONS: &[&str] = &["adoc", "asciidoc", "asc"];

/// Discover all `AsciiDoc` files under the given workspace roots.
///
/// Walks directories recursively using `std::fs`, skipping hidden directories
/// and common build/dependency directories.
pub fn discover_adoc_files(roots: &[Url]) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        if let Ok(path) = root.to_file_path() {
            walk_directory(&path, &mut files);
        }
    }
    files
}

fn walk_directory(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') || SKIP_DIRS.contains(&name_str.as_ref()) {
                continue;
            }
            walk_directory(&path, files);
        } else if let Some(ext) = path.extension()
            && ADOC_EXTENSIONS.contains(&ext.to_string_lossy().as_ref())
        {
            files.push(path);
        }
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_index_updated_on_document_change() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;

        let content = "[[my-anchor]]\n== My Section\n\nSome content.\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        let anchors = workspace.find_anchor_globally("my-anchor");
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors.first().map(|(u, _)| u), Some(&uri));
        Ok(())
    }

    #[test]
    fn test_anchor_index_cleaned_on_document_remove() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;

        let content = "[[my-anchor]]\n== My Section\n\nContent.\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        assert!(!workspace.find_anchor_globally("my-anchor").is_empty());

        workspace.remove_document(&uri);

        assert!(workspace.find_anchor_globally("my-anchor").is_empty());
        Ok(())
    }

    #[test]
    fn test_anchor_index_updated_on_document_reparse() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;

        let content1 = "[[old-anchor]]\n== Old Section\n\nContent.\n";
        workspace.update_document(uri.clone(), content1.to_string(), 1);
        assert!(!workspace.find_anchor_globally("old-anchor").is_empty());

        let content2 = "[[new-anchor]]\n== New Section\n\nContent.\n";
        workspace.update_document(uri.clone(), content2.to_string(), 2);

        assert!(workspace.find_anchor_globally("old-anchor").is_empty());
        assert!(!workspace.find_anchor_globally("new-anchor").is_empty());
        Ok(())
    }

    #[test]
    fn test_resolve_xref_file() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let from_uri = Url::parse("file:///docs/chapter1.adoc")?;

        let resolved = workspace.resolve_xref_file(&from_uri, "chapter2.adoc");
        assert!(resolved.is_some());
        assert_eq!(
            resolved.map(|u| u.to_string()),
            Some("file:///docs/chapter2.adoc".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_all_anchors_across_documents() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri1 = Url::parse("file:///doc1.adoc")?;
        let uri2 = Url::parse("file:///doc2.adoc")?;

        workspace.update_document(
            uri1,
            "[[anchor1]]\n== Section 1\n\nContent.\n".to_string(),
            1,
        );
        workspace.update_document(
            uri2,
            "[[anchor2]]\n== Section 2\n\nContent.\n".to_string(),
            1,
        );

        let all = workspace.all_anchors();
        let ids: Vec<&str> = all.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"anchor1"));
        assert!(ids.contains(&"anchor2"));
        Ok(())
    }

    #[test]
    fn test_workspace_symbol_query() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri1 = Url::parse("file:///doc1.adoc")?;
        let uri2 = Url::parse("file:///doc2.adoc")?;

        // Open doc1 (live AST)
        workspace.update_document(
            uri1.clone(),
            "= Doc One\n\n== Introduction\n\nContent.\n".to_string(),
            1,
        );

        // Manually add doc2 to symbol_index (simulates scanned file)
        let doc2 = acdc_parser::parse(
            "= Doc Two\n\n== TLS Configuration\n\nStuff.\n",
            &acdc_parser::Options::default(),
        )?;
        let symbols = extract_workspace_symbols(&doc2);
        workspace.insert_indexed_symbols(uri2.clone(), symbols);

        // Query for "tls" (case-insensitive)
        let results = workspace.query_workspace_symbols("tls");
        assert!(results.iter().any(|(_, s)| s.name == "TLS Configuration"));

        // Query for "introduction"
        let results = workspace.query_workspace_symbols("introduction");
        assert!(results.iter().any(|(_, s)| s.name == "Introduction"));

        // Empty query returns all
        let results = workspace.query_workspace_symbols("");
        assert!(results.len() >= 4); // at least: 2 doc titles + 2 sections

        Ok(())
    }

    #[test]
    fn test_symbol_index_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        let tmp = std::env::temp_dir().join("acdc_lsp_test_symbol_idx");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp)?;
        fs::write(
            tmp.join("doc.adoc"),
            "= My Document\n\n== First Section\n\nContent.\n",
        )?;

        let workspace = Workspace::new();
        let root_url = Url::from_file_path(&tmp).map_err(|()| "bad path")?;
        workspace.set_workspace_roots(vec![root_url]);

        // Scan workspace — should populate symbol_index
        workspace.scan_workspace_files();
        assert!(
            workspace.symbol_index_len() > 0,
            "symbol index should have entries after scan"
        );

        // Open the document — should remove from symbol_index
        let doc_url = Url::from_file_path(tmp.join("doc.adoc")).map_err(|()| "bad path")?;
        workspace.update_document(
            doc_url.clone(),
            "= My Document\n\n== First Section\n\nContent.\n".to_string(),
            1,
        );
        assert!(
            !workspace.has_indexed_symbols(&doc_url),
            "opened doc should be removed from symbol_index"
        );

        // Close the document — should re-add to symbol_index
        workspace.remove_document(&doc_url);
        assert!(
            workspace.has_indexed_symbols(&doc_url),
            "closed doc should be re-added to symbol_index"
        );

        let _ = fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_discover_adoc_files() -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        let tmp = std::env::temp_dir().join("acdc_lsp_test_discover");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("subdir"))?;
        fs::create_dir_all(tmp.join(".git"))?;
        fs::create_dir_all(tmp.join("target"))?;

        fs::write(tmp.join("doc.adoc"), "= Doc\n")?;
        fs::write(tmp.join("subdir/nested.asciidoc"), "= Nested\n")?;
        fs::write(tmp.join("readme.txt"), "not an adoc file")?;
        fs::write(tmp.join(".git/config"), "git stuff")?;
        fs::write(tmp.join("target/output.adoc"), "build artifact")?;

        let root_url = Url::from_file_path(&tmp).map_err(|()| "bad path")?;
        let files = discover_adoc_files(&[root_url]);

        let filenames: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
            .collect();

        assert!(filenames.contains(&"doc.adoc".to_string()));
        assert!(filenames.contains(&"nested.asciidoc".to_string()));
        assert!(!filenames.contains(&"readme.txt".to_string()));
        assert!(!filenames.contains(&"config".to_string()));
        assert!(!filenames.contains(&"output.adoc".to_string()));

        let _ = fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_find_anchor_in_document() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;

        workspace.update_document(
            uri.clone(),
            "[[my-anchor]]\n== Section\n\nContent.\n".to_string(),
            1,
        );

        assert!(
            workspace
                .find_anchor_in_document(&uri, "my-anchor")
                .is_some()
        );
        assert!(
            workspace
                .find_anchor_in_document(&uri, "nonexistent")
                .is_none()
        );
        Ok(())
    }
}
