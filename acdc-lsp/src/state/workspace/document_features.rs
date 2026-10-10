use std::error::Error;

use tower_lsp_server::ls_types::{
    DocumentSymbol, FormattingOptions, HoverContents, Position, Range, Uri,
};

use super::{
    Workspace,
    source_locations::{file_uri, options_with_sources},
};
use crate::capabilities::{
    code_lens, document_links, folding, formatting, hover, inlay_hints, on_type_formatting,
    selection_range, semantic_tokens, symbols,
};

const CHILD: &str = "// omitted\n// omitted\n=== Included heading\n\n:child-attribute: value\n\nxref:_parent[] link:child.adoc[Child] *child*\n";
const BOOK: &str = "= Book\n\n== Parent\n\nBefore *parent*.\n\ninclude::child.adoc[lines=3..7]\n\nAfter *local* and <<_parent>>. link:local.adoc[Local]\n";

fn setup() -> Result<(Workspace, Uri), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("book.adoc")?;
    workspace.update_document_with_options(
        uri.clone(),
        BOOK.into(),
        1,
        &options_with_sources(&[("child.adoc", CHILD)])?,
    );
    Ok((workspace, uri))
}

fn flatten_symbols(symbols: &[DocumentSymbol]) -> Vec<&DocumentSymbol> {
    let mut result = Vec::new();
    for symbol in symbols {
        result.push(symbol);
        if let Some(children) = &symbol.children {
            for child in flatten_symbols(children) {
                assert!(
                    symbol.range.start <= child.range.start && child.range.end <= symbol.range.end
                );
                result.push(child);
            }
        }
    }
    result
}

#[test]
fn outline_keeps_parent_text_nested_under_an_included_section() -> Result<(), Box<dyn Error>> {
    let (workspace, uri) = setup()?;
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let ast = doc.ast().ok_or("missing AST")?;
    let symbols = symbols::document_symbols(ast.document());
    let symbols = flatten_symbols(&symbols);
    assert!(symbols.iter().any(|symbol| symbol.name == "Parent"));
    assert!(
        symbols
            .iter()
            .any(|symbol| symbol.name.starts_with("After local"))
    );
    assert!(
        !symbols
            .iter()
            .any(|symbol| symbol.name.contains("Included heading")
                || symbol.name.contains("child-attribute"))
    );
    Ok(())
}

#[test]
fn outline_keeps_a_local_heading_when_its_section_ends_in_an_include() -> Result<(), Box<dyn Error>>
{
    let workspace = Workspace::new();
    let uri = file_uri("book.adoc")?;
    let text = "== Parent\n\nBefore.\n\ninclude::child.adoc[]\n";
    workspace.update_document_with_options(
        uri.clone(),
        text.into(),
        1,
        &options_with_sources(&[("child.adoc", CHILD)])?,
    );
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let ast = doc.ast().ok_or("missing AST")?;
    let symbols = symbols::document_symbols(ast.document());
    let symbols = flatten_symbols(&symbols);
    let parent = symbols
        .iter()
        .find(|symbol| symbol.name == "Parent")
        .ok_or("missing parent heading")?;
    assert_eq!(parent.range.start.line, 0);
    assert!(parent.range.end.line < 4);
    assert!(
        !symbols
            .iter()
            .any(|symbol| symbol.name == "Included heading")
    );
    Ok(())
}

#[test]
fn highlighting_and_folding_only_use_primary_source_ranges() -> Result<(), Box<dyn Error>> {
    let (workspace, uri) = setup()?;
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let ast = doc.ast().ok_or("missing AST")?;
    let tokens =
        semantic_tokens::compute_semantic_tokens(ast.document(), &doc.conditionals, doc.text());
    let mut line = 0;
    for token in &tokens.data {
        line += token.delta_line;
        assert!(
            [2, 4, 8].contains(&line),
            "token from included text on line {line}: {token:?}"
        );
    }
    assert_ne!(tokens.data, []);
    let folds = folding::compute_folding_ranges(ast.document());
    assert_eq!(folds.len(), 1);
    assert_eq!(folds.first().ok_or("missing parent fold")?.start_line, 2);
    Ok(())
}

#[test]
fn hints_lenses_and_links_stay_in_the_primary_file() -> Result<(), Box<dyn Error>> {
    let (workspace, uri) = setup()?;
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let hints = inlay_hints::compute_inlay_hints(
        &doc,
        &Range::new(Position::new(0, 0), Position::new(100, 0)),
    );
    assert_eq!(hints.len(), 1);
    assert_eq!(hints.first().ok_or("missing local hint")?.position.line, 8);
    let lenses = code_lens::compute_code_lenses(&doc, &uri, &workspace);
    assert_eq!(lenses.len(), 1);
    assert_eq!(
        lenses
            .first()
            .ok_or("missing parent lens")?
            .range
            .start
            .line,
        2
    );
    let links = document_links::collect_document_links(&doc, &uri);
    assert_eq!(links.len(), 2);
    assert!(
        links
            .iter()
            .all(|link| [6, 8].contains(&link.range.start.line))
    );
    Ok(())
}

#[test]
fn selection_reaches_parent_text_nested_under_an_included_section() -> Result<(), Box<dyn Error>> {
    let (workspace, uri) = setup()?;
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let ranges = selection_range::compute_selection_ranges(&doc, &[Position::new(8, 8)]);
    let mut selection = ranges.first().ok_or("missing selection")?;
    assert_eq!(selection.range.start.line, 8);
    while let Some(parent) = &selection.parent {
        assert!(
            parent.range.start <= selection.range.start && selection.range.end <= parent.range.end
        );
        selection = parent;
    }
    Ok(())
}

#[test]
fn workspace_symbols_use_included_file_uris_without_duplicates() -> Result<(), Box<dyn Error>> {
    let (workspace, _) = setup()?;
    let child = file_uri("child.adoc")?;
    workspace.update_document(child.clone(), CHILD.into(), 1);
    let symbols = workspace.query_workspace_symbols("Included heading");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols.first().ok_or("missing included heading")?.0, child);
    assert_eq!(
        symbols
            .first()
            .ok_or("missing included heading")?
            .1
            .location
            .start
            .line,
        3
    );
    Ok(())
}

#[test]
fn reference_lenses_count_each_physical_xref_once() -> Result<(), Box<dyn Error>> {
    let (workspace, uri) = setup()?;
    workspace.update_document(file_uri("child.adoc")?, CHILD.into(), 1);
    workspace.update_document_with_options(
        file_uri("second.adoc")?,
        BOOK.into(),
        1,
        &options_with_sources(&[("child.adoc", CHILD)])?,
    );
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let lenses = code_lens::compute_code_lenses(&doc, &uri, &workspace);
    let parent = lenses
        .iter()
        .find(|lens| lens.range.start.line == 2)
        .ok_or("missing parent lens")?;
    assert_eq!(
        parent
            .command
            .as_ref()
            .ok_or("missing reference command")?
            .title,
        "3 references"
    );
    Ok(())
}

#[test]
fn included_listing_does_not_protect_unrelated_parent_lines() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("book.adoc")?;
    let text = "First.  \n\n* Item\n\ninclude::child.adoc[]\n";
    workspace.update_document_with_options(
        uri.clone(),
        text.into(),
        1,
        &options_with_sources(&[("child.adoc", "----\ncode\n----\n")])?,
    );
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let edits = formatting::format_document(&doc, &FormattingOptions::default());
    assert!(
        edits
            .iter()
            .any(|edit| edit.range.start.line == 0 && edit.new_text.is_empty())
    );
    let edits = on_type_formatting::format_on_type(&doc, Position::new(3, 0), "\n")
        .ok_or("missing list continuation")?;
    assert_eq!(edits.first().ok_or("missing edit")?.new_text, "* ");
    Ok(())
}

#[test]
fn included_block_boundaries_do_not_insert_blank_lines_in_the_parent() -> Result<(), Box<dyn Error>>
{
    let workspace = Workspace::new();
    let uri = file_uri("book.adoc")?;
    let text = "include::child.adoc[]\n\nParent text.\n";
    workspace.update_document_with_options(
        uri.clone(),
        text.into(),
        1,
        &options_with_sources(&[("child.adoc", "== First\n== Second\n")])?,
    );
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let edits = formatting::format_document(&doc, &FormattingOptions::default());
    assert_eq!(edits, []);
    Ok(())
}

#[test]
fn hover_distinguishes_section_offsets_in_different_files() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("book.adoc")?;
    let text = "== Parent\n\nSee <<_child>>.\n\ninclude::child.adoc[]\n";
    workspace.update_document_with_options(
        uri.clone(),
        text.into(),
        1,
        &options_with_sources(&[("child.adoc", "== Child\n")])?,
    );
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    let hover =
        hover::compute_hover(&doc, &uri, &workspace, Position::new(2, 8)).ok_or("missing hover")?;
    let HoverContents::Markup(content) = hover.contents else {
        return Err("expected markup hover".into());
    };
    assert!(
        content.value.contains("Section: Child"),
        "{}",
        content.value
    );
    Ok(())
}

#[test]
fn formatting_preserves_raw_listing_text_when_an_include_closes_the_parsed_block()
-> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("book.adoc")?;
    let text = "----\nkeep spaces  \ninclude::child.adoc[]\nkeep these too  \n----\n";
    workspace.update_document_with_options(
        uri.clone(),
        text.into(),
        1,
        &options_with_sources(&[("child.adoc", "----\n")])?,
    );
    let doc = workspace.get_document(&uri).ok_or("missing book")?;
    assert_eq!(
        formatting::format_document(&doc, &FormattingOptions::default()),
        []
    );
    Ok(())
}
