//! Inlay hints show header attribute values and cross-reference text inline.

use std::ops::ControlFlow;

use acdc_converters_core::InlineTextTransform;
use acdc_parser::ast::{DocumentRef, InlineKind};

use super::body_occurrences::{BodyOccurrence, visit_body};
use tower_lsp_server::ls_types::{InlayHint, InlayHintLabel, Position, Range};

use crate::{
    convert::{is_primary_location, location_to_utf16_range},
    state::DocumentState,
};

/// Maximum length for attribute value hints before truncation.
const MAX_HINT_VALUE_LEN: usize = 50;

/// Compute inlay hints for a document within the given range.
#[must_use]
pub(crate) fn compute_inlay_hints(doc: &DocumentState, range: &Range) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    collect_attribute_hints(doc, range, &mut hints);

    if let Some(ast) = doc.ast() {
        collect_xref_hints(ast.document_ref(), doc.text(), range, &mut hints);
    }

    hints
}

/// Collect inlay hints for attribute references.
///
/// Show nonempty formatted values from the header snapshot after `{name}` references.
fn collect_attribute_hints(doc: &DocumentState, range: &Range, hints: &mut Vec<InlayHint>) {
    let Some(ast) = doc.ast() else {
        return;
    };
    let ast = ast.document_ref();

    for (name, loc) in &doc.attribute_refs {
        let Some(reference_range) = location_to_utf16_range(doc.text(), loc) else {
            continue;
        };
        let hint_pos = reference_range.end;
        if !position_in_range(hint_pos, range) {
            continue;
        }

        let mut value = String::new();
        if let Some(attribute) = ast.attributes().get(name) {
            let _ = attribute.write_text(&mut value);
            if value.is_empty() {
                continue;
            }
            let label = truncate_hint_value(&value);
            hints.push(InlayHint {
                position: hint_pos,
                label: InlayHintLabel::String(label),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: None,
                data: None,
            });
        }
    }
}

/// Show registered local section text after body references without explicit labels.
fn collect_xref_hints(
    ast: DocumentRef<'_>,
    source: &str,
    range: &Range,
    hints: &mut Vec<InlayHint>,
) {
    let _ = visit_body(ast, |node| {
        if let BodyOccurrence::Inline(inline) = node
            && let InlineKind::CrossReference(xref) = inline.kind()
            && is_primary_location(xref.location())
            && xref.text().is_empty()
            && let Some(xref_range) = location_to_utf16_range(source, &xref.location().into())
            && position_in_range(xref_range.end, range)
            && let Some(hint_text) = resolve_xref_hint_text(xref.target(), ast)
        {
            hints.push(InlayHint {
                position: xref_range.end,
                label: InlayHintLabel::String(format!("→ {hint_text}")),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: None,
                data: None,
            });
        }
        ControlFlow::<()>::Continue(())
    });
}

/// Resolve a local cross-reference target to its section reference text.
fn resolve_xref_hint_text(target: &str, ast: DocumentRef<'_>) -> Option<String> {
    let parsed = crate::state::XrefTarget::parse(target);

    if parsed.is_cross_file() {
        return None;
    }

    let anchor_id = parsed.anchor.as_deref()?;

    if !ast.toc_entries().any(|entry| entry.id() == anchor_id) {
        return None;
    }
    let reference = ast.references().get(anchor_id)?;
    reference
        .xreflabel()
        .or_else(|| reference.title())
        .map(|content| InlineTextTransform::default().content_to_string(content))
}

/// Check if a position falls within a range.
fn position_in_range(pos: Position, range: &Range) -> bool {
    let after_start = pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character);
    let before_end = pos.line < range.end.line
        || (pos.line == range.end.line && pos.character <= range.end.character);
    after_start && before_end
}

/// Format an attribute value as a hint label, truncating if too long.
fn truncate_hint_value(value: &str) -> String {
    let char_count = value.chars().count();
    if char_count <= MAX_HINT_VALUE_LEN {
        format!("= {value}")
    } else {
        let truncated: String = value.chars().take(MAX_HINT_VALUE_LEN).collect();
        format!("= {truncated}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;
    use std::error::Error;
    use tower_lsp_server::ls_types::Uri;

    /// Full-document range for tests that don't need range filtering.
    fn full_range() -> Range {
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: u32::MAX,
                character: u32::MAX,
            },
        }
    }

    #[test]
    fn test_attribute_hint_resolved() -> Result<(), Box<dyn Error>> {
        let content =
            ":product-name: Acme Cloud Platform\n\n== Section\n\nWelcome to {product-name} docs.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());

        assert_eq!(hints.len(), 1, "Expected exactly one hint, got: {hints:?}");
        let hint = hints.first().ok_or("expected at least one hint")?;
        let label = match &hint.label {
            InlayHintLabel::String(s) => s.as_str(),
            InlayHintLabel::LabelParts(_) => "",
        };
        assert_eq!(label, "= Acme Cloud Platform");
        assert_eq!(hint.padding_left, Some(true));
        Ok(())
    }

    #[test]
    fn test_attribute_hint_undefined() -> Result<(), Box<dyn Error>> {
        let content = "== Section\n\nSee {undefined-attr} here.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());

        assert!(
            hints.is_empty(),
            "Expected no hints for undefined attribute, got: {hints:?}"
        );
        Ok(())
    }

    #[test]
    fn test_attribute_hint_formats_typed_default() -> Result<(), Box<dyn Error>> {
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), "depth={max-include-depth}\n".to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());
        let hint = hints.first().ok_or("expected typed attribute hint")?;
        let InlayHintLabel::String(label) = &hint.label else {
            return Err("expected string hint label".into());
        };
        assert_eq!(label, "= 64");
        Ok(())
    }

    #[test]
    fn test_attribute_hint_empty_value() -> Result<(), Box<dyn Error>> {
        let content = ":empty-attr:\n\n== Section\n\nSee {empty-attr} here.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());

        // Boolean/empty attributes should not produce hints
        assert!(
            hints.is_empty(),
            "Expected no hints for empty attribute, got: {hints:?}"
        );
        Ok(())
    }

    #[test]
    fn test_attribute_hint_truncation() {
        let long_value = "a".repeat(100);
        let label = truncate_hint_value(&long_value);
        assert!(label.starts_with("= "));
        assert!(label.ends_with('\u{2026}'));
        // "= " + 50 chars + "…"
        assert_eq!(label.chars().count(), 2 + MAX_HINT_VALUE_LEN + 1);
    }

    #[test]
    fn test_xref_hint_with_title() -> Result<(), Box<dyn Error>> {
        let content = "[[setup]]\n== Initial Setup\n\nSee <<setup>> for details.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());

        let xref_label = hints.iter().find_map(|h| match &h.label {
            InlayHintLabel::String(s) if s.contains('\u{2192}') => Some(s.as_str()),
            InlayHintLabel::String(_) | InlayHintLabel::LabelParts(_) => None,
        });
        assert_eq!(
            xref_label,
            Some("\u{2192} Initial Setup"),
            "Expected xref hint with section title"
        );
        Ok(())
    }

    #[test]
    fn interdocument_xref_macro_does_not_use_a_matching_local_title_hint()
    -> Result<(), Box<dyn Error>> {
        let content = "Macro: xref:Other.adoc[].\n\nExplicit: xref:Other.adoc[Other].\n\nShorthand: <<Other.adoc>>.\n\n== Other.adoc\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let labels = compute_inlay_hints(&doc, &full_range())
            .into_iter()
            .filter_map(|hint| match hint.label {
                InlayHintLabel::String(label) if label.starts_with('→') => Some(label),
                InlayHintLabel::String(_) | InlayHintLabel::LabelParts(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(labels, ["→ Other.adoc"]);
        Ok(())
    }

    #[test]
    fn compat_mode_natural_xref_has_no_title_hint() -> Result<(), Box<dyn Error>> {
        let content = "= Document\n:compat-mode:\n\nNatural: <<Syntax Highlighting>>.\nExplicit: <<_syntax_highlighting>>.\n\n== Syntax Highlighting\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let labels = compute_inlay_hints(&doc, &full_range())
            .into_iter()
            .filter_map(|hint| match hint.label {
                InlayHintLabel::String(label) if label.starts_with('→') => Some(label),
                InlayHintLabel::String(_) | InlayHintLabel::LabelParts(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(labels, ["→ Syntax Highlighting"]);
        Ok(())
    }

    #[test]
    fn named_section_reftext_controls_xref_hints() -> Result<(), Box<dyn Error>> {
        let content = "Named: <<Custom Label>>.\nTitle: <<Actual Title>>.\nExplicit: <<id>>.\nExplicit text: <<id,Chosen text>>.\nFormatted: <<Custom *Formatted* Label>>.\n\n[#id,reftext=\"Custom Label\"]\n== Actual Title\n\n[#formatted,reftext=\"Custom *Formatted* Label\"]\n== Formatted Title\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let labels = compute_inlay_hints(&doc, &full_range())
            .into_iter()
            .filter_map(|hint| match hint.label {
                InlayHintLabel::String(label) if label.starts_with('→') => Some(label),
                InlayHintLabel::String(_) | InlayHintLabel::LabelParts(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            [
                "→ Custom Label",
                "→ Custom Label",
                "→ Custom Formatted Label"
            ]
        );
        Ok(())
    }

    #[test]
    fn passthroughs_are_restored_before_natural_xref_hints() -> Result<(), Box<dyn Error>> {
        let content = "Resolved: <<Pass raw Title>>.\nMissing: <<Missing pass:[raw] Title>>.\n\n== Pass pass:[raw] Title\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let labels = compute_inlay_hints(&doc, &full_range())
            .into_iter()
            .filter_map(|hint| match hint.label {
                InlayHintLabel::String(label) if label.starts_with('→') => Some(label),
                InlayHintLabel::String(_) | InlayHintLabel::LabelParts(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(labels, ["→ Pass raw Title"]);
        Ok(())
    }

    #[test]
    fn test_xref_hint_explicit_text_skipped() -> Result<(), Box<dyn Error>> {
        let content = "[[setup]]\n== Initial Setup\n\nSee <<setup,click here>> for details.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());

        let xref_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(&h.label, InlayHintLabel::String(s) if s.contains('\u{2192}')))
            .collect();
        assert!(
            xref_hints.is_empty(),
            "Expected no xref hint when display text is present, got: {xref_hints:?}"
        );
        Ok(())
    }

    #[test]
    fn test_xref_hint_unresolved() -> Result<(), Box<dyn Error>> {
        let content = "== Section\n\nSee <<nonexistent>> here.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());

        let xref_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(&h.label, InlayHintLabel::String(s) if s.contains('\u{2192}')))
            .collect();
        assert!(
            xref_hints.is_empty(),
            "Expected no xref hint for unresolved target, got: {xref_hints:?}"
        );
        Ok(())
    }

    #[test]
    fn test_hints_filtered_by_range() -> Result<(), Box<dyn Error>> {
        let content = ":name: Value\n\n== Section\n\n{name} on line 5.\n\n{name} on line 7.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Only request hints for lines 4-5 (0-indexed), covering just the first {name}
        let narrow_range = Range {
            start: Position {
                line: 4,
                character: 0,
            },
            end: Position {
                line: 4,
                character: u32::MAX,
            },
        };
        let hints = compute_inlay_hints(&doc, &narrow_range);

        assert_eq!(hints.len(), 1, "Expected 1 hint in range, got: {hints:?}");
        Ok(())
    }

    #[test]
    fn test_multiple_hints() -> Result<(), Box<dyn Error>> {
        let content = ":product: Acme\n:version: 2.0\n\n[[intro]]\n== Introduction\n\n{product} v{version} — see <<intro>>.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());

        // Should have 2 attribute hints + 1 xref hint = 3
        assert!(
            hints.len() >= 3,
            "Expected at least 3 hints (2 attrs + 1 xref), got {}: {hints:?}",
            hints.len()
        );
        Ok(())
    }

    #[test]
    fn test_position_in_range_basic() {
        let range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 10,
                character: 80,
            },
        };

        assert!(position_in_range(
            Position {
                line: 5,
                character: 10
            },
            &range
        ));
        assert!(!position_in_range(
            Position {
                line: 1,
                character: 0
            },
            &range
        ));
        assert!(!position_in_range(
            Position {
                line: 11,
                character: 0
            },
            &range
        ));
    }

    #[test]
    fn test_xref_with_xreflabel() -> Result<(), Box<dyn Error>> {
        let content = "[[setup,Getting Started]]\n== Initial Setup\n\nSee <<setup>> for details.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hints = compute_inlay_hints(&doc, &full_range());

        let xref_label = hints.iter().find_map(|h| match &h.label {
            InlayHintLabel::String(s) if s.contains('\u{2192}') => Some(s.as_str()),
            InlayHintLabel::String(_) | InlayHintLabel::LabelParts(_) => None,
        });
        // Should use xreflabel "Getting Started" instead of section title "Initial Setup"
        assert_eq!(
            xref_label,
            Some("\u{2192} Getting Started"),
            "Expected xreflabel to override section title"
        );
        Ok(())
    }
}
