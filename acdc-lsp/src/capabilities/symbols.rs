//! Document symbols: extract full block tree from AST

use acdc_parser::{
    Block, DelimitedBlock, DelimitedBlockType, Document, Section, inlines_to_string,
};
use tower_lsp_server::ls_types::{DocumentSymbol, SymbolKind};

use crate::convert::location_to_range;

/// Maximum length for paragraph preview text in symbol names
const PARAGRAPH_PREVIEW_LEN: usize = 50;

/// Extract document outline as nested symbols
#[must_use]
pub fn document_symbols(doc: &Document) -> Vec<DocumentSymbol> {
    let mut symbols = vec![];

    // Add header as top-level symbol if present
    if let Some(header) = &doc.header {
        let title_text = inlines_to_string(&header.title);

        symbols.push(DocumentSymbol {
            name: title_text,
            kind: SymbolKind::FILE,
            range: location_to_range(&header.location),
            selection_range: location_to_range(&header.location),
            children: None,
            detail: Some("Document title".to_string()),
            tags: Some(vec![]),
            deprecated: None,
        });
    }

    // Process blocks recursively
    for block in &doc.blocks {
        if let Some(symbol) = block_to_symbol(block) {
            symbols.push(symbol);
        }
    }

    symbols
}

/// Convert children blocks to nested symbols, returning `None` if empty.
fn children_from_blocks(blocks: &[Block]) -> Option<Vec<DocumentSymbol>> {
    let children: Vec<DocumentSymbol> = blocks.iter().filter_map(block_to_symbol).collect();
    if children.is_empty() {
        None
    } else {
        Some(children)
    }
}

/// Build a `DocumentSymbol` with common defaults.
fn make_symbol(
    name: String,
    kind: SymbolKind,
    location: &acdc_parser::Location,
    detail: Option<String>,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    let range = location_to_range(location);
    DocumentSymbol {
        name,
        kind,
        range,
        selection_range: range,
        detail,
        children,
        tags: Some(vec![]),
        deprecated: None,
    }
}

/// Truncate text to `max_len` chars, appending "..." if truncated.
fn truncate(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len).collect();
        format!("{truncated}...")
    }
}

/// Get a display name from a title, falling back to `default`.
fn title_or_default(title: &acdc_parser::Title, default: &str) -> String {
    let text = inlines_to_string(title);
    if text.is_empty() {
        default.to_string()
    } else {
        text
    }
}

/// Get a display name from a title, falling back to a source string.
fn title_or_source(title: &acdc_parser::Title, source: &str) -> String {
    if title.is_empty() {
        source.to_string()
    } else {
        inlines_to_string(title)
    }
}

fn block_to_symbol(block: &Block) -> Option<DocumentSymbol> {
    match block {
        Block::Section(section) => Some(section_to_symbol(section)),
        Block::Paragraph(para) => Some(paragraph_to_symbol(para)),
        Block::Admonition(adm) => Some(admonition_to_symbol(adm)),
        Block::DelimitedBlock(delimited) => Some(delimited_block_to_symbol(delimited)),
        Block::UnorderedList(list) => Some(list_to_symbol(
            &list.title,
            "Unordered list",
            list.items.len(),
            &list.items.iter().flat_map(|i| &i.blocks).collect::<Vec<_>>(),
            &list.location,
        )),
        Block::OrderedList(list) => Some(list_to_symbol(
            &list.title,
            "Ordered list",
            list.items.len(),
            &list.items.iter().flat_map(|i| &i.blocks).collect::<Vec<_>>(),
            &list.location,
        )),
        Block::DescriptionList(list) => Some(list_to_symbol(
            &list.title,
            "Description list",
            list.items.len(),
            &list
                .items
                .iter()
                .flat_map(|i| &i.description)
                .collect::<Vec<_>>(),
            &list.location,
        )),
        Block::CalloutList(list) => Some(make_symbol(
            title_or_default(&list.title, "Callout list"),
            SymbolKind::ARRAY,
            &list.location,
            Some(format!("{} items", list.items.len())),
            None,
        )),
        Block::Image(img) => Some(make_symbol(
            title_or_source(&img.title, &img.source.to_string()),
            SymbolKind::FILE,
            &img.location,
            Some("Image".to_string()),
            None,
        )),
        Block::Audio(audio) => Some(make_symbol(
            title_or_source(&audio.title, &audio.source.to_string()),
            SymbolKind::FILE,
            &audio.location,
            Some("Audio".to_string()),
            None,
        )),
        Block::Video(video) => Some(video_to_symbol(video)),
        Block::DiscreteHeader(header) => Some(make_symbol(
            inlines_to_string(&header.title),
            SymbolKind::KEY,
            &header.location,
            Some(format!("Discrete heading (level {})", header.level)),
            None,
        )),
        Block::DocumentAttribute(attr) => Some(make_symbol(
            format!(":{}: {}", attr.name, attr.value),
            SymbolKind::PROPERTY,
            &attr.location,
            Some("Attribute".to_string()),
            None,
        )),
        Block::TableOfContents(toc) => Some(make_symbol(
            "Table of contents".to_string(),
            SymbolKind::STRUCT,
            &toc.location,
            None,
            None,
        )),
        Block::ThematicBreak(tb) => Some(make_symbol(
            "---".to_string(),
            SymbolKind::NULL,
            &tb.location,
            Some("Thematic break".to_string()),
            None,
        )),
        Block::PageBreak(pb) => Some(make_symbol(
            "<<<".to_string(),
            SymbolKind::NULL,
            &pb.location,
            Some("Page break".to_string()),
            None,
        )),
        Block::Comment(_)
        // non_exhaustive
        | _ => None,
    }
}

fn paragraph_to_symbol(para: &acdc_parser::Paragraph) -> DocumentSymbol {
    let name = if para.title.is_empty() {
        truncate(&inlines_to_string(&para.content), PARAGRAPH_PREVIEW_LEN)
    } else {
        inlines_to_string(&para.title)
    };
    make_symbol(
        name,
        SymbolKind::STRING,
        &para.location,
        Some("Paragraph".to_string()),
        None,
    )
}

fn admonition_to_symbol(adm: &acdc_parser::Admonition) -> DocumentSymbol {
    let label = format!("{}", adm.variant).to_uppercase();
    let name = if adm.title.is_empty() {
        label
    } else {
        format!("{label}: {}", inlines_to_string(&adm.title))
    };
    make_symbol(
        name,
        SymbolKind::EVENT,
        &adm.location,
        Some("Admonition".to_string()),
        children_from_blocks(&adm.blocks),
    )
}

fn list_to_symbol(
    title: &acdc_parser::Title,
    default_name: &str,
    item_count: usize,
    child_blocks: &[&Block],
    location: &acdc_parser::Location,
) -> DocumentSymbol {
    let children: Vec<DocumentSymbol> = child_blocks
        .iter()
        .filter_map(|b| block_to_symbol(b))
        .collect();
    make_symbol(
        title_or_default(title, default_name),
        SymbolKind::ARRAY,
        location,
        Some(format!("{item_count} items")),
        if children.is_empty() {
            None
        } else {
            Some(children)
        },
    )
}

fn video_to_symbol(video: &acdc_parser::Video) -> DocumentSymbol {
    let name = if video.title.is_empty() {
        video
            .sources
            .first()
            .map_or_else(|| "Video".to_string(), ToString::to_string)
    } else {
        inlines_to_string(&video.title)
    };
    make_symbol(
        name,
        SymbolKind::FILE,
        &video.location,
        Some("Video".to_string()),
        None,
    )
}

fn delimited_block_to_symbol(block: &DelimitedBlock) -> DocumentSymbol {
    let (default_name, kind, children) = match &block.inner {
        DelimitedBlockType::DelimitedExample(blocks) => {
            ("Example", SymbolKind::STRUCT, children_from_blocks(blocks))
        }
        DelimitedBlockType::DelimitedSidebar(blocks) => {
            ("Sidebar", SymbolKind::STRUCT, children_from_blocks(blocks))
        }
        DelimitedBlockType::DelimitedOpen(blocks) => {
            ("Open block", SymbolKind::STRUCT, children_from_blocks(blocks))
        }
        DelimitedBlockType::DelimitedQuote(blocks) => {
            ("Quote", SymbolKind::STRUCT, children_from_blocks(blocks))
        }
        DelimitedBlockType::DelimitedListing(_) => ("Listing", SymbolKind::STRING, None),
        DelimitedBlockType::DelimitedLiteral(_) => ("Literal", SymbolKind::STRING, None),
        DelimitedBlockType::DelimitedTable(_) => ("Table", SymbolKind::STRUCT, None),
        DelimitedBlockType::DelimitedPass(_) => ("Passthrough", SymbolKind::STRING, None),
        DelimitedBlockType::DelimitedVerse(_) => ("Verse", SymbolKind::STRING, None),
        DelimitedBlockType::DelimitedStem(_) => ("Stem", SymbolKind::STRING, None),
        DelimitedBlockType::DelimitedComment(_)
        // non_exhaustive
        | _ => ("Block", SymbolKind::STRING, None),
    };

    let name = title_or_default(&block.title, default_name);
    make_symbol(
        name,
        kind,
        &block.location,
        Some(default_name.to_string()),
        children,
    )
}

fn section_to_symbol(section: &Section) -> DocumentSymbol {
    let title_text = inlines_to_string(&section.title);

    // Recursively process child blocks
    let children = children_from_blocks(&section.content);

    DocumentSymbol {
        name: title_text,
        kind: section_level_to_symbol_kind(section.level),
        range: location_to_range(&section.location),
        selection_range: location_to_range(&section.location),
        children,
        detail: Some(format!("Level {}", section.level)),
        tags: Some(vec![]),
        deprecated: None,
    }
}

/// Map section levels to appropriate symbol kinds for visual hierarchy
const fn section_level_to_symbol_kind(level: u8) -> SymbolKind {
    match level {
        0 => SymbolKind::NAMESPACE, // Part (rare)
        1 => SymbolKind::MODULE,    // == Section (h2)
        2 => SymbolKind::CLASS,     // === Subsection (h3)
        3 => SymbolKind::METHOD,    // ==== Sub-subsection (h4)
        4 => SymbolKind::FUNCTION,  // ===== (h5)
        _ => SymbolKind::VARIABLE,  // Deeper levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Options;

    #[test]
    fn test_document_symbols_extraction() -> Result<(), acdc_parser::Error> {
        let content = r"= Document Title

== Section One

Some content.

== Section Two

=== Subsection

More content.
";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = document_symbols(&doc);

        // Header + 2 top-level sections
        assert_eq!(symbols.len(), 3);

        assert_eq!(
            symbols.first().map(|s| &s.name),
            Some(&"Document Title".to_string())
        );
        assert_eq!(
            symbols.get(1).map(|s| &s.name),
            Some(&"Section One".to_string())
        );
        assert_eq!(
            symbols.get(2).map(|s| &s.name),
            Some(&"Section Two".to_string())
        );

        // Section One has a paragraph child
        let section_one = symbols.get(1);
        let sec_one_children = section_one.and_then(|s| s.children.as_ref());
        assert!(
            sec_one_children.is_some(),
            "Section One should have children (paragraph)"
        );

        // Section Two has a subsection child (which itself has a paragraph)
        let section_two = symbols.get(2);
        let children = section_two.and_then(|s| s.children.as_ref());
        assert!(children.is_some(), "Section Two should have children");
        let children = children.map(Vec::as_slice);
        assert!(
            children.is_some_and(|c| c.iter().any(|s| s.name == "Subsection")),
            "expected Subsection child"
        );
        Ok(())
    }

    #[test]
    fn test_paragraph_symbols() -> Result<(), acdc_parser::Error> {
        let content = "= Doc\n\n== Section\n\nA simple paragraph.\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = document_symbols(&doc);

        // Find the paragraph inside Section
        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let para =
            children.and_then(|c| c.iter().find(|s| s.detail.as_deref() == Some("Paragraph")));
        assert!(para.is_some(), "expected paragraph symbol");
        assert_eq!(para.map(|p| p.kind), Some(SymbolKind::STRING));
        Ok(())
    }

    #[test]
    fn test_admonition_symbols() -> Result<(), acdc_parser::Error> {
        let content = "= Doc\n\n== Section\n\nNOTE: This is a note.\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = document_symbols(&doc);

        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let adm =
            children.and_then(|c| c.iter().find(|s| s.detail.as_deref() == Some("Admonition")));
        assert!(adm.is_some(), "expected admonition symbol");
        assert!(
            adm.is_some_and(|a| a.name.starts_with("NOTE")),
            "admonition name should start with NOTE"
        );
        Ok(())
    }

    #[test]
    fn test_delimited_block_symbols() -> Result<(), acdc_parser::Error> {
        let content = "= Doc\n\n== Section\n\n.My sidebar\n****\nSidebar content.\n****\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = document_symbols(&doc);

        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let sidebar =
            children.and_then(|c| c.iter().find(|s| s.detail.as_deref() == Some("Sidebar")));
        assert!(sidebar.is_some(), "expected sidebar symbol");
        assert_eq!(
            sidebar.map(|s| s.name.as_str()),
            Some("My sidebar"),
            "sidebar should use block title"
        );
        // Sidebar should have children (the paragraph inside)
        assert!(
            sidebar.is_some_and(|s| s.children.is_some()),
            "sidebar should have nested children"
        );
        Ok(())
    }

    #[test]
    fn test_list_symbols() -> Result<(), acdc_parser::Error> {
        let content = "= Doc\n\n== Section\n\n* Item one\n* Item two\n* Item three\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = document_symbols(&doc);

        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let list = children.and_then(|c| c.iter().find(|s| s.kind == SymbolKind::ARRAY));
        assert!(list.is_some(), "expected list symbol");
        assert!(
            list.is_some_and(|l| l.detail.as_deref().is_some_and(|d| d.contains("3 items"))),
            "list should show item count"
        );
        Ok(())
    }

    #[test]
    fn test_image_symbols() -> Result<(), acdc_parser::Error> {
        let content = "= Doc\n\n== Section\n\nimage::photo.png[]\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = document_symbols(&doc);

        let section = symbols.get(1);
        let children = section.and_then(|s| s.children.as_ref());
        assert!(children.is_some());
        let img = children.and_then(|c| c.iter().find(|s| s.detail.as_deref() == Some("Image")));
        assert!(img.is_some(), "expected image symbol");
        assert!(
            img.is_some_and(|i| i.name.contains("photo.png")),
            "image name should contain source path"
        );
        Ok(())
    }

    #[test]
    fn test_comment_excluded() -> Result<(), acdc_parser::Error> {
        let content = "= Doc\n\n// This is a comment\n\n== Section\n\nContent.\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = document_symbols(&doc);

        // Comments should not appear in symbols
        let has_comment = symbols.iter().any(|s| s.name.contains("comment"));
        assert!(
            !has_comment,
            "comments should not appear in document symbols"
        );
        Ok(())
    }

    #[test]
    fn test_paragraph_preview_truncation() {
        let short = "Hello world";
        assert_eq!(truncate(short, PARAGRAPH_PREVIEW_LEN), "Hello world");

        let long = "a".repeat(60);
        let result = truncate(&long, PARAGRAPH_PREVIEW_LEN);
        assert!(result.ends_with("..."));
        // 50 chars + "..." = 53
        assert_eq!(result.len(), 53);
    }
}
