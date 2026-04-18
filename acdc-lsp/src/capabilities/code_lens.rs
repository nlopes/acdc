//! `CodeLens`: show reference counts above headings, anchors, and attribute definitions

use acdc_parser::{Block, Document, InlineNode, Section};
use tower_lsp_server::ls_types::{CodeLens, Command, Uri};

use crate::convert::location_to_range;
use crate::state::{DocumentState, Workspace, XrefTarget};

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
    let mut lenses = Vec::new();

    if let Some(ast) = doc.ast() {
        collect_section_lenses(ast.document(), doc_uri, workspace, &mut lenses);
    }
    collect_attribute_def_lenses(doc, doc_uri, workspace, &mut lenses);

    lenses
}

/// Collect code lenses for section headings and inline anchors.
fn collect_section_lenses(
    ast: &Document,
    doc_uri: &Uri,
    workspace: &Workspace,
    lenses: &mut Vec<CodeLens>,
) {
    for block in &ast.blocks {
        collect_block_lenses(block, doc_uri, workspace, lenses);
    }
}

fn collect_block_lenses(
    block: &Block,
    doc_uri: &Uri,
    workspace: &Workspace,
    lenses: &mut Vec<CodeLens>,
) {
    match block {
        Block::Section(section) => {
            add_section_lens(section, doc_uri, workspace, lenses);
            for child in &section.content {
                collect_block_lenses(child, doc_uri, workspace, lenses);
            }
        }
        Block::Paragraph(para) => {
            collect_inline_anchor_lenses(&para.content, doc_uri, workspace, lenses);
        }
        Block::DelimitedBlock(delimited) => {
            collect_delimited_lenses(&delimited.inner, doc_uri, workspace, lenses);
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                collect_inline_anchor_lenses(&item.principal, doc_uri, workspace, lenses);
                for b in &item.blocks {
                    collect_block_lenses(b, doc_uri, workspace, lenses);
                }
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                collect_inline_anchor_lenses(&item.principal, doc_uri, workspace, lenses);
                for b in &item.blocks {
                    collect_block_lenses(b, doc_uri, workspace, lenses);
                }
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                collect_inline_anchor_lenses(&item.principal_text, doc_uri, workspace, lenses);
                for b in &item.description {
                    collect_block_lenses(b, doc_uri, workspace, lenses);
                }
            }
        }
        Block::Admonition(adm) => {
            for b in &adm.blocks {
                collect_block_lenses(b, doc_uri, workspace, lenses);
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

fn collect_delimited_lenses(
    inner: &acdc_parser::DelimitedBlockType,
    doc_uri: &Uri,
    workspace: &Workspace,
    lenses: &mut Vec<CodeLens>,
) {
    match inner {
        acdc_parser::DelimitedBlockType::DelimitedExample(blocks)
        | acdc_parser::DelimitedBlockType::DelimitedOpen(blocks)
        | acdc_parser::DelimitedBlockType::DelimitedSidebar(blocks)
        | acdc_parser::DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                collect_block_lenses(block, doc_uri, workspace, lenses);
            }
        }
        acdc_parser::DelimitedBlockType::DelimitedListing(inlines)
        | acdc_parser::DelimitedBlockType::DelimitedLiteral(inlines)
        | acdc_parser::DelimitedBlockType::DelimitedPass(inlines)
        | acdc_parser::DelimitedBlockType::DelimitedVerse(inlines)
        | acdc_parser::DelimitedBlockType::DelimitedComment(inlines) => {
            collect_inline_anchor_lenses(inlines, doc_uri, workspace, lenses);
        }
        acdc_parser::DelimitedBlockType::DelimitedTable(_)
        | acdc_parser::DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

fn add_section_lens(
    section: &Section,
    doc_uri: &Uri,
    workspace: &Workspace,
    lenses: &mut Vec<CodeLens>,
) {
    let id = Section::generate_id_string(&section.metadata, &section.title);
    let count = count_xrefs_to_anchor(&id, workspace);

    let range = location_to_range(&section.location);
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

fn collect_inline_anchor_lenses(
    inlines: &[InlineNode],
    doc_uri: &Uri,
    workspace: &Workspace,
    lenses: &mut Vec<CodeLens>,
) {
    for inline in inlines {
        match inline {
            InlineNode::InlineAnchor(anchor) => {
                let count = count_xrefs_to_anchor(anchor.id, workspace);
                let range = location_to_range(&anchor.location);
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
            InlineNode::BoldText(b) => {
                collect_inline_anchor_lenses(&b.content, doc_uri, workspace, lenses);
            }
            InlineNode::ItalicText(i) => {
                collect_inline_anchor_lenses(&i.content, doc_uri, workspace, lenses);
            }
            InlineNode::MonospaceText(m) => {
                collect_inline_anchor_lenses(&m.content, doc_uri, workspace, lenses);
            }
            InlineNode::HighlightText(h) => {
                collect_inline_anchor_lenses(&h.content, doc_uri, workspace, lenses);
            }
            InlineNode::SubscriptText(s) => {
                collect_inline_anchor_lenses(&s.content, doc_uri, workspace, lenses);
            }
            InlineNode::SuperscriptText(s) => {
                collect_inline_anchor_lenses(&s.content, doc_uri, workspace, lenses);
            }
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            // non_exhaustive
            | _ => {}
        }
    }
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
        let range = location_to_range(loc);
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
    let mut count = 0usize;
    workspace.for_each_document(|_uri, doc| {
        count += doc
            .xrefs
            .iter()
            .filter(|(target, _)| {
                let parsed = XrefTarget::parse(target);
                parsed.anchor.as_deref() == Some(anchor_id) || target == anchor_id
            })
            .count();
    });
    count
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

        assert!(!lenses.is_empty());
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
