//! Go-to-definition: navigate from xrefs to anchors

use std::collections::HashMap;

use acdc_parser::{
    Block, DelimitedBlockType, Document, InlineMacro, InlineNode, Location, Section,
};
use tower_lsp::lsp_types::Position;

use tower_lsp::lsp_types::Url;

use crate::state::{DocumentState, Workspace, XrefTarget};

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

/// Build a location covering just the section heading line (from section start
/// to the end of the title text), rather than the full section span.
fn heading_line_location(section: &Section) -> Location {
    let mut loc = section.location.clone();
    if let Some(last_inline) = section.title.last() {
        let title_loc = last_inline.location();
        loc.absolute_end = title_loc.absolute_end;
        loc.end = title_loc.end.clone();
    }
    loc
}

fn collect_section_anchors(section: &Section, anchors: &mut HashMap<String, Location>) {
    // Get the section ID (explicit or generated)
    // Title implements Deref<Target = [InlineNode]>
    let safe_id = Section::generate_id(&section.metadata, &section.title);
    // Use heading-line location (section start to title end), not the full
    // section span. The full span covers all content blocks, which would make
    // the anchor match on hover anywhere inside the section body.
    let heading_loc = heading_line_location(section);
    anchors.insert(safe_id.to_string(), heading_loc);

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

/// Find definition at cursor position, with cross-file resolution
#[must_use]
pub fn find_definition_at_position(
    doc_state: &DocumentState,
    doc_uri: &Url,
    workspace: &Workspace,
    position: Position,
) -> Option<(Url, Location)> {
    let offset = crate::convert::position_to_offset(&doc_state.text, position)?;
    let ast = doc_state.ast.as_ref()?;
    tracing::info!(
        ?position,
        offset,
        text_len = doc_state.text.len(),
        "find_definition_at_position"
    );

    // Check if cursor is on an xref
    if let Some((target, _)) = crate::capabilities::hover::find_xref_at_offset(ast, offset) {
        tracing::info!(target, "found xref at offset");
        return resolve_xref_target(&target, doc_state, doc_uri, workspace);
    }

    // Check if cursor is on a link macro
    if let Some((target, _)) = crate::capabilities::hover::find_link_at_offset(ast, offset) {
        return resolve_link_target(&target, doc_uri, workspace);
    }

    tracing::info!("no xref or link found at offset");
    None
}

/// Resolve an xref target to a definition location
fn resolve_xref_target(
    target: &str,
    doc_state: &DocumentState,
    doc_uri: &Url,
    workspace: &Workspace,
) -> Option<(Url, Location)> {
    let parsed = XrefTarget::parse(target);
    tracing::info!(
        ?target,
        file = ?parsed.file,
        anchor = ?parsed.anchor,
        "resolve_xref_target"
    );

    if let Some(file_path) = &parsed.file {
        // Try direct file + anchor resolution first
        if let Some(target_uri) = workspace.resolve_xref_file(doc_uri, file_path) {
            tracing::info!(%target_uri, "resolved xref file URI");
            if let Some(anchor_id) = &parsed.anchor {
                if let Some(loc) = workspace.find_anchor_in_document(&target_uri, anchor_id) {
                    return Some((target_uri, loc));
                }
                tracing::info!(
                    anchor_id,
                    "anchor not found in document, falling through to global"
                );
                // File resolved but anchor not found — fall through to global search
            } else {
                // Cross-file without anchor — jump to file start
                let mut loc = Location::default();
                loc.start.line = 1;
                loc.start.column = 1;
                return Some((target_uri, loc));
            }
        } else {
            tracing::info!(%doc_uri, file_path, "resolve_xref_file returned None");
        }

        // Fallback: try global anchor index (handles URI mismatches, file not yet indexed)
        if let Some(anchor_id) = &parsed.anchor {
            let global = workspace.find_anchor_globally(anchor_id);
            tracing::info!(anchor_id, count = global.len(), "global anchor search");
            if let Some((uri, loc)) = global.into_iter().next() {
                return Some((uri, loc));
            }
        }

        return None;
    }

    // Local target
    if let Some(anchor_id) = &parsed.anchor {
        if let Some(loc) = doc_state.anchors.get(anchor_id) {
            return Some((doc_uri.clone(), loc.clone()));
        }
        let global = workspace.find_anchor_globally(anchor_id);
        if let Some((uri, loc)) = global.into_iter().next() {
            return Some((uri, loc));
        }
    }

    None
}

/// Resolve a link target to a definition location (file paths only)
fn resolve_link_target(
    target: &str,
    doc_uri: &Url,
    workspace: &Workspace,
) -> Option<(Url, Location)> {
    // Only resolve file paths, not full URLs
    if target.contains("://") {
        return None;
    }

    // Strip fragment if present
    let file_path = target.split('#').next()?;
    if file_path.is_empty() {
        return None;
    }

    let target_uri = workspace.resolve_xref_file(doc_uri, file_path)?;
    let mut loc = Location::default();
    loc.start.line = 1;
    loc.start.column = 1;
    Some((target_uri, loc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;
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

    #[test]
    fn test_cross_file_xref_definition() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let a_uri = Url::parse("file:///project/a.adoc")?;
        let file_uri = Url::parse("file:///project/file.adoc")?;

        // Use same content as real files
        let file_content = "= another doc\n\n== yay\n\nA thing\n";
        workspace.update_document(file_uri.clone(), file_content.to_string(), 1);

        let a_content =
            "= A document\n\n== Capitulo 1\n\nA coisa do xref:file.adoc#_yay[text] e parva.\n";
        workspace.update_document(a_uri.clone(), a_content.to_string(), 1);

        // Verify file.adoc has the _yay anchor
        let file_doc = workspace
            .get_document(&file_uri)
            .ok_or("file.adoc should be indexed")?;
        assert!(
            file_doc.anchors.contains_key("_yay"),
            "file.adoc should have _yay anchor, found: {:?}",
            file_doc.anchors.keys().collect::<Vec<_>>()
        );
        drop(file_doc);

        // Verify xrefs were collected in a.adoc
        let a_doc = workspace
            .get_document(&a_uri)
            .ok_or("a.adoc should be indexed")?;
        assert!(!a_doc.xrefs.is_empty(), "a.adoc should have xrefs");

        // Verify the AST has the xref findable at offset
        let ast = a_doc.ast.as_ref().ok_or("a.adoc should have AST")?;
        let xref_offset = a_content
            .find("xref:")
            .ok_or("xref: not found in content")?;
        let found = crate::capabilities::hover::find_xref_at_offset(ast, xref_offset + 10);
        assert!(found.is_some(), "xref should be findable at offset");

        // Use position_to_offset round-trip like the real code path
        let position = tower_lsp::lsp_types::Position {
            line: 4,       // 0-indexed: line 5 of the document
            character: 20, // middle of the xref
        };
        let result = find_definition_at_position(&a_doc, &a_uri, &workspace, position);
        let (uri, _loc) = result.ok_or("expected cross-file xref to resolve")?;
        assert_eq!(uri, file_uri);
        Ok(())
    }
}
