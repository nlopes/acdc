//! Diagnostics: convert acdc-parser errors to LSP diagnostics
//!
//! This module handles two types of diagnostics:
//! - Parse errors: converted from acdc-parser errors
//! - Validation warnings: unresolved xrefs, duplicate anchors, etc.

use std::collections::HashMap;
use std::hash::BuildHasher;

use acdc_parser::{Error, Location, Positioning};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::convert::{location_to_range, parser_position_to_lsp};

/// Convert acdc-parser Error to LSP Diagnostic
#[must_use]
pub fn error_to_diagnostic(error: &Error) -> Diagnostic {
    let range = error
        .source_location()
        .map(|source_loc| positioning_to_range(&source_loc.positioning))
        .unwrap_or_default();

    let message = error.to_string();

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("acdc".to_string()),
        message,
        ..Default::default()
    }
}

/// Convert acdc-parser Positioning to LSP Range
///
/// Note: acdc-parser uses 1-indexed lines/columns, LSP uses 0-indexed
fn positioning_to_range(pos: &Positioning) -> Range {
    match pos {
        Positioning::Location(loc) => location_to_range(loc),
        Positioning::Position(p) => {
            let lsp_pos = parser_position_to_lsp(p);
            Range {
                start: lsp_pos,
                end: lsp_pos,
            }
        }
    }
}

/// Compute validation warnings for a document.
///
/// Returns warnings for:
/// - Unresolved xref targets (target ID doesn't exist as an anchor)
/// - Duplicate anchor IDs
#[must_use]
pub fn compute_warnings<S: BuildHasher>(
    anchors: &HashMap<String, Location, S>,
    xrefs: &[(String, Location)],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Check for unresolved xrefs
    for (target, location) in xrefs {
        if !anchors.contains_key(target) {
            diagnostics.push(Diagnostic {
                range: location_to_range(location),
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("acdc".to_string()),
                message: format!("Unresolved cross-reference: target '{target}' not found"),
                ..Default::default()
            });
        }
    }

    diagnostics
}

/// Collect duplicate anchors and return warnings.
///
/// This should be called during anchor collection to detect duplicates.
#[must_use]
pub fn check_duplicate_anchors<S: BuildHasher>(
    anchor_id: &str,
    location: &Location,
    existing_anchors: &HashMap<String, Location, S>,
) -> Option<Diagnostic> {
    if existing_anchors.contains_key(anchor_id) {
        Some(Diagnostic {
            range: location_to_range(location),
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("acdc".to_string()),
            message: format!("Duplicate anchor ID: '{anchor_id}' is already defined"),
            ..Default::default()
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positioning_converts_to_zero_indexed() {
        // Use Location::default() since Location is non_exhaustive
        let loc = Location::default();
        let range = positioning_to_range(&Positioning::Location(loc));

        // Default Location has line=0, column=0
        // 0.saturating_sub(1) = 0
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
    }

    #[test]
    fn test_unresolved_xref_warning() {
        let anchors = HashMap::new();
        let mut loc = Location::default();
        loc.start.line = 5;
        loc.start.column = 1;
        loc.end.line = 5;
        loc.end.column = 20;
        let xrefs = vec![("missing-target".to_string(), loc)];

        let warnings = compute_warnings(&anchors, &xrefs);
        assert_eq!(warnings.len(), 1);
        let warning = warnings.first();
        assert!(warning.is_some(), "expected at least one warning");
        let warning = warning.map(|w| (&w.message, w.severity));
        assert!(
            warning.is_some_and(|(msg, _)| msg.contains("missing-target")),
            "expected warning about missing-target"
        );
        assert_eq!(
            warning.map(|(_, sev)| sev),
            Some(Some(DiagnosticSeverity::WARNING))
        );
    }

    #[test]
    fn test_resolved_xref_no_warning() {
        let mut loc = Location::default();
        loc.start.line = 1;
        let mut anchors = HashMap::new();
        anchors.insert("existing-target".to_string(), loc.clone());
        let xrefs = vec![("existing-target".to_string(), loc)];

        let warnings = compute_warnings(&anchors, &xrefs);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_duplicate_anchor_warning() {
        let mut loc = Location::default();
        loc.start.line = 10;
        let mut anchors = HashMap::new();
        anchors.insert("my-anchor".to_string(), loc.clone());

        let warning = check_duplicate_anchors("my-anchor", &loc, &anchors);
        assert!(warning.is_some(), "expected warning for duplicate anchor");
        assert!(
            warning.is_some_and(|w| w.message.contains("my-anchor")),
            "expected warning about my-anchor"
        );
    }

    #[test]
    fn test_unique_anchor_no_warning() {
        let anchors = HashMap::new();
        let loc = Location::default();

        let warning = check_duplicate_anchors("new-anchor", &loc, &anchors);
        assert!(warning.is_none());
    }
}
