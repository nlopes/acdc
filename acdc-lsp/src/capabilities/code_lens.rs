//! `CodeLens`: show reference counts above headings, anchors, and attribute definitions

use std::ops::ControlFlow;

use acdc_parser::ast::{DocumentRef, InlineKind, SectionRef};

use super::body_occurrences::{BodyOccurrence, visit_body};
use tower_lsp_server::ls_types::{CodeLens, Command, Uri};

use crate::{
    convert::{is_primary_location, location_to_utf16_range},
    state::{DocumentState, Workspace},
};

/// Compute code lenses for a document.
///
/// Returns lenses showing reference counts for:
/// - Section headings (by section ID)
/// - Standalone inline anchors
/// - Document attribute definitions
#[must_use]
pub(crate) fn compute_code_lenses(
    doc: &DocumentState,
    doc_uri: &Uri,
    workspace: &Workspace,
) -> Vec<CodeLens> {
    let source = doc.text();
    let mut lenses = Vec::new();

    if let Some(ast) = doc.ast() {
        collect_section_lenses(ast.document_ref(), source, doc_uri, workspace, &mut lenses);
    }
    collect_attribute_def_lenses(doc, doc_uri, workspace, &mut lenses);

    lenses
}

/// Collect code lenses for section headings and inline anchors.
fn collect_section_lenses(
    ast: DocumentRef<'_>,
    source: &str,
    doc_uri: &Uri,
    workspace: &Workspace,
    lenses: &mut Vec<CodeLens>,
) {
    let _ = visit_body(ast, |node| {
        match node {
            BodyOccurrence::Section(section) => {
                add_section_lens(section, source, doc_uri, workspace, lenses);
            }
            BodyOccurrence::Inline(inline) => {
                if let InlineKind::Anchor(anchor) = inline.kind()
                    && is_primary_location(anchor.location())
                    && let Some(range) = location_to_utf16_range(source, &anchor.location().into())
                {
                    let count = count_xrefs_to_anchor(anchor.id(), workspace);
                    let range = tower_lsp_server::ls_types::Range::new(range.start, range.start);
                    lenses.push(CodeLens {
                        range,
                        command: Some(make_references_command(count, doc_uri, range.start)),
                        data: None,
                    });
                }
            }
            BodyOccurrence::Image(_) => {}
        }
        ControlFlow::<()>::Continue(())
    });
}

fn add_section_lens(
    section: SectionRef<'_>,
    source: &str,
    doc_uri: &Uri,
    workspace: &Workspace,
    lenses: &mut Vec<CodeLens>,
) {
    let heading = super::identity::heading_location(section);
    if !is_primary_location(&heading) {
        return;
    }
    let Some(id) = section.id() else {
        return;
    };
    let count = count_xrefs_to_anchor(id.as_str(), workspace);

    let Some(range) = location_to_utf16_range(source, &heading) else {
        return;
    };
    // Zero-width range at start of heading line
    let range = tower_lsp_server::ls_types::Range {
        start: range.start,
        end: range.start,
    };

    lenses.push(CodeLens {
        range,
        command: Some(make_references_command(count, doc_uri, range.start)),
        data: None,
    });
}

/// Collect code lenses for document attribute definitions.
fn collect_attribute_def_lenses(
    doc: &DocumentState,
    doc_uri: &Uri,
    workspace: &Workspace,
    lenses: &mut Vec<CodeLens>,
) {
    for (name, loc) in &doc.attribute_defs {
        let count = count_attribute_refs(name, workspace);
        let Some(range) = location_to_utf16_range(doc.text(), loc) else {
            continue;
        };
        let range = tower_lsp_server::ls_types::Range {
            start: range.start,
            end: range.start,
        };

        lenses.push(CodeLens {
            range,
            command: Some(make_references_command(count, doc_uri, range.start)),
            data: None,
        });
    }
}

/// Count xrefs to a given anchor ID across all open documents.
fn count_xrefs_to_anchor(anchor_id: &str, workspace: &Workspace) -> usize {
    super::references::collect_xref_locations(workspace, anchor_id).len()
}

/// Count attribute references to a given attribute name across all open documents.
fn count_attribute_refs(attr_name: &str, workspace: &Workspace) -> usize {
    let mut count = 0usize;
    workspace.for_each_document(|_uri, doc| {
        count += doc
            .attribute_refs
            .iter()
            .filter(|(name, _)| name == attr_name)
            .count();
    });
    count
}

/// Build a Command for a `CodeLens` showing reference count.
fn make_references_command(
    count: usize,
    uri: &Uri,
    position: tower_lsp_server::ls_types::Position,
) -> Command {
    let title = match count {
        0 => "0 references".to_string(),
        1 => "1 reference".to_string(),
        n => format!("{n} references"),
    };

    Command {
        title,
        command: "editor.action.showReferences".to_string(),
        arguments: Some(vec![
            serde_json::Value::String(uri.to_string()),
            serde_json::json!({
                "line": position.line,
                "character": position.character,
            }),
            serde_json::json!([]),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;

    #[test]
    fn test_section_with_references() -> Result<(), Box<dyn std::error::Error>> {
        let content = "[[target]]\n== Target Section\n\nFirst reference <<target>>.\n\nSecond reference <<target>>.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let lenses = compute_code_lenses(&doc, &uri, &workspace);

        // Should have at least 1 lens for the section
        assert!(!lenses.is_empty(), "Expected at least one code lens");

        // Find the section lens and check it shows 2 references
        let section_lens = lenses.iter().find(|l| {
            l.command
                .as_ref()
                .is_some_and(|c| c.title.contains("2 references"))
        });
        assert!(
            section_lens.is_some(),
            "Expected a lens showing '2 references', got: {:?}",
            lenses
                .iter()
                .map(|l| l.command.as_ref().map(|c| &c.title))
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn test_section_with_zero_references() -> Result<(), Box<dyn std::error::Error>> {
        let content = "== Lonely Section\n\nNo one references this.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let lenses = compute_code_lenses(&doc, &uri, &workspace);

        let lens = lenses.first().ok_or("expected at least one lens")?;
        assert_eq!(
            lens.command.as_ref().map(|c| c.title.as_str()),
            Some("0 references")
        );
        Ok(())
    }

    #[test]
    fn test_cross_file_references() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let uri1 = "file:///doc1.adoc".parse::<Uri>()?;
        let uri2 = "file:///doc2.adoc".parse::<Uri>()?;

        let content1 = "[[shared]]\n== Shared Section\n\nContent.\n";
        let content2 = "= Other Doc\n\nSee <<shared>> for details.\n";

        workspace.update_document(uri1.clone(), content1.to_string(), 1);
        workspace.update_document(uri2, content2.to_string(), 1);

        let doc = workspace.get_document(&uri1).ok_or("document not found")?;
        let lenses = compute_code_lenses(&doc, &uri1, &workspace);

        // Section should show 1 reference (from doc2)
        let section_lens = lenses.iter().find(|l| {
            l.command
                .as_ref()
                .is_some_and(|c| c.title.contains("1 reference"))
        });
        assert!(
            section_lens.is_some(),
            "Expected '1 reference' from cross-file xref"
        );
        Ok(())
    }

    #[test]
    fn test_multiple_sections() -> Result<(), Box<dyn std::error::Error>> {
        let content =
            "== First Section\n\n<<_second_section,see below>>\n\n== Second Section\n\nContent.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let lenses = compute_code_lenses(&doc, &uri, &workspace);

        // Should have lenses for both sections
        assert!(
            lenses.len() >= 2,
            "Expected at least 2 lenses, got {}",
            lenses.len()
        );
        Ok(())
    }

    #[test]
    fn test_auto_generated_section_id() -> Result<(), Box<dyn std::error::Error>> {
        let content = "== My Section\n\nSee <<_my_section>>.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let lenses = compute_code_lenses(&doc, &uri, &workspace);

        assert_ne!(lenses, []);
        // The auto-generated ID "_my_section" should match the xref
        let lens = lenses.iter().find(|l| {
            l.command
                .as_ref()
                .is_some_and(|c| c.title.contains("1 reference"))
        });
        assert!(lens.is_some(), "Expected 1 reference for auto-generated ID");
        Ok(())
    }

    #[test]
    fn test_attribute_definition_lens() -> Result<(), Box<dyn std::error::Error>> {
        let content = ":imagesdir: ./images\n\n== Section\n\nImage in {imagesdir}/logo.png\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let lenses = compute_code_lenses(&doc, &uri, &workspace);

        // Should have a lens for the attribute definition
        let attr_lens = lenses.iter().find(|l| {
            l.command
                .as_ref()
                .is_some_and(|c| c.title.contains("1 reference"))
        });
        assert!(
            attr_lens.is_some(),
            "Expected a lens showing '1 reference' for attribute definition, got: {:?}",
            lenses
                .iter()
                .map(|l| l.command.as_ref().map(|c| &c.title))
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn test_attribute_with_no_references() -> Result<(), Box<dyn std::error::Error>> {
        let content = ":unused-attr: some value\n\n== Section\n\nNo attribute refs here.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let lenses = compute_code_lenses(&doc, &uri, &workspace);

        // Find the attribute definition lens
        let attr_lens = lenses.iter().find(|l| {
            l.command
                .as_ref()
                .is_some_and(|c| c.title == "0 references")
                && l.range.start.line == 0
        });
        assert!(
            attr_lens.is_some(),
            "Expected '0 references' lens for unused attribute"
        );
        Ok(())
    }
}
