//! Call Hierarchy: show include-tree relationships between `AsciiDoc` documents
//!
//! Maps `include::` directives to the LSP Call Hierarchy protocol:
//! - "Outgoing calls" = documents this file includes
//! - "Incoming calls" = documents that include this file

use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, Range,
    SymbolKind, Url,
};

use crate::convert::{
    location_to_range, offset_in_location, position_to_offset, resolve_relative_uri,
};
use crate::state::{DocumentState, Workspace, extract_includes};

/// Build a `CallHierarchyItem` representing an `AsciiDoc` file.
fn make_call_hierarchy_item(uri: Url, line_count: usize) -> CallHierarchyItem {
    let name = uri
        .path_segments()
        .and_then(|mut s| s.next_back().map(String::from))
        .unwrap_or_else(|| uri.to_string());

    let end_line: u32 = line_count.saturating_sub(1).try_into().unwrap_or(u32::MAX);
    let range = Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: end_line,
            character: 0,
        },
    };

    CallHierarchyItem {
        name,
        kind: SymbolKind::FILE,
        tags: None,
        detail: None,
        uri,
        range,
        selection_range: range,
        data: None,
    }
}

/// Count lines in a file on disk, returning 0 if the file can't be read.
fn line_count_from_disk(uri: &Url) -> usize {
    uri.to_file_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map_or(0, |t| t.lines().count())
}

/// Prepare call hierarchy items at the given position.
///
/// If the cursor is on an `include::` directive, returns an item for the
/// included file. Otherwise returns an item for the current document.
#[must_use]
pub fn prepare_call_hierarchy(
    doc: &DocumentState,
    doc_uri: &Url,
    position: Position,
) -> Option<Vec<CallHierarchyItem>> {
    let offset = position_to_offset(&doc.text, position)?;

    // Check if cursor is on an include directive
    for (target, location) in &doc.includes {
        if offset_in_location(offset, location) {
            let resolved_uri = resolve_relative_uri(doc_uri, target)?;
            let line_count = line_count_from_disk(&resolved_uri);
            return Some(vec![make_call_hierarchy_item(resolved_uri, line_count)]);
        }
    }

    // Fallback: return an item for the current document
    let line_count = doc.text.lines().count();
    Some(vec![make_call_hierarchy_item(doc_uri.clone(), line_count)])
}

/// Find all documents that include the given file (incoming calls).
#[must_use]
pub fn incoming_calls(
    item: &CallHierarchyItem,
    workspace: &Workspace,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let target_uri = &item.uri;
    let mut callers: HashMap<Url, Vec<Range>> = HashMap::new();

    // Scan open documents
    workspace.for_each_document(|doc_uri, doc_state| {
        let mut ranges = Vec::new();
        for (include_target, location) in &doc_state.includes {
            if let Some(resolved) = resolve_relative_uri(doc_uri, include_target)
                && &resolved == target_uri
            {
                ranges.push(location_to_range(location));
            }
        }
        if !ranges.is_empty() {
            callers.entry(doc_uri.clone()).or_default().extend(ranges);
        }
    });

    // Scan non-open workspace files
    for path in workspace.discover_workspace_files() {
        let Ok(file_uri) = Url::from_file_path(&path) else {
            continue;
        };
        if workspace.has_document(&file_uri) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let includes = extract_includes(&text);
        let mut ranges = Vec::new();
        for (include_target, location) in &includes {
            if let Some(resolved) = resolve_relative_uri(&file_uri, include_target)
                && &resolved == target_uri
            {
                ranges.push(location_to_range(location));
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
        .map(|(uri, from_ranges)| {
            let line_count = if let Some(doc) = workspace.get_document(&uri) {
                doc.text.lines().count()
            } else {
                line_count_from_disk(&uri)
            };
            CallHierarchyIncomingCall {
                from: make_call_hierarchy_item(uri, line_count),
                from_ranges,
            }
        })
        .collect();

    Some(calls)
}

/// Find all documents that the given file includes (outgoing calls).
#[must_use]
pub fn outgoing_calls(
    item: &CallHierarchyItem,
    workspace: &Workspace,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    let item_uri = &item.uri;

    // Get includes — from open document or from disk
    let includes = if let Some(doc) = workspace.get_document(item_uri) {
        doc.includes.clone()
    } else {
        let path = item_uri.to_file_path().ok()?;
        let text = std::fs::read_to_string(path).ok()?;
        extract_includes(&text)
    };

    if includes.is_empty() {
        return None;
    }

    let calls: Vec<CallHierarchyOutgoingCall> = includes
        .iter()
        .filter_map(|(target, location)| {
            let resolved_uri = resolve_relative_uri(item_uri, target)?;
            let line_count = if let Some(doc) = workspace.get_document(&resolved_uri) {
                doc.text.lines().count()
            } else {
                line_count_from_disk(&resolved_uri)
            };
            Some(CallHierarchyOutgoingCall {
                to: make_call_hierarchy_item(resolved_uri, line_count),
                from_ranges: vec![location_to_range(location)],
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
        let uri = Url::parse("file:///docs/main.adoc")?;
        let content = "= Main\n\ninclude::chapter.adoc[]\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("doc not found")?;

        // Position on the include target (line 2, character 9 = start of "chapter.adoc")
        let position = Position {
            line: 2,
            character: 9,
        };
        let result = prepare_call_hierarchy(&doc, &uri, position);
        assert!(result.is_some());
        let items = result.ok_or("expected items")?;
        assert_eq!(items.len(), 1);
        let item = items.first().ok_or("expected at least one item")?;
        assert!(
            item.uri.as_str().ends_with("chapter.adoc"),
            "expected chapter.adoc URI, got: {}",
            item.uri
        );
        assert_eq!(item.kind, SymbolKind::FILE);
        Ok(())
    }

    #[test]
    fn test_prepare_fallback_to_current_document() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = Url::parse("file:///docs/main.adoc")?;
        let content = "= Main\n\nSome regular text.\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("doc not found")?;

        let position = Position {
            line: 0,
            character: 0,
        };
        let result = prepare_call_hierarchy(&doc, &uri, position);
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
        let uri = Url::parse("file:///docs/main.adoc")?;
        let content = "= Main\n\ninclude::a.adoc[]\n\ninclude::b.adoc[]\n";
        workspace.update_document(uri.clone(), content.to_string(), 1);

        let item = make_call_hierarchy_item(uri, 5);
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
        let main_uri = Url::parse("file:///docs/main.adoc")?;
        let other_uri = Url::parse("file:///docs/other.adoc")?;
        let shared_uri = Url::parse("file:///docs/shared.adoc")?;

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

        let item = make_call_hierarchy_item(shared_uri, 3);
        let result = incoming_calls(&item, &workspace);
        assert!(result.is_some());
        let calls = result.ok_or("expected calls")?;
        assert_eq!(calls.len(), 2);
        Ok(())
    }

    #[test]
    fn test_incoming_calls_multiple_ranges() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = Url::parse("file:///docs/main.adoc")?;
        let shared_uri = Url::parse("file:///docs/shared.adoc")?;

        // Same file included twice
        workspace.update_document(
            uri,
            "= Main\n\ninclude::shared.adoc[]\n\nMore text.\n\ninclude::shared.adoc[]\n"
                .to_string(),
            1,
        );
        workspace.update_document(shared_uri.clone(), "= Shared\n".to_string(), 1);

        let item = make_call_hierarchy_item(shared_uri, 1);
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
        let uri = Url::parse("file:///docs/main.adoc")?;
        workspace.update_document(uri.clone(), "= Main\n\nNo includes here.\n".to_string(), 1);

        let item = make_call_hierarchy_item(uri, 3);
        let result = outgoing_calls(&item, &workspace);
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_no_incoming_calls() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri = Url::parse("file:///docs/lonely.adoc")?;
        workspace.update_document(
            uri.clone(),
            "= Lonely\n\nNobody includes me.\n".to_string(),
            1,
        );

        let item = make_call_hierarchy_item(uri, 3);
        let result = incoming_calls(&item, &workspace);
        assert!(result.is_none());
        Ok(())
    }
}
