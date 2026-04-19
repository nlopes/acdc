//! Hover: show information about elements at cursor position

use acdc_parser::{
    Block, DelimitedBlockType, Document, InlineMacro, InlineNode, Location, inlines_to_string,
};
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Uri};

use crate::convert::{location_to_range, offset_in_location, position_to_offset};
use crate::state::{DocumentState, Workspace, XrefTarget};

/// Compute hover information for a position
#[must_use]
pub(crate) fn compute_hover(
    doc: &DocumentState,
    doc_uri: &Uri,
    workspace: &Workspace,
    position: Position,
) -> Option<Hover> {
    let offset = position_to_offset(doc.text(), position)?;
    let ast_guard = doc.ast()?;
    let ast = ast_guard.document();

    // Check for xref at this position
    if let Some((target, xref_loc)) = find_xref_at_offset(ast, offset) {
        let parsed = XrefTarget::parse(&target);
        let content = build_xref_hover_content(&target, &parsed, doc, doc_uri, workspace, ast);

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
        // Count references to this anchor (local + cross-file)
        let local_refs = doc.xrefs.iter().filter(|(t, _)| t == &id).count();
        let mut cross_file_refs = 0usize;
        workspace.for_each_document(|uri, other_doc| {
            if uri != doc_uri {
                cross_file_refs += other_doc
                    .xrefs
                    .iter()
                    .filter(|(t, _)| {
                        let parsed = XrefTarget::parse(t);
                        parsed.anchor.as_deref() == Some(id.as_str())
                    })
                    .count();
            }
        });

        let total_refs = local_refs + cross_file_refs;
        let refs_text = match total_refs {
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

    // Check for attribute reference at this position
    if let Some((attr_name, attr_ref_loc)) = find_attribute_ref_at_offset(doc, offset) {
        let content = if let Some(value) = ast.attributes.get(&attr_name) {
            format!("**Attribute**\n\n`:{attr_name}:` = `{value}`")
        } else {
            format!("**Attribute** (undefined)\n\n`{{{attr_name}}}`")
        };

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(location_to_range(&attr_ref_loc)),
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

/// Build hover content for an xref, with cross-file awareness
fn build_xref_hover_content(
    raw_target: &str,
    parsed: &XrefTarget,
    doc: &DocumentState,
    doc_uri: &Uri,
    workspace: &Workspace,
    ast: &Document,
) -> String {
    if let Some(file_path) = &parsed.file {
        // Cross-file xref
        let file_info = format!("File: `{file_path}`");
        if let Some(anchor_id) = &parsed.anchor {
            if let Some(target_uri) = crate::convert::resolve_relative_uri(doc_uri, file_path) {
                // find_anchor_in_document checks open docs then falls back to disk
                if workspace
                    .find_anchor_in_document(&target_uri, anchor_id)
                    .is_some()
                {
                    format!("**Cross-reference**\n\n{file_info}\n\nTarget: `{anchor_id}`")
                } else {
                    format!(
                        "**Cross-reference** (anchor not found)\n\n{file_info}\n\nTarget: `{anchor_id}`"
                    )
                }
            } else {
                format!(
                    "**Cross-reference** (cannot resolve file)\n\n{file_info}\n\nTarget: `{anchor_id}`"
                )
            }
        } else {
            format!("**Cross-reference**\n\n{file_info}")
        }
    } else if let Some(anchor_id) = &parsed.anchor {
        // Local xref
        if let Some(anchor_loc) = doc.anchors.get(anchor_id) {
            if let Some(title) = find_section_title_at_location(ast, anchor_loc) {
                format!("**Cross-reference**\n\nTarget: `{anchor_id}`\n\nSection: {title}")
            } else {
                format!("**Cross-reference**\n\nTarget: `{anchor_id}`")
            }
        } else {
            // Try workspace-wide
            let global = workspace.find_anchor_globally(anchor_id);
            if let Some((uri, _)) = global.first() {
                format!(
                    "**Cross-reference**\n\nTarget: `{anchor_id}`\n\nDefined in: `{}`",
                    uri.as_str()
                )
            } else {
                format!("**Cross-reference** (unresolved)\n\nTarget: `{raw_target}`")
            }
        }
    } else {
        format!("**Cross-reference** (unresolved)\n\nTarget: `{raw_target}`")
    }
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
        | Block::Comment(_)
        // non_exhaustive
        | _ => None,
    }
}

fn find_xref_in_delimited(inner: &DelimitedBlockType, offset: usize) -> Option<(String, Location)> {
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
        DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => None,
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
    match inline {
        InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
            if offset_in_location(offset, &xref.location) {
                return Some((xref.target.to_string(), xref.location.clone()));
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
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        // non_exhaustive
        | _ => None,
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
        | Block::Comment(_)
        // non_exhaustive
        | _ => None,
    }
}

fn find_inline_anchor_in_delimited(
    inner: &DelimitedBlockType,
    offset: usize,
) -> Option<(String, Location)> {
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
        DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => None,
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
    match inline {
        InlineNode::InlineAnchor(anchor) => {
            if offset_in_location(offset, &anchor.location) {
                return Some((anchor.id.to_string(), anchor.location.clone()));
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
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        // non_exhaustive
        | _ => None,
    }
}

/// Find link/URL at a byte offset
pub(crate) fn find_link_at_offset(doc: &Document, offset: usize) -> Option<(String, Location)> {
    for block in &doc.blocks {
        if let Some(result) = find_link_in_block(block, offset) {
            return Some(result);
        }
    }
    None
}

fn find_link_in_block(block: &Block, offset: usize) -> Option<(String, Location)> {
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
        | Block::Comment(_)
        // non_exhaustive
        | _ => None,
    }
}

fn find_link_in_delimited(inner: &DelimitedBlockType, offset: usize) -> Option<(String, Location)> {
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
        DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => None,
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
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        // non_exhaustive
        | _ => None,
    }
}

/// Find attribute reference at a byte offset using pre-extracted attribute refs.
fn find_attribute_ref_at_offset(doc: &DocumentState, offset: usize) -> Option<(String, Location)> {
    doc.attribute_refs.iter().find_map(|(name, loc)| {
        if offset_in_location(offset, loc) {
            Some((name.clone(), loc.clone()))
        } else {
            None
        }
    })
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
        Block::TableOfContents(_)
        | Block::Admonition(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::UnorderedList(_)
        | Block::OrderedList(_)
        | Block::CalloutList(_)
        | Block::DescriptionList(_)
        | Block::DelimitedBlock(_)
        | Block::Paragraph(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        // non_exhaustive
        | _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hover_on_xref() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[my-section]]
== My Section

See <<my-section>> for details.
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on the xref (line 3, somewhere in <<my-section>>)
        let position = Position {
            line: 3,
            character: 8,
        };

        let result = compute_hover(&doc, &uri, &workspace, position);
        assert!(result.is_some(), "Expected hover result for xref position");
        let hover = result;
        assert!(
            matches!(&hover, Some(h) if matches!(&h.contents, HoverContents::Markup(_))),
            "Expected HoverContents::Markup"
        );
        if let Some(hover) = hover
            && let HoverContents::Markup(markup) = hover.contents
        {
            assert!(markup.value.contains("Cross-reference"));
            assert!(markup.value.contains("my-section"));
        }
        Ok(())
    }

    #[test]
    fn test_hover_on_attribute_ref() -> Result<(), Box<dyn std::error::Error>> {
        let content = ":imagesdir: ./images\n\n== Section\n\nImage in {imagesdir}/logo.png\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on {imagesdir} — line 4 (0-indexed), character 10
        let position = Position {
            line: 4,
            character: 10,
        };

        let result = compute_hover(&doc, &uri, &workspace, position);
        assert!(result.is_some(), "Expected hover result for attribute ref");

        if let Some(hover) = result
            && let HoverContents::Markup(markup) = hover.contents
        {
            assert!(
                markup.value.contains("Attribute"),
                "Expected 'Attribute' in hover, got: {}",
                markup.value
            );
            assert!(
                markup.value.contains("imagesdir"),
                "Expected attribute name in hover, got: {}",
                markup.value
            );
            assert!(
                markup.value.contains("./images"),
                "Expected attribute value in hover, got: {}",
                markup.value
            );
        }
        Ok(())
    }

    #[test]
    fn test_hover_on_undefined_attribute_ref() -> Result<(), Box<dyn std::error::Error>> {
        let content = "== Section\n\nSee {undefined-attr} here.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on {undefined-attr} — line 2, character 6
        let position = Position {
            line: 2,
            character: 6,
        };

        let result = compute_hover(&doc, &uri, &workspace, position);
        assert!(
            result.is_some(),
            "Expected hover result for undefined attribute ref"
        );

        if let Some(hover) = result
            && let HoverContents::Markup(markup) = hover.contents
        {
            assert!(
                markup.value.contains("undefined"),
                "Expected 'undefined' in hover, got: {}",
                markup.value
            );
        }
        Ok(())
    }
}
