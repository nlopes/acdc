//! Call Hierarchy: show include-tree relationships between `AsciiDoc` documents
//!
//! Maps `include::` directives to the LSP Call Hierarchy protocol:
//! - "Outgoing calls" = documents this file includes
//! - "Incoming calls" = documents that include this file

use std::collections::HashMap;

use tower_lsp_server::ls_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, Range,
    SymbolKind, Uri,
};

use crate::{
    convert::{
        byte_range_to_utf16_range, file_path_to_uri, location_to_utf16_range, offset_in_location,
        position_to_offset, resolve_relative_uri, uri_filename,
    },
    limits::read_bounded,
    state::{DocumentState, Workspace, extract_includes},
};

/// Build a `CallHierarchyItem` representing an `AsciiDoc` file.
fn make_call_hierarchy_item(uri: Uri, range: Range) -> CallHierarchyItem {
    let name = uri_filename(&uri).to_string();

    CallHierarchyItem {
        name,
        kind: SymbolKind::FILE,
        tags: None,
        detail: None,
        uri,
        range,
        selection_range: Range::default(),
        data: None,
    }
}

fn file_item(uri: Uri, workspace: &Workspace) -> CallHierarchyItem {
    let range = workspace
        .with_document_text(&uri, |text| byte_range_to_utf16_range(text, 0..text.len()))
        .flatten()
        .unwrap_or_default();
    make_call_hierarchy_item(uri, range)
}

/// Prepare call hierarchy items at the given position.
///
/// If the cursor is on an `include::` directive, returns an item for the
/// included file. Otherwise returns an item for the current document.
#[must_use]
pub(crate) fn prepare_call_hierarchy(
    doc: &DocumentState,
    doc_uri: &Uri,
    position: Position,
    workspace: &Workspace,
) -> Option<Vec<CallHierarchyItem>> {
    let offset = position_to_offset(doc.text(), position)?;

    // Check if cursor is on an include directive
    for (target, location) in &doc.includes {
        if offset_in_location(offset, location) {
            let resolved_uri = resolve_relative_uri(doc_uri, target)?;
            return Some(vec![file_item(resolved_uri, workspace)]);
        }
    }

    // Fallback: return an item for the current document
    let range = byte_range_to_utf16_range(doc.text(), 0..doc.text().len())?;
    Some(vec![make_call_hierarchy_item(doc_uri.clone(), range)])
}

/// Find all documents that include the given file (incoming calls).
#[must_use]
pub(crate) fn incoming_calls(
    item: &CallHierarchyItem,
    workspace: &Workspace,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let target_uri = &item.uri;
    let mut callers: HashMap<Uri, Vec<Range>> = HashMap::new();

    // Scan open documents
    workspace.for_each_document(|doc_uri, doc_state| {
        let mut ranges = Vec::new();
        for (include_target, location) in &doc_state.includes {
            if let Some(resolved) = resolve_relative_uri(doc_uri, include_target)
                && &resolved == target_uri
                && let Some(range) = location_to_utf16_range(doc_state.text(), location)
            {
                ranges.push(range);
            }
        }
        if !ranges.is_empty() {
            callers.entry(doc_uri.clone()).or_default().extend(ranges);
        }
    });

    // Scan non-open workspace files
    for path in workspace.discover_workspace_files() {
        let Some(file_uri) = file_path_to_uri(&path) else {
            continue;
        };
        if workspace.has_document(&file_uri) {
            continue;
        }
        let Some(text) = read_bounded(&path) else {
            continue;
        };
        let includes = extract_includes(&text);
        let mut ranges = Vec::new();
        for (include_target, location) in &includes {
            if let Some(resolved) = resolve_relative_uri(&file_uri, include_target)
                && &resolved == target_uri
                && let Some(range) = location_to_utf16_range(&text, location)
            {
                ranges.push(range);
            }
        }
        if !ranges.is_empty() {
            callers.entry(file_uri.clone()).or_default().extend(ranges);
        }
    }

    if callers.is_empty() {
        return None;
    }

    let calls = callers
        .into_iter()
        .map(|(uri, from_ranges)| CallHierarchyIncomingCall {
            from: file_item(uri, workspace),
            from_ranges,
        })
        .collect();

    Some(calls)
}

/// Find all documents that the given file includes (outgoing calls).
#[must_use]
pub(crate) fn outgoing_calls(
    item: &CallHierarchyItem,
    workspace: &Workspace,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    let item_uri = &item.uri;

    let includes = workspace.with_document_text(item_uri, |text| {
        extract_includes(text)
            .into_iter()
            .filter_map(|(target, location)| {
                let range = location_to_utf16_range(text, &location)?;
                Some((target, range))
            })
            .collect::<Vec<_>>()
    })?;

    if includes.is_empty() {
        return None;
    }

    let calls: Vec<CallHierarchyOutgoingCall> = includes
        .iter()
        .filter_map(|(target, range)| {
            let resolved_uri = resolve_relative_uri(item_uri, target)?;
            Some(CallHierarchyOutgoingCall {
                to: file_item(resolved_uri, workspace),
                from_ranges: vec![*range],
            })
        })
        .collect();

    if calls.is_empty() { None } else { Some(calls) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;

    #[test]
    fn test_prepare_on_include_directive() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///docs/main.adoc".parse::<Uri>()?;
        let content = "= Main\n\ninclude::chapter.adoc[]\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("doc not found")?;

        // Position on the include target (line 2, character 9 = start of "chapter.adoc")
        let position = Position {
            line: 2,
            character: 9,
        };
        let result = prepare_call_hierarchy(&doc, &uri, position, &workspace);
        assert!(result.is_some());
        let items = result.ok_or("expected items")?;
        assert_eq!(items.len(), 1);
        let item = items.first().ok_or("expected at least one item")?;
        assert!(
            item.uri.as_str().ends_with("chapter.adoc"),
            "expected chapter.adoc URI, got: {}",
            item.uri.as_str()
        );
        assert_eq!(item.kind, SymbolKind::FILE);
        Ok(())
    }

    #[test]
    fn test_prepare_fallback_to_current_document() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///docs/main.adoc".parse::<Uri>()?;
        let content = "= Main\n\nSome regular text.\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("doc not found")?;

        let position = Position {
            line: 0,
            character: 0,
        };
        let result = prepare_call_hierarchy(&doc, &uri, position, &workspace);
        assert!(result.is_some());
        let items = result.ok_or("expected items")?;
        assert_eq!(items.len(), 1);
        let item = items.first().ok_or("expected at least one item")?;
        assert_eq!(item.uri, uri);
        Ok(())
    }

    #[test]
    fn test_outgoing_calls() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///docs/main.adoc".parse::<Uri>()?;
        let content = "= Main\n\ninclude::a.adoc[]\n\ninclude::b.adoc[]\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        let item = file_item(uri, &workspace);
        let result = outgoing_calls(&item, &workspace);
        assert!(result.is_some());
        let calls = result.ok_or("expected calls")?;
        assert_eq!(calls.len(), 2);

        let targets: Vec<&str> = calls.iter().map(|c| c.to.uri.as_str()).collect();
        assert!(
            targets.iter().any(|t| t.ends_with("a.adoc")),
            "missing a.adoc in {targets:?}"
        );
        assert!(
            targets.iter().any(|t| t.ends_with("b.adoc")),
            "missing b.adoc in {targets:?}"
        );
        Ok(())
    }

    #[test]
    fn test_incoming_calls_from_open_documents() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let main_uri = "file:///docs/main.adoc".parse::<Uri>()?;
        let other_uri = "file:///docs/other.adoc".parse::<Uri>()?;
        let shared_uri = "file:///docs/shared.adoc".parse::<Uri>()?;

        workspace.update_document(
            main_uri,
            "= Main\n\ninclude::shared.adoc[]\n".to_string(),
            1,
        );
        workspace.update_document(
            other_uri,
            "= Other\n\ninclude::shared.adoc[]\n".to_string(),
            1,
        );
        workspace.update_document(shared_uri.clone(), "= Shared\n\nContent.\n".to_string(), 1);

        let item = file_item(shared_uri, &workspace);
        let result = incoming_calls(&item, &workspace);
        assert!(result.is_some());
        let calls = result.ok_or("expected calls")?;
        assert_eq!(calls.len(), 2);
        Ok(())
    }

    #[test]
    fn test_incoming_calls_multiple_ranges() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///docs/main.adoc".parse::<Uri>()?;
        let shared_uri = "file:///docs/shared.adoc".parse::<Uri>()?;

        // Same file included twice
        workspace.update_document(
            uri,
            "= Main\n\ninclude::shared.adoc[]\n\nMore text.\n\ninclude::shared.adoc[]\n"
                .to_string(),
            1,
        );
        workspace.update_document(shared_uri.clone(), "= Shared\n".to_string(), 1);

        let item = file_item(shared_uri, &workspace);
        let result = incoming_calls(&item, &workspace);
        assert!(result.is_some());
        let calls = result.ok_or("expected calls")?;
        assert_eq!(calls.len(), 1, "should be grouped into one caller");
        let call = calls.first().ok_or("expected at least one call")?;
        assert_eq!(call.from_ranges.len(), 2, "should have two from_ranges");
        Ok(())
    }

    #[test]
    fn test_no_outgoing_calls() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///docs/main.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), "= Main\n\nNo includes here.\n".to_string(), 1);

        let item = file_item(uri, &workspace);
        let result = outgoing_calls(&item, &workspace);
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_no_incoming_calls() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = "file:///docs/lonely.adoc".parse::<Uri>()?;
        workspace.update_document(
            uri.clone(),
            "= Lonely\n\nNobody includes me.\n".to_string(),
            1,
        );

        let item = file_item(uri, &workspace);
        let result = incoming_calls(&item, &workspace);
        assert!(result.is_none());
        Ok(())
    }
}
