//! Find References: locate all usages of an anchor or xref

use tower_lsp::lsp_types::{Position, Url};

use crate::convert::{location_to_range, position_to_offset};
use crate::state::{DocumentState, Workspace, XrefTarget};

/// Find all references to the symbol at the given position.
///
/// Returns locations of all references across all open documents:
/// - Cursor on anchor: returns all xrefs pointing to this anchor (local + cross-file)
/// - Cursor on xref: returns the anchor definition + all xrefs to the same target
#[must_use]
pub fn find_references(
    doc: &DocumentState,
    doc_uri: &Url,
    workspace: &Workspace,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<tower_lsp::lsp_types::Location>> {
    let offset = position_to_offset(&doc.text, position)?;
    let ast = doc.ast.as_ref()?;

    // Check if cursor is on an xref - find all references to its target
    if let Some((target_id, _xref_loc)) = super::hover::find_xref_at_offset(ast, offset) {
        let parsed = XrefTarget::parse(&target_id);
        let anchor_id = parsed.anchor.as_deref().unwrap_or(&target_id);
        return Some(collect_cross_file_references(
            workspace,
            doc_uri,
            anchor_id,
            include_declaration,
            Some(&parsed),
        ));
    }

    // Check if cursor is on an anchor - find all xrefs pointing to it
    if let Some((anchor_id, _anchor_loc)) = super::hover::find_anchor_at_offset(ast, offset, doc) {
        return Some(collect_cross_file_references(
            workspace,
            doc_uri,
            &anchor_id,
            include_declaration,
            None,
        ));
    }

    None
}

/// Collect all references to a given anchor ID across all open documents.
///
/// When `xref_target` is provided (cross-file xref), also tries resolving the
/// target file on disk if the anchor isn't in the global index.
fn collect_cross_file_references(
    workspace: &Workspace,
    current_uri: &Url,
    anchor_id: &str,
    include_declaration: bool,
    xref_target: Option<&XrefTarget>,
) -> Vec<tower_lsp::lsp_types::Location> {
    let mut locations = Vec::new();

    // Include anchor declarations from workspace index
    if include_declaration {
        let global = workspace.find_anchor_globally(anchor_id);
        if global.is_empty() {
            // Try on-disk resolution for cross-file xrefs
            if let Some(parsed) = xref_target
                && let Some(file_path) = &parsed.file
                && let Some(target_uri) = workspace.resolve_xref_file(current_uri, file_path)
            {
                if let Some(loc) = workspace.find_anchor_in_document(&target_uri, anchor_id) {
                    locations.push(tower_lsp::lsp_types::Location {
                        uri: target_uri,
                        range: location_to_range(&loc),
                    });
                }
            }
        } else {
            for (uri, loc) in global {
                locations.push(tower_lsp::lsp_types::Location {
                    uri,
                    range: location_to_range(&loc),
                });
            }
        }
    }

    // Collect all xrefs pointing to this target across all open documents
    workspace.for_each_document(|uri, doc| {
        for (xref_target, xref_loc) in &doc.xrefs {
            let parsed = XrefTarget::parse(xref_target);
            let target_anchor = parsed.anchor.as_deref().unwrap_or(xref_target.as_str());
            if target_anchor == anchor_id {
                locations.push(tower_lsp::lsp_types::Location {
                    uri: uri.clone(),
                    range: location_to_range(xref_loc),
                });
            }
        }
    });

    locations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;

    #[test]
    fn test_find_references_from_xref() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[target]]
== Target Section

First reference <<target>>.

Second reference <<target>>.
";
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on the first xref (line 3, character 22 is inside <<target>>)
        let position = Position {
            line: 3,
            character: 22,
        };

        let result = find_references(&doc, &uri, &workspace, position, true);
        assert!(result.is_some());

        let locs = result.unwrap_or_default();
        // Should find: anchor definition + 2 xrefs = 3 references
        assert_eq!(locs.len(), 3);
        Ok(())
    }

    #[test]
    fn test_find_references_from_anchor() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[my-anchor]]
== My Section

See <<my-anchor>> for more info.
";
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on the anchor (line 0, inside [[my-anchor]])
        let position = Position {
            line: 0,
            character: 5,
        };

        let result = find_references(&doc, &uri, &workspace, position, true);
        assert!(result.is_some());

        let locs = result.unwrap_or_default();
        // Should find: anchor definition + 1 xref = 2 references
        assert_eq!(locs.len(), 2);
        Ok(())
    }

    #[test]
    fn test_find_references_without_declaration() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[target]]
== Target Section

Reference <<target>>.
";
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on the xref
        let position = Position {
            line: 3,
            character: 14,
        };

        let result = find_references(&doc, &uri, &workspace, position, false);
        assert!(result.is_some());

        let locs = result.unwrap_or_default();
        // Should find only the xref, not the anchor declaration
        assert_eq!(locs.len(), 1);
        Ok(())
    }

    #[test]
    fn test_cross_file_references() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri1 = Url::parse("file:///doc1.adoc")?;
        let uri2 = Url::parse("file:///doc2.adoc")?;

        let content1 = "[[shared-anchor]]\n== Shared Section\n\nContent.\n";
        let content2 = "= Other Doc\n\nSee xref:doc1.adoc#shared-anchor[link].\n";

        workspace.update_document(uri1.clone(), content1.to_string(), 1);
        workspace.update_document(uri2, content2.to_string(), 1);

        let doc = workspace.get_document(&uri1).ok_or("document not found")?;

        // Position on the anchor in doc1
        let position = Position {
            line: 0,
            character: 5,
        };

        let result = find_references(&doc, &uri1, &workspace, position, true);
        assert!(result.is_some());

        let locs = result.unwrap_or_default();
        // anchor declaration in doc1 + xref in doc2
        assert!(
            locs.len() >= 2,
            "Expected at least 2 references, got {}",
            locs.len()
        );
        Ok(())
    }
}
