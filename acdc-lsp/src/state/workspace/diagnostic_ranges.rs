use std::{error::Error, sync::Arc};

use acdc_parser::{Location, SourceLocation};
use tower_lsp_server::ls_types::{Diagnostic, Position, Range};

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
    let mut state = DocumentState::new_failure("first\r\né😀x".into(), 1, vec![]);
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
        diagnostics::error_to_diagnostic(&error),
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
        Diagnostic::default(),
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
    assert_eq!(symbols::document_symbols(parsed.document(), ""), []);
    Ok(())
}
