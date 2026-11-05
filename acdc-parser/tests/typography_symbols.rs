use acdc_parser::{
    ContentAttribute, InlineNode, Location, Options, Symbol, SymbolKind,
    ast::{ContentRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn symbol_reads_and_native_json_project_one_semantic_value() -> Result<(), Error> {
    let parsed = parse("First\n\n(C)", &Options::default())?;
    let location = Location::from(
        parsed
            .document()
            .blocks()
            .last()
            .ok_or("missing source block")?
            .location(),
    );
    let content = ContentAttribute::new(vec![InlineNode::Symbol(Symbol {
        kind: SymbolKind::Copyright,
        location: location.clone(),
    })]);
    let content_ref = ContentRef::from(&content);
    let node = content_ref.inlines().next().ok_or("missing symbol")?;
    let InlineKind::Symbol(symbol) = node.kind() else {
        return Err("wrong semantic kind".into());
    };
    assert_eq!(symbol.kind(), SymbolKind::Copyright);
    assert_eq!(symbol.text(), "©");
    assert_eq!(symbol.location().absolute_start(), 7);
    assert_eq!(symbol.location().absolute_end(), 9);
    assert_eq!(node.location().absolute_start(), 7);
    let json = serde_json::to_value(&content)?;
    assert_eq!(
        json,
        serde_json::json!([{
            "name": "symbol",
            "type": "string",
            "value": "©",
            "location": serde_json::to_value(&location)?,
        }])
    );
    Ok(())
}

#[test]
fn symbol_reads_keep_the_shared_handle_traits() {
    fn assert_handle<T: Copy + Send + Sync>() {}
    assert_handle::<acdc_parser::ast::SymbolRef<'_>>();
    assert_handle::<SymbolKind>();
}
