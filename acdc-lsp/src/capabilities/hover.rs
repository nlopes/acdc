//! Hover: show information about elements at cursor position

use acdc_parser::{
    Block, DelimitedBlockType, Document, InlineMacro, InlineNode, Location, inlines_to_string,
};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::convert::{location_to_range, offset_in_location, position_to_offset};
use crate::state::DocumentState;

/// Compute hover information for a position
#[must_use]
pub fn compute_hover(doc: &DocumentState, position: Position) -> Option<Hover> {
    let offset = position_to_offset(&doc.text, position)?;
    let ast = doc.ast.as_ref()?;

    // Check for xref at this position
    if let Some((target, xref_loc)) = find_xref_at_offset(ast, offset) {
        // Look up the target anchor to get section title
        let content = if let Some(anchor_loc) = doc.anchors.get(&target) {
            // Try to find the section title for this anchor
            if let Some(title) = find_section_title_at_location(ast, anchor_loc) {
                format!("**Cross-reference**\n\nTarget: `{target}`\n\nSection: {title}")
            } else {
                format!("**Cross-reference**\n\nTarget: `{target}`")
            }
        } else {
            format!("**Cross-reference** (unresolved)\n\nTarget: `{target}`")
        };

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(location_to_range(&xref_loc)),
        });
    }

    // Check for anchor at this position
    if let Some((id, anchor_loc)) = find_anchor_at_offset(ast, offset, doc) {
        // Count references to this anchor
        let ref_count = doc.xrefs.iter().filter(|(t, _)| t == &id).count();
        let refs_text = match ref_count {
            0 => "No references".to_string(),
            1 => "1 reference".to_string(),
            n => format!("{n} references"),
        };

        let content = format!("**Anchor**\n\nID: `{id}`\n\n{refs_text}");

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(location_to_range(&anchor_loc)),
        });
    }

    // Check for URL/link at this position
    if let Some((url, link_loc)) = find_link_at_offset(ast, offset) {
        let content = format!("**Link**\n\nURL: {url}");

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(location_to_range(&link_loc)),
        });
    }

    None
}

/// Find xref at a byte offset
pub(crate) fn find_xref_at_offset(doc: &Document, offset: usize) -> Option<(String, Location)> {
    for block in &doc.blocks {
        if let Some(result) = find_xref_in_block(block, offset) {
            return Some(result);
        }
    }
    None
}

fn find_xref_in_block(block: &Block, offset: usize) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match block {
        Block::Section(section) => {
            for child in &section.content {
                if let Some(result) = find_xref_in_block(child, offset) {
                    return Some(result);
                }
            }
            None
        }
        Block::Paragraph(para) => find_xref_in_inlines(&para.content, offset),
        Block::DelimitedBlock(delimited) => find_xref_in_delimited(&delimited.inner, offset),
        Block::UnorderedList(list) => {
            for item in &list.items {
                if let Some(result) = find_xref_in_inlines(&item.principal, offset) {
                    return Some(result);
                }
                for b in &item.blocks {
                    if let Some(result) = find_xref_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                if let Some(result) = find_xref_in_inlines(&item.principal, offset) {
                    return Some(result);
                }
                for b in &item.blocks {
                    if let Some(result) = find_xref_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                if let Some(result) = find_xref_in_inlines(&item.principal_text, offset) {
                    return Some(result);
                }
                for b in &item.description {
                    if let Some(result) = find_xref_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::Admonition(adm) => {
            for b in &adm.blocks {
                if let Some(result) = find_xref_in_block(b, offset) {
                    return Some(result);
                }
            }
            None
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
        | Block::Comment(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn find_xref_in_delimited(inner: &DelimitedBlockType, offset: usize) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                if let Some(result) = find_xref_in_block(block, offset) {
                    return Some(result);
                }
            }
            None
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => find_xref_in_inlines(inlines, offset),
        DelimitedBlockType::DelimitedTable(_) | DelimitedBlockType::DelimitedStem(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn find_xref_in_inlines(inlines: &[InlineNode], offset: usize) -> Option<(String, Location)> {
    for inline in inlines {
        if let Some(result) = find_xref_in_inline(inline, offset) {
            return Some(result);
        }
    }
    None
}

fn find_xref_in_inline(inline: &InlineNode, offset: usize) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match inline {
        InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
            if offset_in_location(offset, &xref.location) {
                return Some((xref.target.clone(), xref.location.clone()));
            }
            None
        }
        InlineNode::BoldText(b) => find_xref_in_inlines(&b.content, offset),
        InlineNode::ItalicText(i) => find_xref_in_inlines(&i.content, offset),
        InlineNode::MonospaceText(m) => find_xref_in_inlines(&m.content, offset),
        InlineNode::HighlightText(h) => find_xref_in_inlines(&h.content, offset),
        InlineNode::SubscriptText(s) => find_xref_in_inlines(&s.content, offset),
        InlineNode::SuperscriptText(s) => find_xref_in_inlines(&s.content, offset),
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// Find anchor definition at a byte offset
pub(crate) fn find_anchor_at_offset(
    doc: &Document,
    offset: usize,
    doc_state: &DocumentState,
) -> Option<(String, Location)> {
    // Check if the offset is within any anchor location
    for (id, loc) in &doc_state.anchors {
        if offset_in_location(offset, loc) {
            return Some((id.clone(), loc.clone()));
        }
    }

    // Also check inline anchors in the AST
    for block in &doc.blocks {
        if let Some(result) = find_inline_anchor_in_block(block, offset) {
            return Some(result);
        }
    }
    None
}

fn find_inline_anchor_in_block(block: &Block, offset: usize) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match block {
        Block::Section(section) => {
            for child in &section.content {
                if let Some(result) = find_inline_anchor_in_block(child, offset) {
                    return Some(result);
                }
            }
            None
        }
        Block::Paragraph(para) => find_inline_anchor_in_inlines(&para.content, offset),
        Block::DelimitedBlock(delimited) => {
            find_inline_anchor_in_delimited(&delimited.inner, offset)
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                if let Some(result) = find_inline_anchor_in_inlines(&item.principal, offset) {
                    return Some(result);
                }
                for b in &item.blocks {
                    if let Some(result) = find_inline_anchor_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                if let Some(result) = find_inline_anchor_in_inlines(&item.principal, offset) {
                    return Some(result);
                }
                for b in &item.blocks {
                    if let Some(result) = find_inline_anchor_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                if let Some(result) = find_inline_anchor_in_inlines(&item.principal_text, offset) {
                    return Some(result);
                }
                for b in &item.description {
                    if let Some(result) = find_inline_anchor_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::Admonition(adm) => {
            for b in &adm.blocks {
                if let Some(result) = find_inline_anchor_in_block(b, offset) {
                    return Some(result);
                }
            }
            None
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
        | Block::Comment(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn find_inline_anchor_in_delimited(
    inner: &DelimitedBlockType,
    offset: usize,
) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                if let Some(result) = find_inline_anchor_in_block(block, offset) {
                    return Some(result);
                }
            }
            None
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => {
            find_inline_anchor_in_inlines(inlines, offset)
        }
        DelimitedBlockType::DelimitedTable(_) | DelimitedBlockType::DelimitedStem(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn find_inline_anchor_in_inlines(
    inlines: &[InlineNode],
    offset: usize,
) -> Option<(String, Location)> {
    for inline in inlines {
        if let Some(result) = find_inline_anchor_in_inline(inline, offset) {
            return Some(result);
        }
    }
    None
}

fn find_inline_anchor_in_inline(inline: &InlineNode, offset: usize) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match inline {
        InlineNode::InlineAnchor(anchor) => {
            if offset_in_location(offset, &anchor.location) {
                return Some((anchor.id.clone(), anchor.location.clone()));
            }
            None
        }
        InlineNode::BoldText(b) => find_inline_anchor_in_inlines(&b.content, offset),
        InlineNode::ItalicText(i) => find_inline_anchor_in_inlines(&i.content, offset),
        InlineNode::MonospaceText(m) => find_inline_anchor_in_inlines(&m.content, offset),
        InlineNode::HighlightText(h) => find_inline_anchor_in_inlines(&h.content, offset),
        InlineNode::SubscriptText(s) => find_inline_anchor_in_inlines(&s.content, offset),
        InlineNode::SuperscriptText(s) => find_inline_anchor_in_inlines(&s.content, offset),
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::Macro(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// Find link/URL at a byte offset
fn find_link_at_offset(doc: &Document, offset: usize) -> Option<(String, Location)> {
    for block in &doc.blocks {
        if let Some(result) = find_link_in_block(block, offset) {
            return Some(result);
        }
    }
    None
}

fn find_link_in_block(block: &Block, offset: usize) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match block {
        Block::Section(section) => {
            for child in &section.content {
                if let Some(result) = find_link_in_block(child, offset) {
                    return Some(result);
                }
            }
            None
        }
        Block::Paragraph(para) => find_link_in_inlines(&para.content, offset),
        Block::DelimitedBlock(delimited) => find_link_in_delimited(&delimited.inner, offset),
        Block::UnorderedList(list) => {
            for item in &list.items {
                if let Some(result) = find_link_in_inlines(&item.principal, offset) {
                    return Some(result);
                }
                for b in &item.blocks {
                    if let Some(result) = find_link_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                if let Some(result) = find_link_in_inlines(&item.principal, offset) {
                    return Some(result);
                }
                for b in &item.blocks {
                    if let Some(result) = find_link_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                if let Some(result) = find_link_in_inlines(&item.principal_text, offset) {
                    return Some(result);
                }
                for b in &item.description {
                    if let Some(result) = find_link_in_block(b, offset) {
                        return Some(result);
                    }
                }
            }
            None
        }
        Block::Admonition(adm) => {
            for b in &adm.blocks {
                if let Some(result) = find_link_in_block(b, offset) {
                    return Some(result);
                }
            }
            None
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
        | Block::Comment(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn find_link_in_delimited(inner: &DelimitedBlockType, offset: usize) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                if let Some(result) = find_link_in_block(block, offset) {
                    return Some(result);
                }
            }
            None
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => find_link_in_inlines(inlines, offset),
        DelimitedBlockType::DelimitedTable(_) | DelimitedBlockType::DelimitedStem(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn find_link_in_inlines(inlines: &[InlineNode], offset: usize) -> Option<(String, Location)> {
    for inline in inlines {
        if let Some(result) = find_link_in_inline(inline, offset) {
            return Some(result);
        }
    }
    None
}

fn find_link_in_inline(inline: &InlineNode, offset: usize) -> Option<(String, Location)> {
    #[allow(clippy::match_same_arms)]
    match inline {
        InlineNode::Macro(InlineMacro::Link(link)) => {
            if offset_in_location(offset, &link.location) {
                return Some((link.target.to_string(), link.location.clone()));
            }
            None
        }
        InlineNode::Macro(InlineMacro::Url(url)) => {
            if offset_in_location(offset, &url.location) {
                return Some((url.target.to_string(), url.location.clone()));
            }
            None
        }
        InlineNode::Macro(InlineMacro::Autolink(autolink)) => {
            if offset_in_location(offset, &autolink.location) {
                return Some((autolink.url.to_string(), autolink.location.clone()));
            }
            None
        }
        InlineNode::BoldText(b) => find_link_in_inlines(&b.content, offset),
        InlineNode::ItalicText(i) => find_link_in_inlines(&i.content, offset),
        InlineNode::MonospaceText(m) => find_link_in_inlines(&m.content, offset),
        InlineNode::HighlightText(h) => find_link_in_inlines(&h.content, offset),
        InlineNode::SubscriptText(s) => find_link_in_inlines(&s.content, offset),
        InlineNode::SuperscriptText(s) => find_link_in_inlines(&s.content, offset),
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// Find section title at a given location
fn find_section_title_at_location(doc: &Document, loc: &Location) -> Option<String> {
    for block in &doc.blocks {
        if let Some(title) = find_section_title_in_block(block, loc) {
            return Some(title);
        }
    }
    None
}

fn find_section_title_in_block(block: &Block, target_loc: &Location) -> Option<String> {
    #[allow(clippy::match_same_arms)]
    match block {
        Block::Section(section) => {
            // Check if this section's location matches
            if section.location.absolute_start == target_loc.absolute_start {
                return Some(inlines_to_string(&section.title));
            }
            // Recurse into children
            for child in &section.content {
                if let Some(title) = find_section_title_in_block(child, target_loc) {
                    return Some(title);
                }
            }
            None
        }
        Block::Paragraph(_)
        | Block::DelimitedBlock(_)
        | Block::UnorderedList(_)
        | Block::OrderedList(_)
        | Block::DescriptionList(_)
        | Block::Admonition(_)
        | Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::CalloutList(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Options;

    fn create_test_doc_state(content: &str) -> DocumentState {
        let options = Options::default();
        let result = acdc_parser::parse(content, &options);

        match result {
            Ok(doc) => {
                let anchors = crate::capabilities::definition::collect_anchors(&doc);
                let xrefs = crate::capabilities::definition::collect_xrefs(&doc);
                DocumentState::new_success(content.to_string(), 1, doc, anchors, xrefs)
            }
            Err(_) => DocumentState::new_failure(content.to_string(), 1, vec![]),
        }
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_hover_on_xref() {
        let content = r"[[my-section]]
== My Section

See <<my-section>> for details.
";
        let doc = create_test_doc_state(content);

        // Position on the xref (line 3, somewhere in <<my-section>>)
        let position = Position {
            line: 3,
            character: 8,
        };

        let result = compute_hover(&doc, position);
        let hover = result.expect("Expected hover result for xref position");
        #[allow(clippy::unreachable)]
        let HoverContents::Markup(markup) = hover.contents else {
            unreachable!("Expected HoverContents::Markup")
        };
        assert!(markup.value.contains("Cross-reference"));
        assert!(markup.value.contains("my-section"));
    }
}
