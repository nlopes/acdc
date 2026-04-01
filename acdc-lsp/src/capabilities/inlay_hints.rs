//! Inlay hints: show resolved attribute values and xref titles inline

use acdc_parser::{
    AttributeValue, Block, DelimitedBlockType, Document, InlineMacro, InlineNode, inlines_to_string,
};
use tower_lsp_server::ls_types::{InlayHint, InlayHintLabel, Position, Range};

use crate::convert::location_to_range;
use crate::state::DocumentState;

/// Maximum length for attribute value hints before truncation.
const MAX_HINT_VALUE_LEN: usize = 50;

/// Compute inlay hints for a document within the given range.
#[must_use]
pub(crate) fn compute_inlay_hints(doc: &DocumentState, range: &Range) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    collect_attribute_hints(doc, range, &mut hints);

    if let Some(ast) = &doc.ast {
        collect_xref_hints(ast, range, &mut hints);
    }

    hints
}

/// Collect inlay hints for attribute references.
///
/// For each `{name}` reference that resolves to a string value,
/// shows the resolved value as a hint after the closing `}`.
fn collect_attribute_hints(doc: &DocumentState, range: &Range, hints: &mut Vec<InlayHint>) {
    let Some(ast) = &doc.ast else {
        return;
    };

    for (name, loc) in &doc.attribute_refs {
        let hint_pos = location_to_range(loc).end;
        if !position_in_range(hint_pos, range) {
            continue;
        }

        if let Some(AttributeValue::String(value)) = ast.attributes.get(name) {
            if value.is_empty() {
                continue;
            }
            let label = truncate_hint_value(value);
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

/// Collect inlay hints for cross-references without explicit display text.
///
/// For each `<<id>>` or `xref:file#id[]` that has no display text,
/// shows the resolved section title as a hint after the xref.
fn collect_xref_hints(ast: &Document, range: &Range, hints: &mut Vec<InlayHint>) {
    for block in &ast.blocks {
        collect_xref_hints_in_block(block, ast, range, hints);
    }
}

fn collect_xref_hints_in_block(
    block: &Block,
    ast: &Document,
    range: &Range,
    hints: &mut Vec<InlayHint>,
) {
    match block {
        Block::Section(section) => {
            for child in &section.content {
                collect_xref_hints_in_block(child, ast, range, hints);
            }
        }
        Block::Paragraph(para) => {
            collect_xref_hints_in_inlines(&para.content, ast, range, hints);
        }
        Block::DelimitedBlock(delimited) => {
            collect_xref_hints_in_delimited(&delimited.inner, ast, range, hints);
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                collect_xref_hints_in_inlines(&item.principal, ast, range, hints);
                for b in &item.blocks {
                    collect_xref_hints_in_block(b, ast, range, hints);
                }
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                collect_xref_hints_in_inlines(&item.principal, ast, range, hints);
                for b in &item.blocks {
                    collect_xref_hints_in_block(b, ast, range, hints);
                }
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                collect_xref_hints_in_inlines(&item.principal_text, ast, range, hints);
                for b in &item.description {
                    collect_xref_hints_in_block(b, ast, range, hints);
                }
            }
        }
        Block::Admonition(adm) => {
            for b in &adm.blocks {
                collect_xref_hints_in_block(b, ast, range, hints);
            }
        }
        Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::CalloutList(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_xref_hints_in_delimited(
    inner: &DelimitedBlockType,
    ast: &Document,
    range: &Range,
    hints: &mut Vec<InlayHint>,
) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                collect_xref_hints_in_block(block, ast, range, hints);
            }
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => {
            collect_xref_hints_in_inlines(inlines, ast, range, hints);
        }
        DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_xref_hints_in_inlines(
    inlines: &[InlineNode],
    ast: &Document,
    range: &Range,
    hints: &mut Vec<InlayHint>,
) {
    for inline in inlines {
        collect_xref_hint_in_inline(inline, ast, range, hints);
    }
}

fn collect_xref_hint_in_inline(
    inline: &InlineNode,
    ast: &Document,
    range: &Range,
    hints: &mut Vec<InlayHint>,
) {
    match inline {
        InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
            if !xref.text.is_empty() {
                return;
            }

            let hint_pos = location_to_range(&xref.location).end;
            if !position_in_range(hint_pos, range) {
                return;
            }

            if let Some(title) = resolve_xref_title(&xref.target, ast) {
                hints.push(InlayHint {
                    position: hint_pos,
                    label: InlayHintLabel::String(format!("\u{2192} {title}")),
                    kind: None,
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: None,
                    data: None,
                });
            }
        }
        InlineNode::BoldText(b) => {
            collect_xref_hints_in_inlines(&b.content, ast, range, hints);
        }
        InlineNode::ItalicText(i) => {
            collect_xref_hints_in_inlines(&i.content, ast, range, hints);
        }
        InlineNode::MonospaceText(m) => {
            collect_xref_hints_in_inlines(&m.content, ast, range, hints);
        }
        InlineNode::HighlightText(h) => {
            collect_xref_hints_in_inlines(&h.content, ast, range, hints);
        }
        InlineNode::SubscriptText(s) => {
            collect_xref_hints_in_inlines(&s.content, ast, range, hints);
        }
        InlineNode::SuperscriptText(s) => {
            collect_xref_hints_in_inlines(&s.content, ast, range, hints);
        }
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        // non_exhaustive
        | _ => {}
    }
}

/// Resolve a xref target to a section title using the TOC entries.
fn resolve_xref_title(target: &str, ast: &Document) -> Option<String> {
    let parsed = crate::state::XrefTarget::parse(target);

    // Only resolve local (same-document) xrefs for now
    if parsed.is_cross_file() {
        return None;
    }

    let anchor_id = parsed.anchor.as_deref()?;

    ast.toc_entries.iter().find_map(|entry| {
        if entry.id == anchor_id {
            Some(
                entry
                    .xreflabel
                    .clone()
                    .unwrap_or_else(|| inlines_to_string(&entry.title)),
            )
        } else {
            None
        }
    })
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
    fn test_attribute_hint_resolved() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_attribute_hint_undefined() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_attribute_hint_empty_value() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_xref_hint_with_title() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_xref_hint_explicit_text_skipped() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_xref_hint_unresolved() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_hints_filtered_by_range() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_multiple_hints() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_xref_with_xreflabel() -> Result<(), Box<dyn std::error::Error>> {
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
