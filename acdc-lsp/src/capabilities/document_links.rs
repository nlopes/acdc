//! Document Links: make URLs and file references clickable

use acdc_parser::{Block, DelimitedBlockType, Document, InlineMacro, InlineNode, Location};
use tower_lsp::lsp_types::DocumentLink;

use crate::convert::location_to_range;

/// Collected link information
struct LinkInfo {
    target: String,
    location: Location,
    tooltip: Option<String>,
}

/// Collect all document links (clickable URLs and file references)
#[must_use]
pub fn collect_document_links(doc: &Document) -> Vec<DocumentLink> {
    let mut links = Vec::new();
    collect_links_from_blocks(&doc.blocks, &mut links);
    links
        .into_iter()
        .filter_map(|info| {
            // Only include links with valid URL schemes or relative paths
            let target = if info.target.starts_with("http://")
                || info.target.starts_with("https://")
                || info.target.starts_with("mailto:")
                || info.target.starts_with("ftp://")
                || info.target.starts_with("file://")
            {
                info.target.parse().ok()
            } else {
                // For relative paths, we'd need the document URI - skip for now
                None
            };

            target.map(|uri| DocumentLink {
                range: location_to_range(&info.location),
                target: Some(uri),
                tooltip: info.tooltip,
                data: None,
            })
        })
        .collect()
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
        // Other block types don't contain links (Block is non_exhaustive)
        _ => {}
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
        // Other types don't contain links (DelimitedBlockType is non_exhaustive)
        _ => {}
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
        // No links in these (InlineNode is non_exhaustive)
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Options;

    #[test]
    fn test_collect_urls() -> Result<(), acdc_parser::Error> {
        let content = r"= Document

Visit https://example.com for more info.

Also see link:https://rust-lang.org[Rust].
";
        let options = Options::default();
        let doc = acdc_parser::parse(content, &options)?;

        let links = collect_document_links(&doc);
        assert_eq!(links.len(), 2);
        Ok(())
    }

    #[test]
    fn test_collect_mailto() -> Result<(), acdc_parser::Error> {
        let content = r"= Document

Contact mailto:test@example.com[us] for help.
";
        let options = Options::default();
        let doc = acdc_parser::parse(content, &options)?;

        let links = collect_document_links(&doc);
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
}
