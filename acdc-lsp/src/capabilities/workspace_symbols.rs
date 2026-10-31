//! Workspace search keeps owned names and source spans after parsing.

use acdc_parser::{
    Location,
    ast::{BlockKind, BlockRef, DocumentRef, LocationRef, SectionRef},
};
use tower_lsp_server::ls_types::SymbolKind;

use super::symbols::{for_each_symbol_child, section_level_to_symbol_kind, title_text};

#[derive(Debug, Clone)]
pub(crate) struct IndexedSymbol {
    pub(crate) name: String,
    pub(crate) kind: SymbolKind,
    pub(crate) location: Location,
    pub(crate) detail: Option<String>,
}

#[must_use]
pub(crate) fn extract_workspace_symbols(doc: DocumentRef<'_>) -> Vec<IndexedSymbol> {
    let mut symbols = Vec::new();
    if let Some(header) = doc.header() {
        let title = title_text(header.title());
        if !title.is_empty() {
            symbols.push(IndexedSymbol {
                name: title,
                kind: SymbolKind::FILE,
                location: header.location().into(),
                detail: Some("Document title".to_string()),
            });
        }
    }
    for block in doc.blocks() {
        extract_block_symbols(block, &mut symbols);
    }
    symbols
}

fn extract_block_symbols(block: BlockRef<'_>, symbols: &mut Vec<IndexedSymbol>) {
    match block.kind() {
        BlockKind::Section(section) => extract_section_symbols(block, section, symbols),
        BlockKind::DiscreteHeader(header) => {
            let title = title_text(header.title());
            if !title.is_empty() {
                symbols.push(IndexedSymbol {
                    name: title,
                    kind: SymbolKind::STRING,
                    location: header.location().into(),
                    detail: Some("Discrete header".to_string()),
                });
            }
            extract_anchors(block, block.location(), symbols);
        }
        BlockKind::DocumentAttribute(attribute) => {
            let assigned = attribute.assignment().value();
            let mut value = String::new();
            if let Some(assigned) = assigned {
                let _ = assigned.write_text(&mut value);
            }
            symbols.push(IndexedSymbol {
                name: attribute.name().to_string(),
                kind: SymbolKind::CONSTANT,
                location: attribute.location().into(),
                detail: Some(if assigned.is_some() {
                    value
                } else {
                    "unset".to_string()
                }),
            });
        }
        BlockKind::Paragraph(_)
        | BlockKind::Image(_)
        | BlockKind::Example(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Open(_)
        | BlockKind::Quote(_)
        | BlockKind::Listing(_)
        | BlockKind::Literal(_)
        | BlockKind::Table(_)
        | BlockKind::Passthrough(_)
        | BlockKind::Verse(_)
        | BlockKind::Stem(_)
        | BlockKind::DelimitedComment(_) => {
            let title = title_text(block.title());
            if !title.is_empty() {
                symbols.push(IndexedSymbol {
                    name: title,
                    kind: SymbolKind::PROPERTY,
                    location: block.location().into(),
                    detail: Some("Block title".to_string()),
                });
            }
            extract_anchors(block, block.location(), symbols);
        }
        BlockKind::Admonition(_) => extract_anchors(block, block.location(), symbols),
        BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::TableOfContents(_)
        | BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::CalloutList(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::Comment(_)
        | _ => {}
    }
    for_each_symbol_child(block, |child| extract_block_symbols(child, symbols));
}

fn extract_section_symbols(
    block: BlockRef<'_>,
    section: SectionRef<'_>,
    symbols: &mut Vec<IndexedSymbol>,
) {
    let heading = super::identity::heading_location(section);
    symbols.push(IndexedSymbol {
        name: title_text(section.title()),
        kind: section_level_to_symbol_kind(section.level()),
        location: heading.clone(),
        detail: Some(format!("Level {}", section.level())),
    });
    extract_anchors(block, (&heading).into(), symbols);
    if let Some(id) = section.id() {
        symbols.push(IndexedSymbol {
            name: id.as_str().to_string(),
            kind: SymbolKind::KEY,
            location: heading,
            detail: Some("Anchor".to_string()),
        });
    }
}

fn extract_anchors(
    block: BlockRef<'_>,
    fallback: LocationRef<'_>,
    symbols: &mut Vec<IndexedSymbol>,
) {
    for anchor in block.anchors() {
        let location: Location = anchor.location().into();
        symbols.push(IndexedSymbol {
            name: anchor.id().to_string(),
            kind: SymbolKind::KEY,
            location: if location == Location::default() {
                fallback.into()
            } else {
                location
            },
            detail: Some("Anchor".to_string()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{Error, Options, parse};

    #[test]
    fn test_extract_symbols_sections() -> Result<(), Error> {
        let content =
            "= Document Title\n\n== Section One\n\nContent.\n\n== Section Two\n\n=== Subsection\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = extract_workspace_symbols(doc);

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
    fn test_extract_symbols_anchors() -> Result<(), Error> {
        let content = "= Doc\n\n[[my-anchor]]\n== Section\n\nContent.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = extract_workspace_symbols(doc);

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "my-anchor" && s.kind == SymbolKind::KEY)
        );
        Ok(())
    }

    #[test]
    fn test_extract_symbols_discrete_header() -> Result<(), Error> {
        let content = "= Doc\n\n[discrete]\n== Discrete Title\n\nContent.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = extract_workspace_symbols(doc);

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Discrete Title"
                    && s.detail.as_deref() == Some("Discrete header"))
        );
        Ok(())
    }

    #[test]
    fn test_extract_symbols_document_attributes() -> Result<(), Error> {
        // Body document attributes produce Block::DocumentAttribute
        let content = "= Doc\n\n== Section\n\n:my-attr: some value\n\nContent.\n";
        let parsed = parse(content, &Options::default())?;
        let doc = parsed.document_ref();
        let symbols = extract_workspace_symbols(doc);

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
