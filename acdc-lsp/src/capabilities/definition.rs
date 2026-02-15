//! Go-to-definition: navigate from xrefs to anchors

use std::collections::HashMap;

use acdc_parser::{
    Block, DelimitedBlockType, Document, InlineMacro, InlineNode, Location, Section,
};
use tower_lsp::lsp_types::Position;

use crate::convert::location_to_range;
use crate::state::DocumentState;

/// Collect all anchor definitions from document AST
#[must_use]
pub fn collect_anchors(doc: &Document) -> HashMap<String, Location> {
    let mut anchors = HashMap::new();

    // Collect from all blocks
    for block in &doc.blocks {
        collect_block_anchors(block, &mut anchors);
    }

    anchors
}

fn collect_block_anchors(block: &Block, anchors: &mut HashMap<String, Location>) {
    match block {
        Block::Section(section) => {
            collect_section_anchors(section, anchors);
        }
        Block::Paragraph(para) => {
            // Collect from paragraph metadata
            collect_metadata_anchors(&para.metadata, &para.location, anchors);
            // Collect inline anchors
            collect_inline_anchors(&para.content, anchors);
        }
        Block::DelimitedBlock(delimited) => {
            // Collect from block metadata
            collect_metadata_anchors(&delimited.metadata, &delimited.location, anchors);
            // Recurse into block content
            collect_delimited_block_anchors(&delimited.inner, anchors);
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                collect_inline_anchors(&item.principal, anchors);
                for b in &item.blocks {
                    collect_block_anchors(b, anchors);
                }
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                collect_inline_anchors(&item.principal, anchors);
                for b in &item.blocks {
                    collect_block_anchors(b, anchors);
                }
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                collect_inline_anchors(&item.principal_text, anchors);
                for b in &item.description {
                    collect_block_anchors(b, anchors);
                }
            }
        }
        Block::Admonition(adm) => {
            for b in &adm.blocks {
                collect_block_anchors(b, anchors);
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
        // non_exhaustive: silently ignore future variants
        | _ => {}
    }
}

fn collect_section_anchors(section: &Section, anchors: &mut HashMap<String, Location>) {
    // Get the section ID (explicit or generated)
    // Title implements Deref<Target = [InlineNode]>
    let safe_id = Section::generate_id(&section.metadata, &section.title);
    anchors.insert(safe_id.to_string(), section.location.clone());

    // Also collect any explicit anchors from metadata
    for anchor in &section.metadata.anchors {
        anchors.insert(anchor.id.clone(), anchor.location.clone());
    }

    // Recurse into section content
    for child in &section.content {
        collect_block_anchors(child, anchors);
    }
}

fn collect_metadata_anchors(
    metadata: &acdc_parser::BlockMetadata,
    block_location: &Location,
    anchors: &mut HashMap<String, Location>,
) {
    // Collect anchors from metadata (from [[id]] or [#id] syntax)
    for anchor in &metadata.anchors {
        anchors.insert(anchor.id.clone(), anchor.location.clone());
    }
    // Also check explicit id attribute
    if let Some(id_anchor) = &metadata.id {
        anchors.insert(id_anchor.id.clone(), block_location.clone());
    }
}

fn collect_delimited_block_anchors(
    inner: &DelimitedBlockType,
    anchors: &mut HashMap<String, Location>,
) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                collect_block_anchors(block, anchors);
            }
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => {
            collect_inline_anchors(inlines, anchors);
        }
        DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_inline_anchors(inlines: &[InlineNode], anchors: &mut HashMap<String, Location>) {
    for inline in inlines {
        // Explicit arms for compile-time check when variants added
        match inline {
            InlineNode::InlineAnchor(anchor) => {
                anchors.insert(anchor.id.clone(), anchor.location.clone());
            }
            // Recurse into formatted text to find nested anchors
            InlineNode::BoldText(b) => collect_inline_anchors(&b.content, anchors),
            InlineNode::ItalicText(i) => collect_inline_anchors(&i.content, anchors),
            InlineNode::MonospaceText(m) => collect_inline_anchors(&m.content, anchors),
            InlineNode::HighlightText(h) => collect_inline_anchors(&h.content, anchors),
            InlineNode::SubscriptText(s) => collect_inline_anchors(&s.content, anchors),
            InlineNode::SuperscriptText(s) => collect_inline_anchors(&s.content, anchors),
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

/// Collect all xref targets from document
#[must_use]
pub fn collect_xrefs(doc: &Document) -> Vec<(String, Location)> {
    let mut xrefs = vec![];

    for block in &doc.blocks {
        collect_block_xrefs(block, &mut xrefs);
    }

    xrefs
}

fn collect_block_xrefs(block: &Block, xrefs: &mut Vec<(String, Location)>) {
    match block {
        Block::Section(section) => {
            for child in &section.content {
                collect_block_xrefs(child, xrefs);
            }
        }
        Block::Paragraph(para) => {
            collect_inline_xrefs(&para.content, xrefs);
        }
        Block::DelimitedBlock(delimited) => {
            collect_delimited_block_xrefs(&delimited.inner, xrefs);
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                collect_inline_xrefs(&item.principal, xrefs);
                for b in &item.blocks {
                    collect_block_xrefs(b, xrefs);
                }
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                collect_inline_xrefs(&item.principal, xrefs);
                for b in &item.blocks {
                    collect_block_xrefs(b, xrefs);
                }
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                collect_inline_xrefs(&item.principal_text, xrefs);
                for b in &item.description {
                    collect_block_xrefs(b, xrefs);
                }
            }
        }
        Block::Admonition(adm) => {
            for b in &adm.blocks {
                collect_block_xrefs(b, xrefs);
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

fn collect_delimited_block_xrefs(inner: &DelimitedBlockType, xrefs: &mut Vec<(String, Location)>) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                collect_block_xrefs(block, xrefs);
            }
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => {
            collect_inline_xrefs(inlines, xrefs);
        }
        DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_inline_xrefs(inlines: &[InlineNode], xrefs: &mut Vec<(String, Location)>) {
    for inline in inlines {
        // Explicit arms for compile-time check when variants added
        match inline {
            InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
                xrefs.push((xref.target.clone(), xref.location.clone()));
            }
            // Recurse into formatted text to find nested xrefs
            InlineNode::BoldText(b) => collect_inline_xrefs(&b.content, xrefs),
            InlineNode::ItalicText(i) => collect_inline_xrefs(&i.content, xrefs),
            InlineNode::MonospaceText(m) => collect_inline_xrefs(&m.content, xrefs),
            InlineNode::HighlightText(h) => collect_inline_xrefs(&h.content, xrefs),
            InlineNode::SubscriptText(s) => collect_inline_xrefs(&s.content, xrefs),
            InlineNode::SuperscriptText(s) => collect_inline_xrefs(&s.content, xrefs),
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
}

/// Find definition at cursor position
#[must_use]
pub fn find_definition_at_position(
    doc_state: &DocumentState,
    position: Position,
) -> Option<Location> {
    // Find if cursor is on an xref
    for (target, xref_loc) in &doc_state.xrefs {
        if position_in_range(position, xref_loc) {
            // Found an xref at cursor, look up its target
            return doc_state.anchors.get(target).cloned();
        }
    }
    None
}

/// Check if a position is within a location's range
fn position_in_range(pos: Position, loc: &Location) -> bool {
    let range = location_to_range(loc);

    if pos.line < range.start.line || pos.line > range.end.line {
        return false;
    }
    if pos.line == range.start.line && pos.character < range.start.character {
        return false;
    }
    if pos.line == range.end.line && pos.character > range.end.character {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Options;

    #[test]
    fn test_collect_section_anchors() -> Result<(), acdc_parser::Error> {
        let content = r"= Document

[[explicit-id]]
== Section With Explicit ID

Some content.

== Section With Generated ID

More content.
";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let anchors = collect_anchors(&doc);

        // Should have both explicit and generated IDs
        assert!(anchors.contains_key("explicit-id"));
        assert!(anchors.contains_key("_section_with_generated_id"));
        Ok(())
    }

    #[test]
    fn test_collect_xrefs() -> Result<(), acdc_parser::Error> {
        let content = r"= Document

== Section One

See xref:section-two[Section Two].

[[section-two]]
== Section Two

Content.
";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let xrefs = collect_xrefs(&doc);

        assert_eq!(xrefs.len(), 1);
        let xref = xrefs.first();
        assert!(xref.is_some(), "expected at least one xref");
        assert_eq!(xref.map(|(t, _)| t.as_str()), Some("section-two"));
        Ok(())
    }
}
