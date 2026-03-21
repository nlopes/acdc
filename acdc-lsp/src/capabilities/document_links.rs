//! Document Links: make URLs, file references, and includes clickable

use acdc_parser::{Block, DelimitedBlockType, InlineMacro, InlineNode, Location};
use tower_lsp::lsp_types::{DocumentLink, Url};

use crate::convert::{location_to_range, resolve_relative_uri};
use crate::state::DocumentState;

/// Collected link information
struct LinkInfo {
    target: String,
    location: Location,
    tooltip: Option<String>,
}

/// Collect all document links (clickable URLs, file references, and includes)
#[must_use]
pub fn collect_document_links(doc: &DocumentState, doc_uri: &Url) -> Vec<DocumentLink> {
    let mut links = Vec::new();

    // Collect links from AST (URLs, link macros, images)
    if let Some(ast) = &doc.ast {
        collect_links_from_blocks(&ast.blocks, &mut links);
    }

    let mut result: Vec<DocumentLink> = links
        .into_iter()
        .filter_map(|info| {
            let target = if info.target.starts_with("http://")
                || info.target.starts_with("https://")
                || info.target.starts_with("mailto:")
                || info.target.starts_with("ftp://")
                || info.target.starts_with("file://")
            {
                info.target.parse().ok()
            } else {
                // Resolve relative paths against the document's directory
                resolve_relative_uri(doc_uri, &info.target)
            };

            target.map(|uri| DocumentLink {
                range: location_to_range(&info.location),
                target: Some(uri),
                tooltip: info.tooltip,
                data: None,
            })
        })
        .collect();

    // Add include directives as clickable links
    for (include_target, include_loc) in &doc.includes {
        if let Some(target_uri) = resolve_relative_uri(doc_uri, include_target) {
            result.push(DocumentLink {
                range: location_to_range(include_loc),
                target: Some(target_uri),
                tooltip: Some(format!("Open included file: {include_target}")),
                data: None,
            });
        }
    }

    result
}

fn collect_links_from_blocks(blocks: &[Block], links: &mut Vec<LinkInfo>) {
    for block in blocks {
        collect_links_from_block(block, links);
    }
}

fn collect_links_from_block(block: &Block, links: &mut Vec<LinkInfo>) {
    match block {
        Block::Section(section) => {
            collect_links_from_blocks(&section.content, links);
        }
        Block::Paragraph(para) => {
            collect_links_from_inlines(&para.content, links);
        }
        Block::DelimitedBlock(delimited) => {
            collect_links_from_delimited(&delimited.inner, links);
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                collect_links_from_inlines(&item.principal, links);
                collect_links_from_blocks(&item.blocks, links);
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                collect_links_from_inlines(&item.principal, links);
                collect_links_from_blocks(&item.blocks, links);
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                collect_links_from_inlines(&item.principal_text, links);
                collect_links_from_blocks(&item.description, links);
            }
        }
        Block::Admonition(adm) => {
            collect_links_from_blocks(&adm.blocks, links);
        }
        Block::Image(img) => {
            // Image source as a link (for opening the image file)
            links.push(LinkInfo {
                target: img.source.to_string(),
                location: img.location.clone(),
                tooltip: Some("Open image".to_string()),
            });
        }
        Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::CalloutList(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_links_from_delimited(inner: &DelimitedBlockType, links: &mut Vec<LinkInfo>) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            collect_links_from_blocks(blocks, links);
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => {
            collect_links_from_inlines(inlines, links);
        }
        DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_links_from_inlines(inlines: &[InlineNode], links: &mut Vec<LinkInfo>) {
    for inline in inlines {
        collect_links_from_inline(inline, links);
    }
}

fn collect_links_from_inline(inline: &InlineNode, links: &mut Vec<LinkInfo>) {
    match inline {
        InlineNode::Macro(InlineMacro::Link(link)) => {
            links.push(LinkInfo {
                target: link.target.to_string(),
                location: link.location.clone(),
                tooltip: link.text.clone(),
            });
        }
        InlineNode::Macro(InlineMacro::Url(url)) => {
            links.push(LinkInfo {
                target: url.target.to_string(),
                location: url.location.clone(),
                tooltip: None,
            });
        }
        InlineNode::Macro(InlineMacro::Autolink(autolink)) => {
            links.push(LinkInfo {
                target: autolink.url.to_string(),
                location: autolink.location.clone(),
                tooltip: None,
            });
        }
        InlineNode::Macro(InlineMacro::Mailto(mailto)) => {
            links.push(LinkInfo {
                target: format!("mailto:{}", mailto.target),
                location: mailto.location.clone(),
                tooltip: None, // Text is Vec<InlineNode>, skip tooltip extraction
            });
        }
        // Recurse into formatted text
        InlineNode::BoldText(b) => collect_links_from_inlines(&b.content, links),
        InlineNode::ItalicText(i) => collect_links_from_inlines(&i.content, links),
        InlineNode::MonospaceText(m) => collect_links_from_inlines(&m.content, links),
        InlineNode::HighlightText(h) => collect_links_from_inlines(&h.content, links),
        InlineNode::SubscriptText(s) => collect_links_from_inlines(&s.content, links),
        InlineNode::SuperscriptText(s) => collect_links_from_inlines(&s.content, links),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;

    #[test]
    fn test_collect_urls() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"= Document

Visit https://example.com for more info.

Also see link:https://rust-lang.org[Rust].
";
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let links = collect_document_links(&doc, &uri);
        assert_eq!(links.len(), 2);
        Ok(())
    }

    #[test]
    fn test_collect_mailto() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"= Document

Contact mailto:test@example.com[us] for help.
";
        let workspace = Workspace::new();
        let uri = Url::parse("file:///test.adoc")?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let links = collect_document_links(&doc, &uri);
        assert_eq!(links.len(), 1);

        let link = links.first();
        assert!(link.is_some(), "expected at least one link");
        assert!(link.is_some_and(|l| {
            l.target
                .as_ref()
                .is_some_and(|u| u.as_str().starts_with("mailto:"))
        }));
        Ok(())
    }

    #[test]
    fn test_include_directives_as_links() -> Result<(), Box<dyn std::error::Error>> {
        let content = "= Document\n\ninclude::chapter1.adoc[]\n\nSome text.\n";
        let workspace = Workspace::new();
        let uri = Url::parse("file:///docs/main.adoc")?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let links = collect_document_links(&doc, &uri);

        // Should have the include directive as a clickable link
        let include_links: Vec<_> = links
            .iter()
            .filter(|l| {
                l.tooltip
                    .as_deref()
                    .is_some_and(|t| t.contains("included file"))
            })
            .collect();
        assert_eq!(include_links.len(), 1, "Expected 1 include link");

        let include_link = include_links.first().ok_or("expected include link")?;
        assert!(
            include_link
                .target
                .as_ref()
                .is_some_and(|u| u.as_str().ends_with("chapter1.adoc"))
        );
        Ok(())
    }

    #[test]
    fn test_resolve_relative_uri() -> Result<(), Box<dyn std::error::Error>> {
        let doc_uri = Url::parse("file:///docs/main.adoc")?;
        let resolved = resolve_relative_uri(&doc_uri, "chapter.adoc");
        assert!(resolved.is_some());
        assert_eq!(
            resolved.map(|u| u.to_string()),
            Some("file:///docs/chapter.adoc".to_string())
        );
        Ok(())
    }
}
