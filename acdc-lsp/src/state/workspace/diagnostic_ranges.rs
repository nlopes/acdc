use std::{error::Error, sync::Arc};

use acdc_parser::{Location, SourceLocation};
use tower_lsp_server::ls_types::{DiagnosticSeverity, Position, Range};

use super::{
    Workspace,
    source_locations::{file_uri, options_with_sources},
};
use crate::{
    capabilities::{diagnostics, symbols},
    state::DocumentState,
};

#[test]
fn diagnostics_do_not_guess_unavailable_source_ranges() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("unavailable-diagnostics.adoc")?;
    let child = file_uri("unavailable-child.adoc")?;
    workspace.update_document_with_options(
        uri.clone(),
        "include::unavailable-child.adoc[]\n".into(),
        1,
        &options_with_sources(&[("unavailable-child.adoc", "😀 <<missing>>\n")])?,
    );
    assert_eq!(workspace.diagnostics_for(&child).0, []);
    assert!(
        !workspace
            .diagnostics_for(&uri)
            .0
            .iter()
            .any(|diagnostic| diagnostic.message.contains("missing"))
    );
    Ok(())
}

#[test]
fn diagnostics_use_original_positions_and_cross_file_spans_mark_the_start()
-> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("diagnostic-points.adoc")?;
    let mut state = DocumentState::new_failure("first\r\né😀x".into(), 1);
    let mut location = Location::default();
    location.start = acdc_parser::Position::new(2, 3);
    location.end = acdc_parser::Position::new(2, 3);
    location.absolute_start = usize::MAX;
    location.absolute_end = usize::MAX;
    let source = SourceLocation::at_location(None, location.clone());
    let error = acdc_parser::Error::InvalidIncludeDirective(
        Box::new(source.clone()),
        "invalid include".into(),
    );
    workspace.push_diagnostic(
        &mut state,
        &uri,
        Some(source),
        diagnostics::error_message(&error),
    );
    assert_eq!(
        state.diagnostics.first().ok_or("error diagnostic")?.range,
        Range::new(Position::new(1, 3), Position::new(1, 4))
    );
    location.end.file = Some(Arc::new(vec!["other.adoc".into()]));
    workspace.push_diagnostic(
        &mut state,
        &uri,
        Some(SourceLocation::at_location(None, location)),
        diagnostics::DiagnosticMessage::new("point".into(), DiagnosticSeverity::ERROR),
    );
    assert_eq!(
        state.diagnostics.get(1).ok_or("point diagnostic")?.range,
        Range::new(Position::new(1, 3), Position::new(1, 3))
    );
    Ok(())
}

#[test]
fn parser_warnings_use_unsaved_included_text() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("warning-parent.adoc")?;
    let child = file_uri("warning-child.adoc")?;
    let text = "=== 😀 Skipped\r\n";
    workspace.update_document(child.clone(), text.into(), 1);
    workspace.update_document_with_options(
        uri,
        "= Book\n\ninclude::warning-child.adoc[]\n".into(),
        1,
        &options_with_sources(&[("warning-child.adoc", text)])?,
    );
    let (warnings, _) = workspace.diagnostics_for(&child);
    let warning = warnings
        .iter()
        .find(|warning| warning.message.contains("Section level"))
        .ok_or("section warning")?;
    assert_eq!(
        warning.range,
        Range::new(Position::new(0, 0), Position::new(0, 14))
    );
    Ok(())
}

#[test]
fn outlines_omit_ranges_that_cannot_be_read_from_original_source() -> Result<(), Box<dyn Error>> {
    let parsed = acdc_parser::parse("== Heading\n\nBody.\n", &acdc_parser::Options::default())?;
    assert_eq!(symbols::document_symbols(parsed.document_ref(), ""), []);
    Ok(())
}

#[test]
fn retained_messages_resolve_ranges_each_time_they_are_published() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("retained-diagnostics.adoc")?;
    let mut state = DocumentState::new_failure("é😀\r\n".into(), 1);
    let mut location = Location::default();
    location.start = acdc_parser::Position::new(2, 1);
    location.end = acdc_parser::Position::new(2, 0);
    location.absolute_start = usize::MAX;
    location.absolute_end = usize::MAX;
    state.parse_diagnostics = vec![
        (
            None,
            diagnostics::DiagnosticMessage::new("whole document".into(), DiagnosticSeverity::ERROR),
        ),
        (
            Some(SourceLocation::at_location(None, location.clone())),
            diagnostics::DiagnosticMessage::new("EOF".into(), DiagnosticSeverity::WARNING),
        ),
    ];
    location.end = acdc_parser::Position::new(2, 9);
    state.parse_diagnostics.push((
        Some(SourceLocation::at_location(None, location)),
        diagnostics::DiagnosticMessage::new("invalid span".into(), DiagnosticSeverity::ERROR),
    ));
    for _ in 0..2 {
        workspace.compute_diagnostics(&uri, &mut state);
        assert_eq!(state.diagnostics.len(), 2);
        let whole = state
            .diagnostics
            .first()
            .ok_or("whole-document diagnostic")?;
        assert_eq!(whole.range, Range::default());
        assert_eq!(whole.message, "whole document");
        assert_eq!(whole.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(whole.source.as_deref(), Some("acdc"));
        let eof = state.diagnostics.get(1).ok_or("EOF diagnostic")?;
        assert_eq!(
            eof.range,
            Range::new(Position::new(1, 0), Position::new(1, 0))
        );
        assert_eq!(eof.message, "EOF");
        assert_eq!(eof.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(state.parse_diagnostics.len(), 3);
    }
    Ok(())
}

#[test]
fn anchor_identity_uses_original_coordinates_without_clamping() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("identity-source.adoc")?;
    let mut location = Location::default();
    location.start = acdc_parser::Position::new(1, 1);
    location.end = acdc_parser::Position::new(1, 4);
    let first = SourceLocation::at_location(None, location.clone());
    location.absolute_start = 999;
    location.absolute_end = 1002;
    let repeated = SourceLocation::at_location(None, location.clone());
    location.start.line = 0;
    location.start.column = 0;
    let invalid = SourceLocation::at_location(None, location);
    workspace.anchor_index.insert(
        "target".into(),
        vec![
            (uri.clone(), first),
            (uri.clone(), repeated),
            (uri.clone(), invalid),
        ],
    );
    assert_eq!(workspace.find_anchor_globally("target").len(), 2);
    Ok(())
}
