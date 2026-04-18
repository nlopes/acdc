//! Workspace-level state management

use std::collections::HashMap;
use std::sync::RwLock;

use acdc_parser::Location;
use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use tower_lsp_server::ls_types::Uri;

use crate::capabilities::{
    definition, diagnostics,
    workspace_symbols::{IndexedSymbol, extract_workspace_symbols},
};
use crate::limits::{MAX_INDEXABLE_FILE_BYTES, read_bounded};
use crate::state::DocumentState;
use crate::state::document::ParsedText;

/// Workspace-level state management
pub(crate) struct Workspace {
    /// Open documents: URI -> `DocumentState`
    documents: DashMap<Uri, DocumentState>,
    /// Global anchor index: `anchor_id` -> [(`file_uri`, location)]
    anchor_index: DashMap<String, Vec<(Uri, Location)>>,
    /// Workspace root directories
    roots: RwLock<Vec<Uri>>,
    /// Cached symbols for non-open files (populated by workspace scan)
    symbol_index: DashMap<Uri, Vec<IndexedSymbol>>,
}

impl Workspace {
    /// Create a new workspace
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            documents: DashMap::new(),
            anchor_index: DashMap::new(),
            roots: RwLock::new(Vec::new()),
            symbol_index: DashMap::new(),
        }
    }

    /// Set workspace root directories (from initialize params)
    pub(crate) fn set_workspace_roots(&self, roots: Vec<Uri>) {
        if let Ok(mut w) = self.roots.write() {
            *w = roots;
        }
    }

    /// Update document on open/change
    pub(crate) fn update_document(&self, uri: Uri, text: String, version: i32) {
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
                && let Some(target_uri) = crate::convert::resolve_relative_uri(&uri, file_path)
            {
                if let Some(anchor_id) = &parsed.anchor {
                    return Self::find_anchor_in_file_on_disk(&target_uri, anchor_id).is_some();
                }
                // File-only reference (no anchor) — just check file exists
                return target_uri.to_file_path().is_some_and(|p| p.exists());
            }
            false
        };
        let mut new_diagnostics = state.diagnostics;
        new_diagnostics.extend(diagnostics::compute_warnings(
            &state.anchors,
            &state.xrefs,
            Some(&cross_file_resolver),
        ));
        state.diagnostics = new_diagnostics;

        // Compute link diagnostics (missing images, audio, video, includes)
        if let Some(doc_path) = uri.to_file_path()
            && let Some(doc_dir) = doc_path.parent()
        {
            let imagesdir_owned: Option<String> =
                state
                    .ast()
                    .and_then(|ast| match ast.document().attributes.get("imagesdir") {
                        Some(acdc_parser::AttributeValue::String(s)) => Some(s.to_string()),
                        _ => None,
                    });
            let link_diags = diagnostics::compute_link_diagnostics(
                &state.media_sources,
                &state.includes,
                doc_dir,
                imagesdir_owned.as_deref(),
            );
            state.diagnostics.extend(link_diags);
        }

        // Compute conditional diagnostics (inactive ifdef/ifndef graying)
        let conditional_diags = diagnostics::compute_conditional_diagnostics(&state.conditionals);
        state.diagnostics.extend(conditional_diags);

        // Compute section level diagnostics (skipped heading levels)
        let section_diags = state
            .ast()
            .map(|ast| diagnostics::compute_section_level_diagnostics(ast.document()))
            .unwrap_or_default();
        state.diagnostics.extend(section_diags);

        self.documents.insert(uri, state);
    }

    /// Get a reference to a document's state
    #[must_use]
    pub(crate) fn get_document(&self, uri: &Uri) -> Option<Ref<'_, Uri, DocumentState>> {
        self.documents.get(uri)
    }

    /// Remove a document from the workspace
    pub(crate) fn remove_document(&self, uri: &Uri) {
        self.remove_anchors_for_uri(uri);
        self.documents.remove(uri);
        // Re-index from disk for workspace symbols
        self.reindex_file_from_disk(uri);
    }

    /// Find an anchor across all open documents
    #[must_use]
    pub(crate) fn find_anchor_globally(&self, anchor_id: &str) -> Vec<(Uri, Location)> {
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
    pub(crate) fn find_anchor_in_document(&self, uri: &Uri, anchor_id: &str) -> Option<Location> {
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

    /// Read a file from disk and search for an anchor without indexing it
    fn find_anchor_in_file_on_disk(uri: &Uri, anchor_id: &str) -> Option<Location> {
        let path = uri.to_file_path()?;
        tracing::info!(?path, anchor_id, "reading file from disk for anchor lookup");
        let text = read_bounded(path.as_ref())?;
        let parsed = acdc_parser::parse(&text, &acdc_parser::Options::default()).ok()?;
        let anchors = definition::collect_anchors(parsed.document());
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
    pub(crate) fn all_anchors(&self) -> Vec<(String, Uri)> {
        let mut result = Vec::new();
        for entry in &self.anchor_index {
            for (uri, _loc) in entry.value() {
                result.push((entry.key().clone(), uri.clone()));
            }
        }
        result
    }

    /// Scan workspace roots for `AsciiDoc` files and populate the symbol index.
    pub(crate) fn scan_workspace_files(&self) {
        let roots: Vec<Uri> = self.roots.read().map(|r| r.clone()).unwrap_or_default();
        let files = discover_adoc_files(&roots);

        for path in files {
            let Some(uri) = Uri::from_file_path(&path) else {
                continue;
            };
            // Skip files that are already open in the editor
            if self.documents.contains_key(&uri) {
                continue;
            }
            if let Some(text) = read_bounded(&path)
                && let Ok(parsed) = acdc_parser::parse(&text, &acdc_parser::Options::default())
            {
                let symbols = extract_workspace_symbols(parsed.document());
                self.symbol_index.insert(uri, symbols);
            }
        }
    }

    /// Number of files in the symbol index
    #[must_use]
    pub(crate) fn symbol_index_len(&self) -> usize {
        self.symbol_index.len()
    }

    /// Query workspace symbols across all documents (open + indexed).
    ///
    /// Returns `(Uri, IndexedSymbol)` pairs matching the query. Empty query
    /// returns all symbols. Matching is case-insensitive substring.
    #[must_use]
    pub(crate) fn query_workspace_symbols(&self, query: &str) -> Vec<(Uri, IndexedSymbol)> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Symbols from open documents (live AST)
        for entry in &self.documents {
            let uri = entry.key();
            if let Some(ast) = entry.value().ast() {
                let symbols = extract_workspace_symbols(ast.document());
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

    /// Check if a document is currently open
    #[must_use]
    pub(crate) fn has_document(&self, uri: &Uri) -> bool {
        self.documents.contains_key(uri)
    }

    /// Rename a document's URI in all workspace indexes.
    ///
    /// Called after a file rename to keep internal state consistent.
    pub(crate) fn rename_document_uri(&self, old_uri: &Uri, new_uri: &Uri) {
        // Move document state
        if let Some((_, state)) = self.documents.remove(old_uri) {
            self.documents.insert(new_uri.clone(), state);
        }

        // Update anchor_index entries
        for mut entry in self.anchor_index.iter_mut() {
            for (uri, _) in entry.value_mut() {
                if uri == old_uri {
                    *uri = new_uri.clone();
                }
            }
        }

        // Move symbol_index entry
        if let Some((_, symbols)) = self.symbol_index.remove(old_uri) {
            self.symbol_index.insert(new_uri.clone(), symbols);
        }
    }

    /// Discover all `AsciiDoc` files in the workspace roots.
    #[must_use]
    pub(crate) fn discover_workspace_files(&self) -> Vec<std::path::PathBuf> {
        let roots: Vec<Uri> = self.roots.read().map(|r| r.clone()).unwrap_or_default();
        discover_adoc_files(&roots)
    }

    fn reindex_file_from_disk(&self, uri: &Uri) {
        if let Some(path) = uri.to_file_path()
            && let Some(text) = read_bounded(path.as_ref())
            && let Ok(parsed) = acdc_parser::parse(&text, &acdc_parser::Options::default())
        {
            let symbols = extract_workspace_symbols(parsed.document());
            self.symbol_index.insert(uri.clone(), symbols);
        }
    }

    /// Iterate over all open documents
    pub(crate) fn for_each_document<F>(&self, mut f: F)
    where
        F: FnMut(&Uri, &DocumentState),
    {
        for entry in &self.documents {
            f(entry.key(), entry.value());
        }
    }

    /// Remove all anchor entries for a given URI from the global index
    fn remove_anchors_for_uri(&self, uri: &Uri) {
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

    /// Line-1 informational diagnostic explaining why AST-backed features
    /// are disabled for oversized open documents.
    fn oversized_document_diagnostic(bytes: usize) -> tower_lsp_server::ls_types::Diagnostic {
        use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, Position, Range};

        let bytes_u64 = u64::try_from(bytes).unwrap_or(u64::MAX);
        let mib = bytes_u64 / (1024 * 1024);
        let limit_mib = MAX_INDEXABLE_FILE_BYTES / (1024 * 1024);
        Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            severity: Some(DiagnosticSeverity::INFORMATION),
            source: Some("acdc".to_string()),
            message: format!(
                "Document is {mib} MiB (above the acdc-lsp indexing limit of {limit_mib} MiB); navigation and workspace features are disabled for this file."
            ),
            ..Default::default()
        }
    }

    /// Parse document and extract all navigation data
    fn parse_and_index(text: String, version: i32) -> DocumentState {
        let options = acdc_parser::Options::default();

        let mut anchors: HashMap<String, Location> = HashMap::new();
        let mut xrefs: Vec<(String, Location)> = Vec::new();
        let mut media_sources: Vec<(crate::state::document::OwnedSource, Location)> = Vec::new();
        let mut parse_diagnostics: Vec<tower_lsp_server::ls_types::Diagnostic> = Vec::new();
        let mut ast_attributes: Option<acdc_parser::DocumentAttributes<'static>> = None;

        // Skip parsing oversized open documents: the parser arena pre-sizes
        // to input length, so a 1 GB file would balloon RSS. Raw-text scans
        // below still run; the diagnostic tells the user why the rest is silent.
        let parsed = if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_INDEXABLE_FILE_BYTES {
            tracing::warn!(
                bytes = text.len(),
                limit = MAX_INDEXABLE_FILE_BYTES,
                "document exceeds LSP indexing size limit; skipping parse"
            );
            parse_diagnostics.push(Self::oversized_document_diagnostic(text.len()));
            ParsedText::from_source(text.into_boxed_str())
        } else {
            match acdc_parser::parse(&text, &options) {
                Ok(parsed_doc) => {
                    {
                        let doc = parsed_doc.document();
                        anchors = definition::collect_anchors(doc);
                        xrefs = definition::collect_xrefs(doc);
                        media_sources = definition::collect_media_sources(doc);
                        ast_attributes = Some(doc.attributes.to_static());
                    }
                    ParsedText::from_parsed(text.clone().into_boxed_str(), parsed_doc)
                }
                Err(error) => {
                    parse_diagnostics.push(diagnostics::error_to_diagnostic(&error));
                    ParsedText::from_source(text.into_boxed_str())
                }
            }
        };

        let raw_text = parsed.text();
        let definitions = crate::state::document::extract_attribute_defs(raw_text);
        let references = crate::state::document::extract_attribute_refs(raw_text);
        let raw_includes = crate::state::document::extract_includes(raw_text);
        let raw_conditionals = if let Some(attrs) = &ast_attributes {
            crate::state::document::extract_conditionals(raw_text, attrs)
        } else {
            crate::state::document::extract_conditionals(
                raw_text,
                &acdc_parser::DocumentAttributes::default(),
            )
        };

        DocumentState {
            parsed,
            version,
            diagnostics: parse_diagnostics,
            anchors,
            xrefs,
            includes: raw_includes,
            attribute_refs: references,
            attribute_defs: definitions,
            media_sources,
            conditionals: raw_conditionals,
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
fn discover_adoc_files(roots: &[Uri]) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        if let Some(path) = root.to_file_path() {
            walk_directory(path.as_ref(), &mut files);
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
        let uri = "file:///test.adoc".parse::<Uri>()?;

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
        let uri = "file:///test.adoc".parse::<Uri>()?;

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
        let uri = "file:///test.adoc".parse::<Uri>()?;

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
    fn test_all_anchors_across_documents() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri1 = "file:///doc1.adoc".parse::<Uri>()?;
        let uri2 = "file:///doc2.adoc".parse::<Uri>()?;

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
        let ids: Vec<&str> = all.iter().map(|(id, _)| id.as_ref()).collect();
        assert!(ids.contains(&"anchor1"));
        assert!(ids.contains(&"anchor2"));
        Ok(())
    }

    #[test]
    fn test_workspace_symbol_query() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri1 = "file:///doc1.adoc".parse::<Uri>()?;
        let uri2 = "file:///doc2.adoc".parse::<Uri>()?;

        // Open doc1 (live AST)
        workspace.update_document(
            uri1.clone(),
            "= Doc One\n\n== Introduction\n\nContent.\n".to_string(),
            1,
        );

        // Manually add doc2 to symbol_index (simulates scanned file)
        let parsed2 = acdc_parser::parse(
            "= Doc Two\n\n== TLS Configuration\n\nStuff.\n",
            &acdc_parser::Options::default(),
        )?;
        let symbols = extract_workspace_symbols(parsed2.document());
        workspace.symbol_index.insert(uri2.clone(), symbols);

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
        let root_url = Uri::from_file_path(&tmp).ok_or("bad path")?;
        workspace.set_workspace_roots(vec![root_url]);

        // Scan workspace — should populate symbol_index
        workspace.scan_workspace_files();
        assert!(
            workspace.symbol_index_len() > 0,
            "symbol index should have entries after scan"
        );

        // Open the document — should remove from symbol_index
        let doc_url = Uri::from_file_path(tmp.join("doc.adoc")).ok_or("bad path")?;
        workspace.update_document(
            doc_url.clone(),
            "= My Document\n\n== First Section\n\nContent.\n".to_string(),
            1,
        );
        assert!(
            !workspace.symbol_index.contains_key(&doc_url),
            "opened doc should be removed from symbol_index"
        );

        // Close the document — should re-add to symbol_index
        workspace.remove_document(&doc_url);
        assert!(
            workspace.symbol_index.contains_key(&doc_url),
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

        let root_url = Uri::from_file_path(&tmp).ok_or("bad path")?;
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
        let uri = "file:///test.adoc".parse::<Uri>()?;

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

    #[test]
    fn test_missing_image_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = std::env::temp_dir().join("acdc_lsp_test_img_diag");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;

        let workspace = Workspace::new();
        let uri = Uri::from_file_path(tmp.join("doc.adoc")).ok_or("bad path")?;
        let content = "= Document\n\nimage::nonexistent.png[]\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        let doc = workspace.get_document(&uri).ok_or("document not found")?;
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.message.contains("nonexistent.png")),
            "expected diagnostic about missing image, got: {:?}",
            doc.diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_missing_include_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = std::env::temp_dir().join("acdc_lsp_test_inc_diag");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;

        let workspace = Workspace::new();
        let uri = Uri::from_file_path(tmp.join("doc.adoc")).ok_or("bad path")?;
        let content = "= Document\n\ninclude::missing.adoc[]\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        let doc = workspace.get_document(&uri).ok_or("document not found")?;
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.message.contains("missing.adoc")),
            "expected diagnostic about missing include, got: {:?}",
            doc.diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    use tower_lsp_server::ls_types::DiagnosticSeverity;

    #[test]
    fn test_section_level_skip_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;

        // First section is === (level 2), skipping == (level 1)
        let content = "= Document Title\n\n=== Skipped Level\n\nSome content.\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        let doc = workspace.get_document(&uri).ok_or("document not found")?;
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.message.contains("Section level skipped")
                    && d.severity == Some(DiagnosticSeverity::WARNING)),
            "expected section level skip warning, got: {:?}",
            doc.diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn test_nested_section_level_skip_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;

        // Nested skip: == followed by ==== (parser error path)
        let content = "= Document Title\n\n== Section\n\n==== Skipped\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        let doc = workspace.get_document(&uri).ok_or("document not found")?;
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.message.contains("Section level skipped")
                    && d.severity == Some(DiagnosticSeverity::WARNING)),
            "expected section level skip warning from parser error, got: {:?}",
            doc.diagnostics
                .iter()
                .map(|d| (&d.message, &d.severity))
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn test_section_level_valid_no_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;

        let content = "= Document Title\n\n== Section 1\n\n=== Subsection\n\n== Section 2\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        let doc = workspace.get_document(&uri).ok_or("document not found")?;
        assert!(
            !doc.diagnostics
                .iter()
                .any(|d| d.message.contains("Section level skipped")),
            "should not have section level skip warnings for valid document"
        );
        Ok(())
    }

    /// Replays edits on a URL-heavy document and asserts the workspace keeps
    /// exactly one document and the expected media-source count — catches
    /// any per-edit accumulation reintroduced into `DocumentState`.
    #[test]
    fn update_document_does_not_accumulate_state() -> Result<(), Box<dyn std::error::Error>> {
        use std::fmt::Write as _;

        let workspace = Workspace::new();
        let uri = "file:///url_heavy.adoc".parse::<Uri>()?;

        let mut content = String::from("= URL-heavy document\n\n");
        for i in 0..10 {
            write!(
                content,
                "image::https://cdn.example.com/pic-{i}.png[Pic {i}]\n\n"
            )?;
            write!(
                content,
                "See link:https://example.com/page-{i}[page {i}].\n\n"
            )?;
            write!(content, "Raw https://example.com/bare-{i} link.\n\n")?;
        }

        for version in 1..=200 {
            workspace.update_document(uri.clone(), content.clone(), version);
        }

        let doc = workspace.get_document(&uri).ok_or("document missing")?;
        assert_eq!(doc.version, 200);
        assert_eq!(
            doc.media_sources.len(),
            10,
            "media_sources should hold exactly the 10 image URLs from the final \
             parse, not accumulate across edits",
        );
        Ok(())
    }

    /// Oversized open documents skip parsing and emit one informational
    /// diagnostic on line 1 so AST-backed features degrade gracefully.
    #[test]
    fn oversized_open_document_is_gated() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///huge.adoc".parse::<Uri>()?;

        let size = usize::try_from(MAX_INDEXABLE_FILE_BYTES).unwrap_or(usize::MAX) + 1;
        let mut content = String::with_capacity(size);
        content.push_str("= Huge document\n\n");
        content.push_str(&"a".repeat(size - content.len()));

        workspace.update_document(uri.clone(), content, 1);

        let doc = workspace.get_document(&uri).ok_or("document missing")?;
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.message.contains("indexing limit")),
            "expected an 'indexing limit' informational diagnostic"
        );
        assert!(doc.anchors.is_empty());
        assert!(doc.xrefs.is_empty());
        assert!(doc.media_sources.is_empty());
        Ok(())
    }
}
