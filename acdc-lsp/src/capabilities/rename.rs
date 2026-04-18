//! Rename: refactor anchor IDs and update all references

use std::collections::HashMap;

use tower_lsp_server::ls_types::{Position, PrepareRenameResponse, TextEdit, Uri, WorkspaceEdit};

use crate::convert::{location_to_range, position_to_offset};
use crate::state::{DocumentState, Workspace, XrefTarget};

/// Prepare for rename operation - validate the position is on an anchor or xref.
///
/// Returns the range and current placeholder text if valid.
#[must_use]
pub(crate) fn prepare_rename(
    doc: &DocumentState,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let offset = position_to_offset(doc.text(), position)?;
    let ast_guard = doc.ast()?;
    let ast = ast_guard.document();

    // Check if cursor is on an xref
    if let Some((target_id, xref_loc)) = super::hover::find_xref_at_offset(ast, offset) {
        return Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: location_to_range(&xref_loc),
            placeholder: target_id,
        });
    }

    // Check if cursor is on an anchor
    if let Some((anchor_id, anchor_loc)) = super::hover::find_anchor_at_offset(ast, offset, doc) {
        return Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: location_to_range(&anchor_loc),
            placeholder: anchor_id,
        });
    }

    None
}

/// Compute the workspace edit for renaming an anchor/xref.
///
/// Returns edits for the anchor definition and all xrefs pointing to it,
/// across all open documents.
#[must_use]
pub(crate) fn compute_rename(
    doc: &DocumentState,
    uri: &Uri,
    workspace: &Workspace,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let offset = position_to_offset(doc.text(), position)?;
    let ast_guard = doc.ast()?;
    let ast = ast_guard.document();

    // Find the target ID (from either xref or anchor)
    let target_id = if let Some((id, _)) = super::hover::find_xref_at_offset(ast, offset) {
        // If it's a cross-file xref, extract just the anchor part
        let parsed = XrefTarget::parse(&id);
        parsed.anchor.unwrap_or(id)
    } else {
        super::hover::find_anchor_at_offset(ast, offset, doc).map(|(id, _)| id)?
    };

    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();

    // Add edits for anchor declarations across all documents
    for (anchor_uri, anchor_loc) in workspace.find_anchor_globally(&target_id) {
        changes.entry(anchor_uri).or_default().push(TextEdit {
            range: location_to_range(&anchor_loc),
            new_text: new_name.to_string(),
        });
    }

    // Add edits for all xrefs pointing to this target across all documents
    workspace.for_each_document(|doc_uri, doc_state| {
        for (xref_target, xref_loc) in &doc_state.xrefs {
            let parsed = XrefTarget::parse(xref_target);
            let anchor = parsed.anchor.as_deref().unwrap_or(xref_target.as_str());
            if anchor == target_id {
                changes.entry(doc_uri.clone()).or_default().push(TextEdit {
                    range: location_to_range(xref_loc),
                    new_text: if parsed.is_cross_file() {
                        // Preserve the file path, only rename the anchor part
                        if let Some(file) = &parsed.file {
                            format!("{file}#{new_name}")
                        } else {
                            new_name.to_string()
                        }
                    } else {
                        new_name.to_string()
                    },
                });
            }
        }
    });

    // If no edits were found via workspace, fall back to current document only
    if changes.is_empty() {
        let mut edits = Vec::new();

        if let Some(anchor_loc) = doc.anchors.get(&target_id) {
            edits.push(TextEdit {
                range: location_to_range(anchor_loc),
                new_text: new_name.to_string(),
            });
        }

        for (xref_target, xref_loc) in &doc.xrefs {
            if xref_target == &target_id {
                edits.push(TextEdit {
                    range: location_to_range(xref_loc),
                    new_text: new_name.to_string(),
                });
            }
        }

        if !edits.is_empty() {
            changes.insert(uri.clone(), edits);
        }
    }

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;

    #[test]
    fn test_prepare_rename_on_anchor() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[my-anchor]]
== Section Title

Reference <<my-anchor>> here.
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on the anchor (line 0, character 5 is inside [[my-anchor]])
        let position = Position {
            line: 0,
            character: 5,
        };

        let result = prepare_rename(&doc, position);
        assert!(
            result.is_some(),
            "Expected prepare_rename to return a result"
        );
        assert!(
            matches!(
                &result,
                Some(PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. }) if placeholder == "my-anchor"
            ),
            "Expected RangeWithPlaceholder with 'my-anchor'"
        );
        Ok(())
    }

    #[test]
    fn test_prepare_rename_on_xref() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[target]]
== Target Section

See <<target>> for details.
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on the xref (line 3, character 6 is inside <<target>>)
        let position = Position {
            line: 3,
            character: 6,
        };

        let result = prepare_rename(&doc, position);
        assert!(
            result.is_some(),
            "Expected prepare_rename to return a result"
        );
        assert!(
            matches!(
                &result,
                Some(PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. }) if placeholder == "target"
            ),
            "Expected RangeWithPlaceholder with 'target'"
        );
        Ok(())
    }

    #[test]
    fn test_compute_rename() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[old-name]]
== Section

First ref: <<old-name>>.

Second ref: <<old-name>>.
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("expected doc")?;

        // Position on the anchor
        let position = Position {
            line: 0,
            character: 5,
        };

        let result = compute_rename(&doc, &uri, &workspace, position, "new-name");
        assert!(result.is_some(), "expected edit result");
        let edit = result.ok_or("expected edit")?;
        assert!(edit.changes.is_some(), "expected changes");
        let changes = edit.changes.ok_or("expected changes")?;
        let file_edits = changes.get(&uri);
        assert!(file_edits.is_some(), "expected edits for URI");
        let file_edits = file_edits.ok_or("expected edits")?;

        // Should have 3 edits: 1 anchor + 2 xrefs
        assert_eq!(file_edits.len(), 3);

        // All edits should have the new name
        for text_edit in file_edits {
            assert_eq!(text_edit.new_text, "new-name");
        }
        Ok(())
    }

    #[test]
    fn test_prepare_rename_invalid_position() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"= Document

Just some text here.
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on regular text (not anchor or xref)
        let position = Position {
            line: 2,
            character: 5,
        };

        let result = prepare_rename(&doc, position);
        assert!(result.is_none());
        Ok(())
    }
}
