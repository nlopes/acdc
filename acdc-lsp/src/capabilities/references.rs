//! Find References: locate all usages of an anchor or xref

use tower_lsp::lsp_types::Position;

use crate::convert::{location_to_range, position_to_offset};
use crate::state::DocumentState;

/// Find all references to the symbol at the given position.
///
/// Returns locations of all references if the cursor is on an anchor or xref:
/// - Cursor on anchor: returns all xrefs pointing to this anchor
/// - Cursor on xref: returns the anchor definition + all xrefs to the same target
#[must_use]
pub fn find_references(
    doc: &DocumentState,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<tower_lsp::lsp_types::Range>> {
    let offset = position_to_offset(&doc.text, position)?;
    let ast = doc.ast.as_ref()?;

    // Check if cursor is on an xref - find all references to its target
    if let Some((target_id, _xref_loc)) = super::hover::find_xref_at_offset(ast, offset) {
        return Some(collect_references_to_target(
            doc,
            &target_id,
            include_declaration,
        ));
    }

    // Check if cursor is on an anchor - find all xrefs pointing to it
    if let Some((anchor_id, _anchor_loc)) = super::hover::find_anchor_at_offset(ast, offset, doc) {
        return Some(collect_references_to_target(
            doc,
            &anchor_id,
            include_declaration,
        ));
    }

    None
}

/// Collect all references to a given target ID
fn collect_references_to_target(
    doc: &DocumentState,
    target_id: &str,
    include_declaration: bool,
) -> Vec<tower_lsp::lsp_types::Range> {
    let mut ranges = Vec::new();

    // Include the anchor definition if requested
    if include_declaration && let Some(anchor_loc) = doc.anchors.get(target_id) {
        ranges.push(location_to_range(anchor_loc));
    }

    // Collect all xrefs pointing to this target
    for (xref_target, xref_loc) in &doc.xrefs {
        if xref_target == target_id {
            ranges.push(location_to_range(xref_loc));
        }
    }

    ranges
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
    fn test_find_references_from_xref() {
        let content = r"[[target]]
== Target Section

First reference <<target>>.

Second reference <<target>>.
";
        let doc = create_test_doc_state(content);

        // Position on the first xref (line 3, character 20 is inside <<target>>)
        let position = Position {
            line: 3,
            character: 22,
        };

        let result = find_references(&doc, position, true);
        assert!(result.is_some());

        let ranges = result.unwrap_or_default();
        // Should find: anchor definition + 2 xrefs = 3 references
        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn test_find_references_from_anchor() {
        let content = r"[[my-anchor]]
== My Section

See <<my-anchor>> for more info.
";
        let doc = create_test_doc_state(content);

        // Position on the anchor (line 0, inside [[my-anchor]])
        let position = Position {
            line: 0,
            character: 5,
        };

        let result = find_references(&doc, position, true);
        assert!(result.is_some());

        let ranges = result.unwrap_or_default();
        // Should find: anchor definition + 1 xref = 2 references
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn test_find_references_without_declaration() {
        let content = r"[[target]]
== Target Section

Reference <<target>>.
";
        let doc = create_test_doc_state(content);

        // Position on the xref
        let position = Position {
            line: 3,
            character: 14,
        };

        let result = find_references(&doc, position, false);
        assert!(result.is_some());

        let ranges = result.unwrap_or_default();
        // Should find only the xref, not the anchor declaration
        assert_eq!(ranges.len(), 1);
    }
}
