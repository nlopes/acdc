//! Resolved symbols retain their meaning across converter replacement settings.

use acdc_converters_core::{InlineTextTransform, substitutions::Replacements};
use acdc_parser::{ContentAttribute, InlineNode, Location, Symbol, SymbolKind, ast::ContentRef};

#[test]
fn resolved_text_reads_do_not_depend_on_converter_replacement_settings() {
    let content = ContentAttribute::new(vec![
        InlineNode::Symbol(Symbol::new(SymbolKind::Copyright, Location::default())),
        InlineNode::Symbol(Symbol::new(SymbolKind::Ellipsis, Location::default())),
        InlineNode::Symbol(Symbol::new(
            SymbolKind::CurvedApostrophe,
            Location::default(),
        )),
    ]);
    for transform in [
        InlineTextTransform::default(),
        InlineTextTransform::default().rendered(),
    ] {
        assert_eq!(
            transform.content_to_string(ContentRef::from(&content)),
            "©…’",
        );
    }
}

#[test]
fn symbol_encoding_keeps_backend_specific_spacing() {
    let mut roff = Replacements::unicode();
    roff.em_dash_spaced = " — ";
    roff.em_dash_word_bounded = "—";
    assert_eq!(roff.symbol(SymbolKind::SpacedEmDash), " — ");
    assert_eq!(roff.symbol(SymbolKind::WordBoundedEmDash), "—");
    assert_eq!(roff.symbol(SymbolKind::Copyright), "©");
}
