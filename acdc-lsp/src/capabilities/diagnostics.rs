//! Diagnostics: convert acdc-parser errors to LSP diagnostics
//!
//! This module handles two types of diagnostics:
//! - Parse errors: converted from acdc-parser errors
//! - Validation warnings: unresolved xrefs, duplicate anchors, etc.

use std::collections::HashMap;
use std::hash::BuildHasher;

use std::path::Path;

use acdc_parser::{Error, Location, Positioning, Source};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::convert::{location_to_range, parser_position_to_lsp};
use crate::state::XrefTarget;

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
///
/// When `cross_file_resolver` is provided, cross-file xrefs are validated
/// using the resolver (which may check open documents, on-disk files, etc.).
/// Without it, cross-file xrefs get an info-level diagnostic.
#[must_use]
pub fn compute_warnings<S: BuildHasher, F>(
    anchors: &HashMap<String, Location, S>,
    xrefs: &[(String, Location)],
    cross_file_resolver: Option<&F>,
) -> Vec<Diagnostic>
where
    F: Fn(&XrefTarget) -> bool,
{
    let mut diagnostics = Vec::new();

    // Check for unresolved xrefs
    for (target, location) in xrefs {
        if !anchors.contains_key(target) {
            let parsed = XrefTarget::parse(target);
            if parsed.is_cross_file() {
                // Check with resolver if available
                let resolved = cross_file_resolver.is_some_and(|resolver| resolver(&parsed));
                if !resolved {
                    diagnostics.push(Diagnostic {
                        range: location_to_range(location),
                        severity: Some(DiagnosticSeverity::INFORMATION),
                        source: Some("acdc".to_string()),
                        message: format!(
                            "Cross-file reference: '{target}' (cannot verify — target file may not be open)"
                        ),
                        ..Default::default()
                    });
                }
            } else {
                diagnostics.push(Diagnostic {
                    range: location_to_range(location),
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("acdc".to_string()),
                    message: format!("Unresolved cross-reference: target '{target}' not found"),
                    ..Default::default()
                });
            }
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

/// Compute diagnostics for missing files referenced by images, audio, video, and includes.
///
/// Checks that local file paths actually exist on disk. URLs and icon names are skipped.
/// Image paths are resolved relative to `imagesdir` (if set), then relative to `doc_dir`.
/// Include paths are resolved relative to `doc_dir`.
#[must_use]
pub fn compute_link_diagnostics(
    media_sources: &[(Source, Location)],
    includes: &[(String, Location)],
    doc_dir: &Path,
    imagesdir: Option<&str>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (source, location) in media_sources {
        if let Source::Path(path) = source {
            let resolved = if path.is_absolute() {
                path.clone()
            } else if let Some(images_dir) = imagesdir {
                doc_dir.join(images_dir).join(path)
            } else {
                doc_dir.join(path)
            };

            if !resolved.exists() {
                diagnostics.push(Diagnostic {
                    range: location_to_range(location),
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("acdc".to_string()),
                    message: format!("File not found: '{}'", path.display()),
                    ..Default::default()
                });
            }
        }
    }

    for (target, location) in includes {
        if target.contains("://") {
            continue;
        }
        let resolved = doc_dir.join(target);
        if !resolved.exists() {
            diagnostics.push(Diagnostic {
                range: location_to_range(location),
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("acdc".to_string()),
                message: format!("Included file not found: '{target}'"),
                ..Default::default()
            });
        }
    }

    diagnostics
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

        let warnings = compute_warnings::<_, fn(&XrefTarget) -> bool>(&anchors, &xrefs, None);
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

        let warnings = compute_warnings::<_, fn(&XrefTarget) -> bool>(&anchors, &xrefs, None);
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

    #[test]
    fn test_missing_image_produces_warning() -> Result<(), Box<dyn std::error::Error>> {
        let loc = Location::default();
        let media = vec![(
            Source::Path(std::path::PathBuf::from("nonexistent.png")),
            loc,
        )];
        let tmp = std::env::temp_dir().join("acdc_test_missing_img");
        std::fs::create_dir_all(&tmp)?;

        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert_eq!(diags.len(), 1);
        let d = diags.first().ok_or("expected a diagnostic")?;
        assert!(d.message.contains("nonexistent.png"));
        assert_eq!(d.severity, Some(DiagnosticSeverity::WARNING));

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_existing_image_no_warning() -> Result<(), Box<dyn std::error::Error>> {
        let loc = Location::default();
        let tmp = std::env::temp_dir().join("acdc_test_existing_img");
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join("photo.png"), b"fake")?;

        let media = vec![(Source::Path(std::path::PathBuf::from("photo.png")), loc)];
        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert!(diags.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_url_source_skipped() -> Result<(), Box<dyn std::error::Error>> {
        let loc = Location::default();
        let url = acdc_parser::SourceUrl::new("https://example.com/img.png")?;
        let media = vec![(Source::Url(url), loc)];
        let tmp = std::env::temp_dir();

        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert!(diags.is_empty());
        Ok(())
    }

    #[test]
    fn test_name_source_skipped() {
        let loc = Location::default();
        let media = vec![(Source::Name("heart".to_string()), loc)];
        let tmp = std::env::temp_dir();

        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_missing_include_produces_warning() -> Result<(), Box<dyn std::error::Error>> {
        let loc = Location::default();
        let includes = vec![("missing.adoc".to_string(), loc)];
        let tmp = std::env::temp_dir().join("acdc_test_missing_inc");
        std::fs::create_dir_all(&tmp)?;

        let diags = compute_link_diagnostics(&[], &includes, &tmp, None);
        assert_eq!(diags.len(), 1);
        let d = diags.first().ok_or("expected a diagnostic")?;
        assert!(d.message.contains("missing.adoc"));

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_existing_include_no_warning() -> Result<(), Box<dyn std::error::Error>> {
        let loc = Location::default();
        let tmp = std::env::temp_dir().join("acdc_test_existing_inc");
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join("chapter.adoc"), "= Chapter")?;

        let includes = vec![("chapter.adoc".to_string(), loc)];
        let diags = compute_link_diagnostics(&[], &includes, &tmp, None);
        assert!(diags.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_imagesdir_prepended_to_relative_paths() -> Result<(), Box<dyn std::error::Error>> {
        let loc = Location::default();
        let tmp = std::env::temp_dir().join("acdc_test_imagesdir");
        std::fs::create_dir_all(tmp.join("images"))?;
        std::fs::write(tmp.join("images/photo.png"), b"fake")?;

        let media = vec![(Source::Path(std::path::PathBuf::from("photo.png")), loc)];

        // Without imagesdir: should warn (file not in root)
        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert_eq!(diags.len(), 1);

        // With imagesdir: should resolve to images/photo.png
        let diags = compute_link_diagnostics(&media, &[], &tmp, Some("images"));
        assert!(diags.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_absolute_path_ignores_imagesdir() -> Result<(), Box<dyn std::error::Error>> {
        let loc = Location::default();
        let tmp = std::env::temp_dir().join("acdc_test_abs_img");
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join("absolute.png"), b"fake")?;

        let abs_path = tmp.join("absolute.png");
        let media = vec![(Source::Path(abs_path), loc)];

        // imagesdir should be ignored for absolute paths
        let diags = compute_link_diagnostics(&media, &[], &tmp, Some("other"));
        assert!(diags.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_url_include_skipped() {
        let loc = Location::default();
        let includes = vec![("https://example.com/file.adoc".to_string(), loc)];
        let tmp = std::env::temp_dir();

        let diags = compute_link_diagnostics(&[], &includes, &tmp, None);
        assert!(diags.is_empty());
    }
}
