//! Document symbols: extract full block tree from AST

use acdc_converters_core::inlines_to_string;
use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType, Document, Section, Title};
use tower_lsp_server::ls_types::{DocumentSymbol, Range, SymbolKind};

use crate::convert::{is_primary_location, location_to_utf16_range};

/// Maximum length for paragraph preview text in symbol names
const PARAGRAPH_PREVIEW_LEN: usize = 50;

/// Extract document outline as nested symbols
#[must_use]
pub(crate) fn document_symbols(doc: &Document, source: &str) -> Vec<DocumentSymbol> {
    let mut symbols = vec![];

    // Add header as top-level symbol if present
    if let Some(header) = &doc.header
        && is_primary_location(&header.location)
        && let Some(range) = location_to_utf16_range(source, &header.location)
    {
        let title_text = inlines_to_string(&header.title);

        symbols.push(DocumentSymbol {
            name: title_text,
            kind: SymbolKind::FILE,
            range,
            selection_range: range,
            children: None,
            detail: Some("Document title".to_string()),
            tags: Some(vec![]),
            deprecated: None,
        });
    }

    // Process blocks recursively
    for block in &doc.blocks {
        collect_block_symbols(block, source, &mut symbols);
    }

    symbols
}

/// Convert children blocks to nested symbols, returning `None` if empty.
fn children_from_blocks(blocks: &[Block], source: &str) -> Option<Vec<DocumentSymbol>> {
    let mut children = Vec::new();
    for block in blocks {
        collect_block_symbols(block, source, &mut children);
    }
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
    range: Range,
    detail: Option<String>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        kind,
        range,
        selection_range: range,
        detail,
        children: None,
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
fn title_or_default(title: &Title, default: &str) -> String {
    let text = inlines_to_string(title);
    if text.is_empty() {
        default.to_string()
    } else {
        text
    }
}

fn document_attribute_name(attribute: &acdc_parser::DocumentAttribute<'_>) -> String {
    let mut value = String::new();
    if let Some(assigned) = attribute.assignment().value() {
        let _ = assigned.write_text(&mut value);
        format!(":{}: {value}", attribute.name)
    } else {
        format!(":{}!:", attribute.name)
    }
}

/// Get a display name from a title, falling back to a source string.
fn title_or_source(title: &Title, source: &str) -> String {
    if title.is_empty() {
        source.to_string()
    } else {
        inlines_to_string(title)
    }
}

fn collect_block_symbols(block: &Block, source: &str, symbols: &mut Vec<DocumentSymbol>) {
    let children = block_children(block, source);
    let location = if is_primary_location(block.location()) {
        Some(block.location().clone())
    } else if let Block::Section(section) = block {
        // A section can start here and end in an include. Keep its local heading.
        let heading = super::definition::heading_line_location(section);
        is_primary_location(&heading).then_some(heading)
    } else {
        None
    };
    if let Some(range) = location.and_then(|location| location_to_utf16_range(source, &location))
        && let Some(mut symbol) = block_to_symbol(block, range)
    {
        for child in children.iter().flatten() {
            symbol.range.start = symbol.range.start.min(child.range.start);
            symbol.range.end = symbol.range.end.max(child.range.end);
        }
        symbol.children = children;
        symbols.push(symbol);
    } else {
        // Included containers can hold text that resumes in the primary file.
        symbols.extend(children.into_iter().flatten());
    }
}

fn block_children(block: &Block, source: &str) -> Option<Vec<DocumentSymbol>> {
    let mut children = Vec::new();
    match block {
        Block::Section(section) => return children_from_blocks(&section.content, source),
        Block::Admonition(adm) => return children_from_blocks(&adm.blocks, source),
        Block::DelimitedBlock(block) => match &block.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks) => {
                return children_from_blocks(blocks, source);
            }
            DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedListing(_)
            | DelimitedBlockType::DelimitedLiteral(_)
            | DelimitedBlockType::DelimitedTable(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedStem(_)
            // non_exhaustive
            | _ => {}
        },
        Block::UnorderedList(list) => {
            for item in &list.items {
                for block in &item.blocks {
                    collect_block_symbols(block, source, &mut children);
                }
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                for block in &item.blocks {
                    collect_block_symbols(block, source, &mut children);
                }
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                for block in &item.description {
                    collect_block_symbols(block, source, &mut children);
                }
            }
        }
        Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::CalloutList(_)
        | Block::Paragraph(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        // non_exhaustive
        | _ => {}
    }
    (!children.is_empty()).then_some(children)
}

fn block_to_symbol(block: &Block, range: Range) -> Option<DocumentSymbol> {
    match block {
        Block::Section(section) => Some(section_to_symbol(section, range)),
        Block::Paragraph(para) => Some(paragraph_to_symbol(para, range)),
        Block::Admonition(adm) => Some(admonition_to_symbol(adm, range)),
        Block::DelimitedBlock(delimited) => Some(delimited_block_to_symbol(delimited, range)),
        Block::UnorderedList(list) => Some(list_to_symbol(
            &list.title,
            "Unordered list",
            list.items.len(),
            range,
        )),
        Block::OrderedList(list) => Some(list_to_symbol(
            &list.title,
            "Ordered list",
            list.items.len(),
            range,
        )),
        Block::DescriptionList(list) => Some(list_to_symbol(
            &list.title,
            "Description list",
            list.items.len(),
            range,
        )),
        Block::CalloutList(list) => Some(make_symbol(
            title_or_default(&list.title, "Callout list"),
            SymbolKind::ARRAY,
            range,
            Some(format!("{} items", list.items.len())),
        )),
        Block::Image(img) => Some(make_symbol(
            title_or_source(&img.title, &img.source.to_string()),
            SymbolKind::FILE,
            range,
            Some("Image".to_string()),
        )),
        Block::Audio(audio) => Some(make_symbol(
            title_or_source(&audio.title, &audio.source.to_string()),
            SymbolKind::FILE,
            range,
            Some("Audio".to_string()),
        )),
        Block::Video(video) => Some(video_to_symbol(video, range)),
        Block::DiscreteHeader(header) => Some(make_symbol(
            inlines_to_string(&header.title),
            SymbolKind::KEY,
            range,
            Some(format!("Discrete heading (level {})", header.level)),
        )),
        Block::DocumentAttribute(attr) => Some(make_symbol(
            document_attribute_name(attr),
            SymbolKind::PROPERTY,
            range,
            Some("Attribute".to_string()),
        )),
        Block::TableOfContents(_) => Some(make_symbol(
            "Table of contents".to_string(),
            SymbolKind::STRUCT,
            range,
            None,
        )),
        Block::ThematicBreak(_) => Some(make_symbol(
            "---".to_string(),
            SymbolKind::NULL,
            range,
            Some("Thematic break".to_string()),
        )),
        Block::PageBreak(_) => Some(make_symbol(
            "<<<".to_string(),
            SymbolKind::NULL,
            range,
            Some("Page break".to_string()),
        )),
        Block::Comment(_)
        // non_exhaustive
        | _ => None,
    }
}

fn paragraph_to_symbol(para: &acdc_parser::Paragraph, range: Range) -> DocumentSymbol {
    let name = if para.title.is_empty() {
        truncate(&inlines_to_string(&para.content), PARAGRAPH_PREVIEW_LEN)
    } else {
        inlines_to_string(&para.title)
    };
    make_symbol(
        name,
        SymbolKind::STRING,
        range,
        Some("Paragraph".to_string()),
    )
}

fn admonition_to_symbol(adm: &acdc_parser::Admonition, range: Range) -> DocumentSymbol {
    let label = format!("{}", adm.variant).to_uppercase();
    let name = if adm.title.is_empty() {
        label
    } else {
        format!("{label}: {}", inlines_to_string(&adm.title))
    };
    make_symbol(
        name,
        SymbolKind::EVENT,
        range,
        Some("Admonition".to_string()),
    )
}

fn list_to_symbol(
    title: &Title,
    default_name: &str,
    item_count: usize,
    range: Range,
) -> DocumentSymbol {
    make_symbol(
        title_or_default(title, default_name),
        SymbolKind::ARRAY,
        range,
        Some(format!("{item_count} items")),
    )
}

fn video_to_symbol(video: &acdc_parser::Video, range: Range) -> DocumentSymbol {
    let name = if video.title.is_empty() {
        video.source.to_string()
    } else {
        inlines_to_string(&video.title)
    };
    make_symbol(name, SymbolKind::FILE, range, Some("Video".to_string()))
}

fn delimited_block_to_symbol(block: &DelimitedBlock, range: Range) -> DocumentSymbol {
    let (default_name, kind) = match &block.inner {
        DelimitedBlockType::DelimitedExample(_) => ("Example", SymbolKind::STRUCT),
        DelimitedBlockType::DelimitedSidebar(_) => ("Sidebar", SymbolKind::STRUCT),
        DelimitedBlockType::DelimitedOpen(_) => ("Open block", SymbolKind::STRUCT),
        DelimitedBlockType::DelimitedQuote(_) => ("Quote", SymbolKind::STRUCT),
        DelimitedBlockType::DelimitedListing(_) => ("Listing", SymbolKind::STRING),
        DelimitedBlockType::DelimitedLiteral(_) => ("Literal", SymbolKind::STRING),
        DelimitedBlockType::DelimitedTable(_) => ("Table", SymbolKind::STRUCT),
        DelimitedBlockType::DelimitedPass(_) => ("Passthrough", SymbolKind::STRING),
        DelimitedBlockType::DelimitedVerse(_) => ("Verse", SymbolKind::STRING),
        DelimitedBlockType::DelimitedStem(_) => ("Stem", SymbolKind::STRING),
        DelimitedBlockType::DelimitedComment(_)
        // non_exhaustive
        | _ => ("Block", SymbolKind::STRING),
    };

    let name = title_or_default(&block.title, default_name);
    make_symbol(name, kind, range, Some(default_name.to_string()))
}

fn section_to_symbol(section: &Section, range: Range) -> DocumentSymbol {
    let title_text = inlines_to_string(&section.title);

    DocumentSymbol {
        name: title_text,
        kind: section_level_to_symbol_kind(section.level),
        range,
        selection_range: range,
        children: None,
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
    use acdc_parser::{Error, Options, parse};

    #[test]
    fn test_document_symbols_extraction() -> Result<(), Error> {
        let content = r"= Document Title

== Section One

Some content.

== Section Two

=== Subsection

More content.
";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document();
        let symbols = document_symbols(doc, content);

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
    fn test_paragraph_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\nA simple paragraph.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document();
        let symbols = document_symbols(doc, content);

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
    fn test_admonition_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\nNOTE: This is a note.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document();
        let symbols = document_symbols(doc, content);

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
    fn test_delimited_block_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\n.My sidebar\n****\nSidebar content.\n****\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document();
        let symbols = document_symbols(doc, content);

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
    fn test_list_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\n* Item one\n* Item two\n* Item three\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document();
        let symbols = document_symbols(doc, content);

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
    fn test_image_symbols() -> Result<(), Error> {
        let content = "= Doc\n\n== Section\n\nimage::photo.png[]\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document();
        let symbols = document_symbols(doc, content);

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
    fn test_comment_excluded() -> Result<(), Error> {
        let content = "= Doc\n\n// This is a comment\n\n== Section\n\nContent.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document();
        let symbols = document_symbols(doc, content);

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
