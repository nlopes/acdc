//! Workspace symbols: extract navigable symbols from documents for cross-file search

use acdc_parser::{
    Block, DelimitedBlockType, Document, Section, inlines_to_string,
};
use tower_lsp::lsp_types::SymbolKind;

use acdc_parser::Location;

/// A symbol extracted from a document for workspace-wide search
#[derive(Debug, Clone)]
pub struct IndexedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub detail: Option<String>,
}

/// Extract all navigable symbols from a parsed document
#[must_use]
pub fn extract_workspace_symbols(doc: &Document) -> Vec<IndexedSymbol> {
    let mut symbols = Vec::new();

    // Document title
    if let Some(header) = &doc.header {
        let title_text = inlines_to_string(&header.title);
        if !title_text.is_empty() {
            symbols.push(IndexedSymbol {
                name: title_text,
                kind: SymbolKind::FILE,
                location: header.location.clone(),
                detail: Some("Document title".to_string()),
            });
        }
    }

    // Walk all blocks
    for block in &doc.blocks {
        extract_block_symbols(block, &mut symbols);
    }

    symbols
}

fn extract_block_symbols(block: &Block, symbols: &mut Vec<IndexedSymbol>) {
    match block {
        Block::Section(section) => extract_section_symbols(section, symbols),
        Block::DiscreteHeader(header) => {
            let title_text = inlines_to_string(&header.title);
            if !title_text.is_empty() {
                symbols.push(IndexedSymbol {
                    name: title_text,
                    kind: SymbolKind::STRING,
                    location: header.location.clone(),
                    detail: Some("Discrete header".to_string()),
                });
            }
            extract_metadata_anchors(&header.metadata, &header.location, symbols);
        }
        Block::DocumentAttribute(attr) => {
            symbols.push(IndexedSymbol {
                name: attr.name.clone(),
                kind: SymbolKind::CONSTANT,
                location: attr.location.clone(),
                detail: Some(attr.value.to_string()),
            });
        }
        Block::Paragraph(para) => {
            let title_text = inlines_to_string(&para.title);
            if !title_text.is_empty() {
                symbols.push(IndexedSymbol {
                    name: title_text,
                    kind: SymbolKind::PROPERTY,
                    location: para.location.clone(),
                    detail: Some("Block title".to_string()),
                });
            }
            extract_metadata_anchors(&para.metadata, &para.location, symbols);
        }
        Block::DelimitedBlock(delimited) => {
            let title_text = inlines_to_string(&delimited.title);
            if !title_text.is_empty() {
                symbols.push(IndexedSymbol {
                    name: title_text,
                    kind: SymbolKind::PROPERTY,
                    location: delimited.location.clone(),
                    detail: Some("Block title".to_string()),
                });
            }
            extract_metadata_anchors(&delimited.metadata, &delimited.location, symbols);
            extract_delimited_symbols(&delimited.inner, symbols);
        }
        Block::Image(img) => {
            let title_text = inlines_to_string(&img.title);
            if !title_text.is_empty() {
                symbols.push(IndexedSymbol {
                    name: title_text,
                    kind: SymbolKind::PROPERTY,
                    location: img.location.clone(),
                    detail: Some("Block title".to_string()),
                });
            }
            extract_metadata_anchors(&img.metadata, &img.location, symbols);
        }
        Block::Admonition(adm) => {
            extract_metadata_anchors(&adm.metadata, &adm.location, symbols);
            for b in &adm.blocks {
                extract_block_symbols(b, symbols);
            }
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                for b in &item.blocks {
                    extract_block_symbols(b, symbols);
                }
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                for b in &item.blocks {
                    extract_block_symbols(b, symbols);
                }
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                for b in &item.description {
                    extract_block_symbols(b, symbols);
                }
            }
        }
        Block::TableOfContents(_)
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

fn extract_section_symbols(section: &Section, symbols: &mut Vec<IndexedSymbol>) {
    let title_text = inlines_to_string(&section.title);
    symbols.push(IndexedSymbol {
        name: title_text,
        kind: section_level_to_symbol_kind(section.level),
        location: section.location.clone(),
        detail: Some(format!("Level {}", section.level)),
    });

    // Collect anchors from metadata
    extract_metadata_anchors(&section.metadata, &section.location, symbols);

    // Also add generated section ID as anchor
    let safe_id = Section::generate_id(&section.metadata, &section.title);
    symbols.push(IndexedSymbol {
        name: safe_id.to_string(),
        kind: SymbolKind::KEY,
        location: section.location.clone(),
        detail: Some("Anchor".to_string()),
    });

    // Recurse into section content
    for child in &section.content {
        extract_block_symbols(child, symbols);
    }
}

fn extract_metadata_anchors(
    metadata: &acdc_parser::BlockMetadata,
    fallback_location: &Location,
    symbols: &mut Vec<IndexedSymbol>,
) {
    for anchor in &metadata.anchors {
        let loc = if anchor.location == Location::default() {
            fallback_location
        } else {
            &anchor.location
        };
        symbols.push(IndexedSymbol {
            name: anchor.id.clone(),
            kind: SymbolKind::KEY,
            location: loc.clone(),
            detail: Some("Anchor".to_string()),
        });
    }
    if let Some(id_anchor) = &metadata.id {
        let loc = if id_anchor.location == Location::default() {
            fallback_location
        } else {
            &id_anchor.location
        };
        symbols.push(IndexedSymbol {
            name: id_anchor.id.clone(),
            kind: SymbolKind::KEY,
            location: loc.clone(),
            detail: Some("Anchor".to_string()),
        });
    }
}

fn extract_delimited_symbols(inner: &DelimitedBlockType, symbols: &mut Vec<IndexedSymbol>) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                extract_block_symbols(block, symbols);
            }
        }
        DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

/// Map section levels to symbol kinds (same mapping as `document_symbols`)
const fn section_level_to_symbol_kind(level: u8) -> SymbolKind {
    match level {
        0 => SymbolKind::NAMESPACE,
        1 => SymbolKind::MODULE,
        2 => SymbolKind::CLASS,
        3 => SymbolKind::METHOD,
        4 => SymbolKind::FUNCTION,
        _ => SymbolKind::VARIABLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Options;

    #[test]
    fn test_extract_symbols_sections() -> Result<(), acdc_parser::Error> {
        let content = "= Document Title\n\n== Section One\n\nContent.\n\n== Section Two\n\n=== Subsection\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = extract_workspace_symbols(&doc);

        // Document title + 3 sections + generated anchors
        let section_symbols: Vec<_> = symbols
            .iter()
            .filter(|s| s.detail.as_deref() != Some("Anchor"))
            .collect();
        assert!(section_symbols.iter().any(|s| s.name == "Document Title"));
        assert!(section_symbols.iter().any(|s| s.name == "Section One"));
        assert!(section_symbols.iter().any(|s| s.name == "Section Two"));
        assert!(section_symbols.iter().any(|s| s.name == "Subsection"));
        Ok(())
    }

    #[test]
    fn test_extract_symbols_anchors() -> Result<(), acdc_parser::Error> {
        let content = "= Doc\n\n[[my-anchor]]\n== Section\n\nContent.\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = extract_workspace_symbols(&doc);

        assert!(symbols
            .iter()
            .any(|s| s.name == "my-anchor" && s.kind == SymbolKind::KEY));
        Ok(())
    }

    #[test]
    fn test_extract_symbols_discrete_header() -> Result<(), acdc_parser::Error> {
        let content = "= Doc\n\n[discrete]\n== Discrete Title\n\nContent.\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = extract_workspace_symbols(&doc);

        assert!(symbols
            .iter()
            .any(|s| s.name == "Discrete Title" && s.detail.as_deref() == Some("Discrete header")));
        Ok(())
    }

    #[test]
    fn test_extract_symbols_document_attributes() -> Result<(), acdc_parser::Error> {
        // Body document attributes produce Block::DocumentAttribute
        let content = "= Doc\n\n== Section\n\n:my-attr: some value\n\nContent.\n";
        let doc = acdc_parser::parse(content, &Options::default())?;
        let symbols = extract_workspace_symbols(&doc);

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "my-attr" && s.kind == SymbolKind::CONSTANT),
            "symbols: {:#?}",
            symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        Ok(())
    }
}
