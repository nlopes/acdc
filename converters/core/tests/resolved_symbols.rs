use acdc_converters_core::{InlineTextTransform, substitutions::Replacements};
use acdc_parser::{ContentAttribute, InlineNode, Location, Symbol, SymbolKind, ast::ContentRef};

#[test]
fn resolved_text_reads_do_not_depend_on_converter_replacement_settings() {
    let content = ContentAttribute::new(vec![
        InlineNode::Symbol(Symbol {
            kind: SymbolKind::Copyright,
            location: Location::default(),
        }),
        InlineNode::Symbol(Symbol {
            kind: SymbolKind::Ellipsis,
            location: Location::default(),
        }),
        InlineNode::Symbol(Symbol {
            kind: SymbolKind::CurvedApostrophe,
            location: Location::default(),
        }),
    ]);
    for enabled in [false, true] {
        assert_eq!(
            InlineTextTransform::default()
                .rendered_replacements(enabled)
                .content_to_string(ContentRef::from(&content)),
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
