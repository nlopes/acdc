//! Diagnostic messages and their final LSP representation.

use std::{collections::HashMap, hash::BuildHasher, path::Path};

use acdc_parser::{Block, Document, Error, Location, SourceLocation, Warning};
use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, Range};

use crate::state::{
    ConditionalBlock, ConditionalDirectiveKind, ConditionalOperation, XrefTarget,
    document::OwnedSource,
};

/// Diagnostic content whose source range has not yet been resolved.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DiagnosticMessage {
    pub(crate) message: String,
    pub(crate) severity: DiagnosticSeverity,
}

impl DiagnosticMessage {
    pub(crate) fn new(message: String, severity: DiagnosticSeverity) -> Self {
        Self { message, severity }
    }

    pub(crate) fn into_diagnostic(self, range: Range) -> Diagnostic {
        Diagnostic {
            range,
            message: self.message,
            severity: Some(self.severity),
            source: Some("acdc".into()),
            ..Default::default()
        }
    }
}

pub(crate) fn error_message(error: &Error) -> DiagnosticMessage {
    if let Error::NestedSectionLevelMismatch(_, found, expected) = error {
        let expected_markers = "=".repeat(usize::from(*expected));
        let found_markers = "=".repeat(usize::from(*found));
        return DiagnosticMessage::new(
            format!(
                "Section level skipped: expected level {expected} (`{expected_markers}`) but found level {found} (`{found_markers}`)"
            ),
            DiagnosticSeverity::WARNING,
        );
    }
    DiagnosticMessage::new(error.to_string(), DiagnosticSeverity::ERROR)
}

pub(crate) fn warning_message(warning: &Warning) -> DiagnosticMessage {
    DiagnosticMessage::new(warning.kind.to_string(), DiagnosticSeverity::WARNING)
}

/// Check local targets and use the resolver for cross-file references when available.
pub(crate) fn compute_warnings<S: BuildHasher, F>(
    anchors: &HashMap<String, SourceLocation, S>,
    targets: &[String],
    cross_file_resolver: Option<&F>,
) -> Vec<DiagnosticMessage>
where
    F: Fn(&XrefTarget) -> bool,
{
    let mut diagnostics = Vec::new();
    for target in targets {
        if anchors.contains_key(target) {
            continue;
        }
        let parsed = XrefTarget::parse(target);
        if parsed.is_cross_file() {
            if !cross_file_resolver.is_some_and(|resolver| resolver(&parsed)) {
                diagnostics.push(DiagnosticMessage::new(format!("Cross-file reference: '{target}' (cannot verify — target file may not be open)"), DiagnosticSeverity::INFORMATION));
            }
        } else {
            diagnostics.push(DiagnosticMessage::new(
                format!("Unresolved cross-reference: target '{target}' not found"),
                DiagnosticSeverity::WARNING,
            ));
        }
    }
    diagnostics
}

/// Check local media and include paths against the source directory.
/// Relative media paths also use `imagesdir` when supplied; URLs and icon names are skipped.
pub(crate) fn compute_link_diagnostics(
    media_sources: &[OwnedSource],
    includes: &[String],
    doc_dir: &Path,
    imagesdir: Option<&str>,
) -> Vec<DiagnosticMessage> {
    let mut diagnostics = Vec::new();
    for source in media_sources {
        if let OwnedSource::Path(path) = source {
            let resolved = if path.is_absolute() {
                path.clone()
            } else if let Some(images_dir) = imagesdir {
                doc_dir.join(images_dir).join(path)
            } else {
                doc_dir.join(path)
            };
            if !resolved.exists() {
                diagnostics.push(DiagnosticMessage::new(
                    format!("File not found: '{}'", path.display()),
                    DiagnosticSeverity::WARNING,
                ));
            }
        }
    }
    for target in includes {
        if !target.contains("://") && !doc_dir.join(target).exists() {
            diagnostics.push(DiagnosticMessage::new(
                format!("Included file not found: '{target}'"),
                DiagnosticSeverity::WARNING,
            ));
        }
    }
    diagnostics
}

/// Collect sections in document order by walking the AST recursively.
fn collect_sections<'a>(blocks: &'a [Block<'_>]) -> Vec<(u8, &'a Location)> {
    let mut sections = Vec::new();
    for block in blocks {
        if let Block::Section(section) = block {
            sections.push((section.level, &section.location));
            sections.extend(collect_sections(&section.content));
        }
    }
    sections
}

/// Compute diagnostics for skipped section heading levels.
///
/// Walks the AST to find sections in document order and checks that
/// levels don't jump by more than 1 (e.g., `==` followed by `====` skips `===`).
/// Going back to a higher level is always fine.
#[must_use]
pub(crate) fn compute_section_level_diagnostics(
    ast: &Document,
) -> Vec<(Location, DiagnosticMessage)> {
    let sections = collect_sections(&ast.blocks);
    let mut diagnostics = Vec::new();
    let mut last_level: u8 = 0;

    for &(level, location) in &sections {
        if level > last_level + 1 {
            let expected = last_level + 2; // display level (internal + 1)
            let found = level + 1;
            let expected_markers = "=".repeat(expected as usize);
            let found_markers = "=".repeat(found as usize);

            diagnostics.push((location.clone(), DiagnosticMessage {
                severity: DiagnosticSeverity::WARNING,
                message: format!(
                    "Section level skipped: expected level {expected} (`{expected_markers}`) but found level {found} (`{found_markers}`)"
                ),
            }));
        }
        last_level = level;
    }

    diagnostics
}

/// Compute diagnostics for inactive conditional blocks.
///
/// Emits HINT-level diagnostics with `DiagnosticTag::UNNECESSARY` for content
/// inside inactive ifdef/ifndef blocks. This causes editors to render the
/// content with reduced opacity (grayed out).
#[must_use]
pub(crate) fn compute_conditional_diagnostics(
    conditionals: &[ConditionalBlock],
    source: &str,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for cond in conditionals {
        if cond.is_active {
            continue;
        }

        let Some(end_line) = cond.end_line else {
            continue;
        };

        let directive = match cond.kind {
            ConditionalDirectiveKind::Ifdef => "ifdef",
            ConditionalDirectiveKind::Ifndef => "ifndef",
        };
        let separator = match cond.operation {
            Some(ConditionalOperation::Or) => ",",
            _ => "+",
        };
        let attrs = cond.attributes.join(separator);

        let Some(last_line) = source.lines().nth(end_line) else {
            continue;
        };
        let Ok(end_character) = u32::try_from(last_line.encode_utf16().count()) else {
            continue;
        };
        let Ok(start_line) = u32::try_from(cond.start_line) else {
            continue;
        };
        let Ok(end_line) = u32::try_from(end_line) else {
            continue;
        };

        diagnostics.push(Diagnostic {
            range: Range::new(
                tower_lsp_server::ls_types::Position::new(start_line, 0),
                tower_lsp_server::ls_types::Position::new(end_line, end_character),
            ),
            severity: Some(DiagnosticSeverity::HINT),
            source: Some("acdc".to_string()),
            message: format!("Inactive conditional block ({directive}::{attrs})"),
            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
            ..Default::default()
        });
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a document for tests, returning the owning wrapper. Callers
    /// access the AST via `.document()` which borrows from the returned
    /// value and thus cannot outlive it.
    fn parse_doc(text: &str) -> Result<acdc_parser::ParseResult, acdc_parser::Error> {
        acdc_parser::parse(text, &acdc_parser::Options::default())
    }

    #[test]
    fn test_section_level_valid_progression_no_warnings() -> Result<(), Box<dyn std::error::Error>>
    {
        let parsed = parse_doc(
            "= Title\n\n== Chapter 1\n\n=== Section 1.1\n\n== Chapter 2\n\n=== Section 2.1\n",
        )?;
        let diags = compute_section_level_diagnostics(parsed.document());
        assert!(
            diags.is_empty(),
            "valid progression should produce no warnings, got: {diags:?}"
        );
        Ok(())
    }

    #[test]
    fn test_section_level_first_section_skip() -> Result<(), Box<dyn std::error::Error>> {
        // First section is === (level 2) but should be == (level 1)
        let parsed = parse_doc("= Title\n\n=== Skipped First\n")?;
        let diags = compute_section_level_diagnostics(parsed.document());
        assert_eq!(diags.len(), 1, "expected 1 warning, got: {diags:?}");
        let (_, d) = diags.first().ok_or("expected a diagnostic")?;
        assert_eq!(d.severity, DiagnosticSeverity::WARNING);
        assert!(
            d.message.contains("Section level skipped"),
            "message: {}",
            d.message
        );
        assert!(
            d.message.contains("`===`"),
            "should mention === markers, message: {}",
            d.message
        );
        Ok(())
    }

    #[test]
    fn test_section_level_going_up_no_warning() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_doc("= Title\n\n== Chapter 1\n\n=== Deep\n\n== Chapter 2\n")?;
        let diags = compute_section_level_diagnostics(parsed.document());
        assert!(
            diags.is_empty(),
            "going up should produce no warnings, got: {diags:?}"
        );
        Ok(())
    }

    #[test]
    fn test_section_level_large_skip() -> Result<(), Box<dyn std::error::Error>> {
        // First section is ==== (level 3), skipping levels 1 and 2
        let parsed = parse_doc("= Title\n\n==== Big Skip\n")?;
        let diags = compute_section_level_diagnostics(parsed.document());
        assert_eq!(diags.len(), 1, "expected 1 warning, got: {diags:?}");
        let (_, d) = diags.first().ok_or("expected a diagnostic")?;
        assert_eq!(d.severity, DiagnosticSeverity::WARNING);
        assert!(
            d.message.contains("`==`"),
            "should suggest == as expected, message: {}",
            d.message
        );
        assert!(
            d.message.contains("`====`"),
            "should show ==== as found, message: {}",
            d.message
        );
        Ok(())
    }

    #[test]
    fn test_section_level_jump_down_one_ok() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_doc("= Title\n\n== Chapter\n\n=== Section\n")?;
        let diags = compute_section_level_diagnostics(parsed.document());
        assert!(
            diags.is_empty(),
            "increment by 1 should be fine, got: {diags:?}"
        );
        Ok(())
    }

    #[test]
    fn test_nested_section_level_mismatch_becomes_warning() -> Result<(), Box<dyn std::error::Error>>
    {
        // A nested skip (== -> ====) no longer aborts parsing; the parser renders
        // the section at its literal level, and the LSP surfaces the skip as a
        // warning from the parsed AST.
        let parsed = parse_doc("= Title\n\n== Section\n\n==== Skipped\n")?;
        let diags = compute_section_level_diagnostics(parsed.document());
        assert_eq!(diags.len(), 1, "expected 1 warning, got: {diags:?}");
        let (_, diag) = diags.first().ok_or("expected a diagnostic")?;
        assert_eq!(diag.severity, DiagnosticSeverity::WARNING);
        assert!(
            diag.message.contains("Section level skipped"),
            "message: {}",
            diag.message
        );
        assert!(
            diag.message.contains("`===`"),
            "should mention expected markers, message: {}",
            diag.message
        );
        assert!(
            diag.message.contains("`====`"),
            "should mention found markers, message: {}",
            diag.message
        );
        Ok(())
    }

    #[test]
    fn test_unresolved_xref_warning() {
        let anchors = HashMap::new();
        let xrefs = vec!["missing-target".to_string()];

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
            Some(DiagnosticSeverity::WARNING)
        );
    }

    #[test]
    fn test_resolved_xref_no_warning() {
        let mut loc = Location::default();
        loc.start.line = 1;
        let mut anchors = HashMap::new();
        anchors.insert(
            "existing-target".to_string(),
            SourceLocation::at_location(None, loc.clone()),
        );
        let xrefs = vec!["existing-target".to_string()];

        let warnings = compute_warnings::<_, fn(&XrefTarget) -> bool>(&anchors, &xrefs, None);
        assert_eq!(warnings, []);
    }

    #[test]
    fn test_missing_image_produces_warning() -> Result<(), Box<dyn std::error::Error>> {
        let media = vec![OwnedSource::Path(std::path::PathBuf::from(
            "nonexistent.png",
        ))];
        let tmp = std::env::temp_dir().join("acdc_test_missing_img");
        std::fs::create_dir_all(&tmp)?;

        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert_eq!(diags.len(), 1);
        let d = diags.first().ok_or("expected a diagnostic")?;
        assert!(d.message.contains("nonexistent.png"));
        assert_eq!(d.severity, DiagnosticSeverity::WARNING);

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_existing_image_no_warning() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = std::env::temp_dir().join("acdc_test_existing_img");
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join("photo.png"), b"fake")?;

        let media = vec![OwnedSource::Path(std::path::PathBuf::from("photo.png"))];
        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert_eq!(diags, []);

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_url_source_skipped() {
        let media = vec![OwnedSource::Url("https://example.com/img.png".to_string())];
        let tmp = std::env::temp_dir();

        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert_eq!(diags, []);
    }

    #[test]
    fn test_name_source_skipped() {
        let media = vec![OwnedSource::Name("heart".to_string())];
        let tmp = std::env::temp_dir();

        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert_eq!(diags, []);
    }

    #[test]
    fn test_missing_include_produces_warning() -> Result<(), Box<dyn std::error::Error>> {
        let includes = vec!["missing.adoc".to_string()];
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
        let tmp = std::env::temp_dir().join("acdc_test_existing_inc");
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join("chapter.adoc"), "= Chapter")?;

        let includes = vec!["chapter.adoc".to_string()];
        let diags = compute_link_diagnostics(&[], &includes, &tmp, None);
        assert_eq!(diags, []);

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_imagesdir_prepended_to_relative_paths() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = std::env::temp_dir().join("acdc_test_imagesdir");
        std::fs::create_dir_all(tmp.join("images"))?;
        std::fs::write(tmp.join("images/photo.png"), b"fake")?;

        let media = vec![OwnedSource::Path(std::path::PathBuf::from("photo.png"))];

        // Without imagesdir: should warn (file not in root)
        let diags = compute_link_diagnostics(&media, &[], &tmp, None);
        assert_eq!(diags.len(), 1);

        // With imagesdir: should resolve to images/photo.png
        let diags = compute_link_diagnostics(&media, &[], &tmp, Some("images"));
        assert_eq!(diags, []);

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_absolute_path_ignores_imagesdir() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = std::env::temp_dir().join("acdc_test_abs_img");
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join("absolute.png"), b"fake")?;

        let abs_path = tmp.join("absolute.png");
        let media = vec![OwnedSource::Path(abs_path)];

        // imagesdir should be ignored for absolute paths
        let diags = compute_link_diagnostics(&media, &[], &tmp, Some("other"));
        assert_eq!(diags, []);

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_url_include_skipped() {
        let includes = vec!["https://example.com/file.adoc".to_string()];
        let tmp = std::env::temp_dir();

        let diags = compute_link_diagnostics(&[], &includes, &tmp, None);
        assert_eq!(diags, []);
    }

    #[test]
    fn test_conditional_inactive_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
        let conditionals = vec![ConditionalBlock {
            kind: ConditionalDirectiveKind::Ifdef,
            attributes: vec!["missing-attr".to_string()],
            operation: None,
            is_active: false,
            start_line: 2,
            end_line: Some(4),
        }];

        let diags = compute_conditional_diagnostics(
            &conditionals,
            "= Title\r\n\r\nifdef::missing-attr[]\r\n😀 body\r\nendif::[]",
        );
        assert_eq!(diags.len(), 1);
        let d = diags.first().ok_or("expected one diagnostic")?;
        assert_eq!(d.severity, Some(DiagnosticSeverity::HINT));
        assert!(d.message.contains("ifdef::missing-attr"));
        assert_eq!(d.tags, Some(vec![DiagnosticTag::UNNECESSARY]));
        assert_eq!(d.range.start.line, 2);
        assert_eq!(d.range.end.line, 4);
        Ok(())
    }

    #[test]
    fn test_conditional_active_no_diagnostic() {
        let conditionals = vec![ConditionalBlock {
            kind: ConditionalDirectiveKind::Ifdef,
            attributes: vec!["present".to_string()],
            operation: None,
            is_active: true,
            start_line: 0,
            end_line: Some(2),
        }];

        let diags = compute_conditional_diagnostics(
            &conditionals,
            "= Title\r\n\r\nifdef::missing-attr[]\r\n😀 body\r\nendif::[]",
        );
        assert_eq!(diags, []);
    }
}
