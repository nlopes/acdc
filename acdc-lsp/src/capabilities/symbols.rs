//! Document symbols: extract document outline from AST

use acdc_parser::{Block, Document, Section, inlines_to_string};
use tower_lsp::lsp_types::{DocumentSymbol, SymbolKind};

use crate::convert::location_to_range;

/// Extract document outline as nested symbols
#[must_use]
pub fn document_symbols(doc: &Document) -> Vec<DocumentSymbol> {
    let mut symbols = vec![];

    // Add header as top-level symbol if present
    if let Some(header) = &doc.header {
        // Title implements Deref<Target = [InlineNode]>
        let title_text = inlines_to_string(&header.title);
        #[allow(deprecated)] // deprecated field but required by the type
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

fn block_to_symbol(block: &Block) -> Option<DocumentSymbol> {
    match block {
        Block::Section(section) => Some(section_to_symbol(section)),
        // MVP: only sections in outline (Block is non_exhaustive)
        _ => None,
    }
}

fn section_to_symbol(section: &Section) -> DocumentSymbol {
    // Title implements Deref<Target = [InlineNode]>
    let title_text = inlines_to_string(&section.title);

    // Recursively process child blocks for nested sections
    let children: Vec<DocumentSymbol> =
        section.content.iter().filter_map(block_to_symbol).collect();

    #[allow(deprecated)] // deprecated field but required by the type
    DocumentSymbol {
        name: title_text,
        kind: section_level_to_symbol_kind(section.level),
        range: location_to_range(&section.location),
        selection_range: location_to_range(&section.location),
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
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

        // Check each symbol by index using .get()
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

        // Section Two has a subsection
        let section_two = symbols.get(2);
        assert!(section_two.is_some(), "expected Section Two");
        let children = section_two.and_then(|s| s.children.as_ref());
        assert!(children.is_some(), "Section Two should have children");
        let children = children.map(Vec::as_slice);
        assert_eq!(children.map(<[_]>::len), Some(1));
        assert_eq!(
            children.and_then(|c| c.first()).map(|s| &s.name),
            Some(&"Subsection".to_string())
        );
        Ok(())
    }
}
