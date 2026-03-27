//! Code actions: quick-fixes, refactorings, and source actions
//!
//! This module provides code actions for `AsciiDoc` documents:
//! - Quick-fix: create missing anchor for unresolved cross-references
//! - Wrap in block: wrap selected text in delimited block (sidebar, example, etc.)
//! - TOC generation: insert `:toc:` attribute in document header

use std::collections::HashMap;

use tower_lsp_server::ls_types::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionOrCommand, Position, Range, TextEdit,
    Uri, WorkspaceEdit,
};

use crate::state::DocumentState;

/// Diagnostic message prefix for unresolved cross-references.
const UNRESOLVED_XREF_PREFIX: &str = "Unresolved cross-reference: target '";

/// Block types available for wrap-in-block refactoring.
const BLOCK_WRAPS: &[(&str, &str)] = &[
    ("sidebar", "****"),
    ("example", "===="),
    ("listing", "----"),
    ("literal", "...."),
    ("open", "--"),
    ("comment", "////"),
    ("passthrough", "++++"),
    ("quote", "____"),
];

/// Compute all available code actions for the given document and context.
#[must_use]
pub fn compute_code_actions(
    doc: &DocumentState,
    uri: &Uri,
    range: Range,
    ctx: &CodeActionContext,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    actions.extend(
        quickfix_actions(doc, uri, ctx)
            .into_iter()
            .map(CodeActionOrCommand::CodeAction),
    );

    if range.start != range.end {
        actions.extend(
            wrap_in_block_actions(doc, uri, range)
                .into_iter()
                .map(CodeActionOrCommand::CodeAction),
        );
    }

    actions.extend(
        toc_actions(doc, uri)
            .into_iter()
            .map(CodeActionOrCommand::CodeAction),
    );

    actions
}

/// Generate quick-fix actions from diagnostics in the context.
///
/// Currently handles:
/// - Unresolved xref: create anchor at nearest section heading
fn quickfix_actions(doc: &DocumentState, uri: &Uri, ctx: &CodeActionContext) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    for diagnostic in &ctx.diagnostics {
        if let Some(anchor_id) = diagnostic
            .message
            .strip_prefix(UNRESOLVED_XREF_PREFIX)
            .and_then(|rest| rest.strip_suffix("' not found"))
        {
            let insert_line = find_insertion_line_before(doc, diagnostic.range.start.line);
            let insert_pos = Position {
                line: insert_line,
                character: 0,
            };

            let mut changes = HashMap::new();
            changes.insert(
                uri.clone(),
                vec![TextEdit {
                    range: Range {
                        start: insert_pos,
                        end: insert_pos,
                    },
                    new_text: format!("[[{anchor_id}]]\n"),
                }],
            );

            actions.push(CodeAction {
                title: format!("Create anchor '{anchor_id}'"),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                }),
                ..Default::default()
            });
        }
    }

    actions
}

/// Find the best line to insert an anchor before, searching forward up to `before_line`.
///
/// Keeps track of the last section heading (`== ...`) seen at or before the given line.
/// Falls back to line 0 (top of document).
#[allow(clippy::cast_possible_truncation)]
fn find_insertion_line_before(doc: &DocumentState, before_line: u32) -> u32 {
    let before = before_line as usize;
    let mut best: Option<usize> = None;

    for (idx, line) in doc.text.lines().enumerate() {
        if idx > before {
            break;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("== ")
            || trimmed.starts_with("=== ")
            || trimmed.starts_with("==== ")
            || trimmed.starts_with("===== ")
            || trimmed.starts_with("====== ")
        {
            best = Some(idx);
        }
    }

    best.map_or(0, |line| line as u32)
}

/// Generate wrap-in-block refactoring actions for a non-empty selection.
fn wrap_in_block_actions(doc: &DocumentState, uri: &Uri, range: Range) -> Vec<CodeAction> {
    let Some(selected_text) = extract_text_for_range(&doc.text, &range) else {
        return Vec::new();
    };

    BLOCK_WRAPS
        .iter()
        .map(|(name, delimiter)| {
            let new_text = format!("{delimiter}\n{selected_text}\n{delimiter}\n");

            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);

            CodeAction {
                title: format!("Wrap in {name} block"),
                kind: Some(CodeActionKind::REFACTOR_EXTRACT),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                }),
                ..Default::default()
            }
        })
        .collect()
}

/// Extract text from source corresponding to an LSP range.
///
/// Converts 0-indexed LSP line/character positions to source text offsets.
#[allow(clippy::cast_possible_truncation)]
fn extract_text_for_range(source: &str, range: &Range) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();

    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;

    if start_line >= lines.len() {
        return None;
    }

    let end_line = end_line.min(lines.len().saturating_sub(1));

    if start_line == end_line {
        let line = lines.get(start_line)?;
        let start_char = char_offset_to_byte(line, range.start.character as usize);
        let end_char = char_offset_to_byte(line, range.end.character as usize);
        Some(line.get(start_char..end_char)?.to_string())
    } else {
        let mut result = String::new();

        // First line from start character
        let first_line = lines.get(start_line)?;
        let start_byte = char_offset_to_byte(first_line, range.start.character as usize);
        result.push_str(first_line.get(start_byte..)?);

        // Middle lines (full)
        for line_idx in (start_line + 1)..end_line {
            result.push('\n');
            if let Some(line) = lines.get(line_idx) {
                result.push_str(line);
            }
        }

        // Last line up to end character
        result.push('\n');
        let last_line = lines.get(end_line)?;
        let end_byte = char_offset_to_byte(last_line, range.end.character as usize);
        result.push_str(last_line.get(..end_byte)?);

        Some(result)
    }
}

/// Convert a character offset (UTF-16 code units in LSP) to byte offset.
fn char_offset_to_byte(line: &str, char_offset: usize) -> usize {
    line.chars().take(char_offset).map(char::len_utf8).sum()
}

/// Generate TOC-related source actions.
///
/// Offers "Generate table of contents" if the document has no `:toc:` attribute.
fn toc_actions(doc: &DocumentState, uri: &Uri) -> Vec<CodeAction> {
    let has_toc = doc.text.lines().any(|line| line.starts_with(":toc:"));

    if has_toc {
        return Vec::new();
    }

    // Only offer TOC if document has sections
    let has_sections = doc.text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("== ") || trimmed.starts_with("=== ")
    });

    if !has_sections {
        return Vec::new();
    }

    let insert_line = find_header_end(doc);
    let insert_pos = Position {
        line: insert_line,
        character: 0,
    };

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: insert_pos,
                end: insert_pos,
            },
            new_text: ":toc:\n".to_string(),
        }],
    );

    vec![CodeAction {
        title: "Generate table of contents".to_string(),
        kind: Some(CodeActionKind::SOURCE),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        ..Default::default()
    }]
}

/// Find the line after the document header where `:toc:` should be inserted.
///
/// Scans for the title line (`= Title`) and any header attributes (`:key: value`),
/// then returns the line after the last header attribute (or after the title).
#[allow(clippy::cast_possible_truncation)]
fn find_header_end(doc: &DocumentState) -> u32 {
    let mut last_header_line: Option<usize> = None;

    for (idx, line) in doc.text.lines().enumerate() {
        if idx == 0 && line.starts_with("= ") {
            last_header_line = Some(idx);
            continue;
        }

        if last_header_line.is_some() {
            if line.starts_with(':') {
                last_header_line = Some(idx);
            } else {
                break;
            }
        }
    }

    last_header_line.map_or(0, |line| (line + 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;
    use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity};

    fn make_diagnostic(message: &str, line: u32, start_char: u32, end_char: u32) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line,
                    character: start_char,
                },
                end: Position {
                    line,
                    character: end_char,
                },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("acdc".to_string()),
            message: message.to_string(),
            ..Default::default()
        }
    }

    // --- Quick-fix tests ---

    #[test]
    fn test_quickfix_for_unresolved_xref() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\n== Section\n\nSee <<missing-anchor>>.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let diag = make_diagnostic(
            "Unresolved cross-reference: target 'missing-anchor' not found",
            4,
            4,
            22,
        );
        let ctx = CodeActionContext {
            diagnostics: vec![diag],
            only: None,
            trigger_kind: None,
        };

        let actions = quickfix_actions(&doc, &uri, &ctx);
        assert_eq!(actions.len(), 1);

        let action = actions.first().ok_or("expected action")?;
        assert_eq!(action.title, "Create anchor 'missing-anchor'");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));

        let edit = action.edit.as_ref().ok_or("expected edit")?;
        let changes = edit.changes.as_ref().ok_or("expected changes")?;
        let edits = changes.get(&uri).ok_or("expected edits for URI")?;
        assert_eq!(edits.len(), 1);
        let text_edit = edits.first().ok_or("expected text edit")?;
        assert_eq!(text_edit.new_text, "[[missing-anchor]]\n");
        // Should insert before "== Section" (line 2)
        assert_eq!(text_edit.range.start.line, 2);
        Ok(())
    }

    #[test]
    fn test_no_quickfix_when_no_unresolved_xref() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\n== Section\n\nJust text.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let ctx = CodeActionContext {
            diagnostics: vec![],
            only: None,
            trigger_kind: None,
        };

        let actions = quickfix_actions(&doc, &uri, &ctx);
        assert!(actions.is_empty());
        Ok(())
    }

    #[test]
    fn test_quickfix_anchor_id_extraction() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\nSee <<my-id>>.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let diag = make_diagnostic(
            "Unresolved cross-reference: target 'my-id' not found",
            2,
            4,
            11,
        );
        let ctx = CodeActionContext {
            diagnostics: vec![diag],
            only: None,
            trigger_kind: None,
        };

        let actions = quickfix_actions(&doc, &uri, &ctx);
        assert_eq!(actions.len(), 1);

        let action = actions.first().ok_or("expected action")?;
        assert_eq!(action.title, "Create anchor 'my-id'");
        Ok(())
    }

    #[test]
    fn test_quickfix_inserts_at_top_when_no_sections() -> Result<(), Box<dyn std::error::Error>> {
        let src = "Some text.\n\nSee <<missing>>.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let diag = make_diagnostic(
            "Unresolved cross-reference: target 'missing' not found",
            2,
            4,
            15,
        );
        let ctx = CodeActionContext {
            diagnostics: vec![diag],
            only: None,
            trigger_kind: None,
        };

        let actions = quickfix_actions(&doc, &uri, &ctx);
        assert_eq!(actions.len(), 1);

        let edit = actions
            .first()
            .ok_or("no action")?
            .edit
            .as_ref()
            .ok_or("no edit")?;
        let changes = edit.changes.as_ref().ok_or("no changes")?;
        let edits = changes.get(&uri).ok_or("no edits for URI")?;
        let text_edit = edits.first().ok_or("no text edit")?;
        assert_eq!(text_edit.range.start.line, 0);
        Ok(())
    }

    // --- Wrap-in-block tests ---

    #[test]
    fn test_wrap_in_sidebar_block() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\nSome paragraph text.\n\nMore text.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 20,
            },
        };

        let actions = wrap_in_block_actions(&doc, &uri, range);
        let sidebar = actions
            .iter()
            .find(|a| a.title == "Wrap in sidebar block")
            .ok_or("expected sidebar action")?;

        let edit = sidebar.edit.as_ref().ok_or("expected edit")?;
        let changes = edit.changes.as_ref().ok_or("expected changes")?;
        let edits = changes.get(&uri).ok_or("expected edits")?;
        let text_edit = edits.first().ok_or("expected text edit")?;
        assert_eq!(text_edit.new_text, "****\nSome paragraph text.\n****\n");
        Ok(())
    }

    #[test]
    fn test_all_block_types_offered() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\nSome text here.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 15,
            },
        };

        let actions = wrap_in_block_actions(&doc, &uri, range);
        assert_eq!(actions.len(), 8);

        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert!(titles.contains(&"Wrap in sidebar block"));
        assert!(titles.contains(&"Wrap in example block"));
        assert!(titles.contains(&"Wrap in listing block"));
        assert!(titles.contains(&"Wrap in literal block"));
        assert!(titles.contains(&"Wrap in open block"));
        assert!(titles.contains(&"Wrap in comment block"));
        assert!(titles.contains(&"Wrap in passthrough block"));
        assert!(titles.contains(&"Wrap in quote block"));
        Ok(())
    }

    #[test]
    fn test_no_wrap_for_empty_selection() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\nSome text.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let range = Range {
            start: Position {
                line: 2,
                character: 5,
            },
            end: Position {
                line: 2,
                character: 5,
            },
        };

        let ctx = CodeActionContext {
            diagnostics: vec![],
            only: None,
            trigger_kind: None,
        };

        let actions = compute_code_actions(&doc, &uri, range, &ctx);

        let wrap_count = actions
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    CodeActionOrCommand::CodeAction(ca)
                        if ca.kind == Some(CodeActionKind::REFACTOR_EXTRACT)
                )
            })
            .count();
        assert_eq!(wrap_count, 0);
        Ok(())
    }

    #[test]
    fn test_wrap_multiline_selection() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\nLine one.\nLine two.\nLine three.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 4,
                character: 11,
            },
        };

        let actions = wrap_in_block_actions(&doc, &uri, range);
        let listing = actions
            .iter()
            .find(|a| a.title == "Wrap in listing block")
            .ok_or("expected listing action")?;

        let edit = listing.edit.as_ref().ok_or("expected edit")?;
        let changes = edit.changes.as_ref().ok_or("expected changes")?;
        let edits = changes.get(&uri).ok_or("expected edits")?;
        let text_edit = edits.first().ok_or("expected text edit")?;
        assert_eq!(
            text_edit.new_text,
            "----\nLine one.\nLine two.\nLine three.\n----\n"
        );
        Ok(())
    }

    // --- TOC tests ---

    #[test]
    fn test_toc_generation_offered() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document Title\n:author: Test\n\n== Section One\n\n== Section Two\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let actions = toc_actions(&doc, &uri);
        assert_eq!(actions.len(), 1);

        let action = actions.first().ok_or("expected action")?;
        assert_eq!(action.title, "Generate table of contents");
        assert_eq!(action.kind, Some(CodeActionKind::SOURCE));

        let edit = action.edit.as_ref().ok_or("expected edit")?;
        let changes = edit.changes.as_ref().ok_or("expected changes")?;
        let edits = changes.get(&uri).ok_or("expected edits")?;
        let text_edit = edits.first().ok_or("expected text edit")?;
        assert_eq!(text_edit.new_text, ":toc:\n");
        assert_eq!(text_edit.range.start.line, 2);
        Ok(())
    }

    #[test]
    fn test_no_toc_when_already_present() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n:toc:\n\n== Section\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let actions = toc_actions(&doc, &uri);
        assert!(actions.is_empty());
        Ok(())
    }

    #[test]
    fn test_no_toc_when_no_sections() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\nJust a paragraph.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let actions = toc_actions(&doc, &uri);
        assert!(actions.is_empty());
        Ok(())
    }

    #[test]
    fn test_toc_insertion_at_top_without_title() -> Result<(), Box<dyn std::error::Error>> {
        let src = "== Section One\n\nSome text.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let actions = toc_actions(&doc, &uri);
        assert_eq!(actions.len(), 1);

        let edit = actions
            .first()
            .ok_or("no action")?
            .edit
            .as_ref()
            .ok_or("no edit")?;
        let changes = edit.changes.as_ref().ok_or("no changes")?;
        let edits = changes.get(&uri).ok_or("no edits")?;
        let text_edit = edits.first().ok_or("no text edit")?;
        assert_eq!(text_edit.range.start.line, 0);
        Ok(())
    }

    // --- Dispatcher test ---

    #[test]
    fn test_compute_code_actions_combines_all() -> Result<(), Box<dyn std::error::Error>> {
        let src = "= Document\n\n== Section\n\nSee <<missing>>.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let diag = make_diagnostic(
            "Unresolved cross-reference: target 'missing' not found",
            4,
            4,
            15,
        );

        let range = Range {
            start: Position {
                line: 4,
                character: 0,
            },
            end: Position {
                line: 4,
                character: 18,
            },
        };

        let ctx = CodeActionContext {
            diagnostics: vec![diag],
            only: None,
            trigger_kind: None,
        };

        let actions = compute_code_actions(&doc, &uri, range, &ctx);

        // 1 quickfix + 8 wrap + 1 toc = 10
        assert_eq!(actions.len(), 10);

        let quickfix_count = actions
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    CodeActionOrCommand::CodeAction(ca)
                        if ca.kind == Some(CodeActionKind::QUICKFIX)
                )
            })
            .count();
        let refactor_count = actions
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    CodeActionOrCommand::CodeAction(ca)
                        if ca.kind == Some(CodeActionKind::REFACTOR_EXTRACT)
                )
            })
            .count();
        let source_count = actions
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    CodeActionOrCommand::CodeAction(ca)
                        if ca.kind == Some(CodeActionKind::SOURCE)
                )
            })
            .count();

        assert_eq!(quickfix_count, 1);
        assert_eq!(refactor_count, 8);
        assert_eq!(source_count, 1);
        Ok(())
    }
}
