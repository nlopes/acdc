//! Rename: refactor anchor IDs and update all references

use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, PrepareRenameResponse, TextEdit, Url, WorkspaceEdit};

use crate::convert::{location_to_range, position_to_offset};
use crate::state::DocumentState;

/// Prepare for rename operation - validate the position is on an anchor or xref.
///
/// Returns the range and current placeholder text if valid.
#[must_use]
pub fn prepare_rename(doc: &DocumentState, position: Position) -> Option<PrepareRenameResponse> {
    let offset = position_to_offset(&doc.text, position)?;
    let ast = doc.ast.as_ref()?;

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
/// Returns edits for the anchor definition and all xrefs pointing to it.
#[must_use]
pub fn compute_rename(
    doc: &DocumentState,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let offset = position_to_offset(&doc.text, position)?;
    let ast = doc.ast.as_ref()?;

    // Find the target ID (from either xref or anchor)
    let target_id = if let Some((id, _)) = super::hover::find_xref_at_offset(ast, offset) {
        Some(id)
    } else {
        super::hover::find_anchor_at_offset(ast, offset, doc).map(|(id, _)| id)
    }?;

    let mut edits = Vec::new();

    // Add edit for the anchor definition
    if let Some(anchor_loc) = doc.anchors.get(&target_id) {
        edits.push(TextEdit {
            range: location_to_range(anchor_loc),
            new_text: new_name.to_string(),
        });
    }

    // Add edits for all xrefs pointing to this target
    for (xref_target, xref_loc) in &doc.xrefs {
        if xref_target == &target_id {
            edits.push(TextEdit {
                range: location_to_range(xref_loc),
                new_text: new_name.to_string(),
            });
        }
    }

    // Return workspace edit
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::definition::{collect_anchors, collect_xrefs};
    use crate::state::DocumentState;
    use acdc_parser::Options;

    fn create_test_doc_state(content: &str) -> DocumentState {
        let options = Options::default();
        let result = acdc_parser::parse(content, &options);

        match result {
            Ok(doc) => {
                let anchors = collect_anchors(&doc);
                let xrefs = collect_xrefs(&doc);
                DocumentState::new_success(content.to_string(), 1, doc, anchors, xrefs)
            }
            Err(_) => DocumentState::new_failure(content.to_string(), 1, vec![]),
        }
    }

    #[test]
    fn test_prepare_rename_on_anchor() {
        let content = r"[[my-anchor]]
== Section Title

Reference <<my-anchor>> here.
";
        let doc = create_test_doc_state(content);

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
    }

    #[test]
    fn test_prepare_rename_on_xref() {
        let content = r"[[target]]
== Target Section

See <<target>> for details.
";
        let doc = create_test_doc_state(content);

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
    }

    #[test]
    fn test_compute_rename() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[old-name]]
== Section

First ref: <<old-name>>.

Second ref: <<old-name>>.
";
        let doc = create_test_doc_state(content);
        let uri = Url::parse("file:///test.adoc")?;

        // Position on the anchor
        let position = Position {
            line: 0,
            character: 5,
        };

        let result = compute_rename(&doc, &uri, position, "new-name");
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
    fn test_prepare_rename_invalid_position() {
        let content = r"= Document

Just some text here.
";
        let doc = create_test_doc_state(content);

        // Position on regular text (not anchor or xref)
        let position = Position {
            line: 2,
            character: 5,
        };

        let result = prepare_rename(&doc, position);
        assert!(result.is_none());
    }
}
