use std::error::Error;

use tower_lsp_server::ls_types::{
    CodeActionContext, CodeActionOrCommand, FileRename, FormattingOptions, Position, Range,
    TextEdit,
};

use super::{
    Workspace,
    source_locations::{file_uri, options_with_sources},
};
use crate::{
    capabilities::{
        call_hierarchy, code_actions, completion, file_rename, formatting, on_type_formatting,
    },
    convert::position_to_offset,
};

type TestResult = Result<(), Box<dyn Error>>;

fn apply_edits(text: &str, edits: &[TextEdit]) -> Result<String, Box<dyn Error>> {
    let mut edits: Vec<_> = edits
        .iter()
        .map(|edit| {
            Ok((
                position_to_offset(text, edit.range.start).ok_or("invalid edit start")?,
                position_to_offset(text, edit.range.end).ok_or("invalid edit end")?,
                &edit.new_text,
            ))
        })
        .collect::<Result<_, Box<dyn Error>>>()?;
    edits.sort_by_key(|&(start, end, _)| (start, end));
    for pair in edits.windows(2) {
        let first = pair.first().ok_or("first edit")?;
        let next = pair.get(1).ok_or("second edit")?;
        assert!(first.1 <= next.0, "overlapping edits: {edits:?}");
    }
    let mut result = text.to_owned();
    for (start, end, replacement) in edits.into_iter().rev() {
        result.replace_range(start..end, replacement);
    }
    Ok(result)
}

#[test]
fn formatting_edits_apply_to_unicode_crlf_and_eof_without_overlap() -> TestResult {
    for (text, expected) in [
        ("é😀  \r\n\r\n\r\n", "é😀\r\n"),
        ("é😀  ", "é😀\n"),
        ("Text\n\n  ", "Text\n"),
        ("----\n  ", "----\n  \n"),
        ("\r\n\r\n", "\r\n"),
        ("", "\n"),
    ] {
        let workspace = Workspace::new();
        let uri = file_uri("source-edits-format.adoc")?;
        workspace.update_document(uri.clone(), text.into(), 1);
        let doc = workspace.get_document(&uri).ok_or("document")?;
        let edits = formatting::format_document(&doc, &FormattingOptions::default());
        assert_eq!(apply_edits(text, &edits)?, expected, "{text:?}: {edits:?}");
        drop(doc);
        workspace.update_document(uri.clone(), expected.into(), 2);
        let doc = workspace.get_document(&uri).ok_or("formatted document")?;
        assert_eq!(
            formatting::format_document(&doc, &FormattingOptions::default()),
            [],
            "formatting must be idempotent for {text:?}"
        );
    }
    Ok(())
}

#[test]
fn formatting_rejects_invalid_ranges_and_preserves_crlf_on_enter() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("source-edits-range.adoc")?;
    workspace.update_document(uri.clone(), "😀  \r\n".into(), 1);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    for range in [
        Range::new(Position::new(0, 1), Position::new(0, 4)),
        Range::new(Position::new(1, 0), Position::new(0, 0)),
        Range::new(Position::new(0, 0), Position::new(8, 0)),
    ] {
        assert_eq!(
            formatting::format_range(&doc, &range, &FormattingOptions::default()),
            []
        );
    }
    drop(doc);
    workspace.update_document(uri.clone(), "----\r\n".into(), 2);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let edits =
        on_type_formatting::format_on_type(&doc, Position::new(1, 0), "\n").ok_or("block close")?;
    assert_eq!(apply_edits(doc.text(), &edits)?, "----\r\n\r\n----\r\n");
    drop(doc);
    workspace.update_document(uri.clone(), "* Item\n😀".into(), 3);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    assert!(on_type_formatting::format_on_type(&doc, Position::new(1, 1), "\n").is_none());
    Ok(())
}

#[test]
fn wrapping_preserves_original_unicode_and_crlf_selection() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("source-edits-wrap.adoc")?;
    let text = "😀 text\r\nnext";
    workspace.update_document(uri.clone(), text.into(), 1);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let context = CodeActionContext {
        diagnostics: vec![],
        only: None,
        trigger_kind: None,
    };
    let actions = code_actions::compute_code_actions(
        &doc,
        &uri,
        Range::new(Position::new(0, 0), Position::new(1, 4)),
        &context,
    );
    let action = actions
        .iter()
        .find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) if action.title == "Wrap in listing block" => {
                Some(action)
            }
            CodeActionOrCommand::Command(_) | CodeActionOrCommand::CodeAction(_) => None,
        })
        .ok_or("wrap action")?;
    let edits = action
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.as_ref())
        .and_then(|changes| changes.get(&uri))
        .ok_or("wrap edits")?;
    assert_eq!(
        apply_edits(text, edits)?,
        "----\r\n😀 text\r\nnext\r\n----\r\n"
    );
    assert_eq!(
        code_actions::compute_code_actions(
            &doc,
            &uri,
            Range::new(Position::new(0, 1), Position::new(0, 2)),
            &context
        ),
        []
    );
    Ok(())
}

#[test]
fn completion_uses_checked_utf16_cursors() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("source-edits-completion.adoc")?;
    workspace.update_document(uri.clone(), "😀 {auth}\r\n".into(), 1);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    assert!(
        completion::compute_completions(&doc, &uri, &workspace, Position::new(0, 8))
            .is_some_and(|items| items.iter().any(|item| item.label == "author"))
    );
    assert!(completion::compute_completions(&doc, &uri, &workspace, Position::new(0, 1)).is_none());
    assert!(completion::compute_completions(&doc, &uri, &workspace, Position::new(9, 0)).is_none());
    Ok(())
}

#[test]
fn include_rename_uses_unicode_target_and_excludes_opening_bracket() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("source-edits-book.adoc")?;
    let old = file_uri("😀.adoc")?;
    let new = file_uri("next.adoc")?;
    let text = "= Book\r\n\r\ninclude::😀.adoc[]\r\n";
    workspace.update_document(uri.clone(), text.into(), 1);
    let edits = file_rename::compute_file_rename_edits(
        &workspace,
        &[FileRename {
            old_uri: old.to_string(),
            new_uri: new.to_string(),
        }],
    )
    .and_then(|edit| edit.changes)
    .ok_or("file edits")?;
    assert_eq!(
        apply_edits(text, edits.get(&uri).ok_or("book edits")?)?,
        text.replace("😀.adoc", "next.adoc")
    );
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let (_, span) = doc.includes.first().ok_or("include")?;
    assert_eq!(
        text.get(span.absolute_start..=span.absolute_end),
        Some("😀.adoc")
    );
    let prepared =
        call_hierarchy::prepare_call_hierarchy(&doc, &uri, Position::new(2, 16), &workspace)
            .ok_or("call hierarchy")?;
    assert_eq!(prepared.first().ok_or("file item")?.uri, uri);
    Ok(())
}

#[test]
fn formatting_preserves_original_verbatim_content_around_selected_includes() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("source-edits-protected.adoc")?;
    let text = "= Book\r\n\r\n----\r\ninclude::child.adoc[lines=3..4,indent=0]\r\n😀 literal  \r\n----\r\n\r\n😀 prose  \r\n";
    workspace.update_document_with_options(
        uri.clone(),
        text.into(),
        1,
        &options_with_sources(&[(
            "child.adoc",
            "// omitted\r\n// omitted\r\n    included\r\n    text\r\n",
        )])?,
    );
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let edits = formatting::format_document(&doc, &FormattingOptions::default());
    assert_eq!(
        apply_edits(text, &edits)?,
        text.replace("😀 prose  ", "😀 prose")
    );
    Ok(())
}
