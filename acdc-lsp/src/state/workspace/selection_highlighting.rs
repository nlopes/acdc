use std::{collections::HashSet, error::Error};

use tower_lsp_server::ls_types::{Position, Range, SelectionRange, SemanticTokens};

use super::{
    Workspace,
    source_locations::{file_uri, options_with_sources},
};
use crate::capabilities::{folding, selection_range, semantic_tokens};

type TestResult = Result<(), Box<dyn Error>>;

fn selection_chain(selection: &SelectionRange) -> Vec<Range> {
    let mut result = vec![selection.range];
    let mut child = selection;
    while let Some(parent) = &child.parent {
        assert!(parent.range.start <= child.range.start && child.range.end <= parent.range.end);
        result.push(parent.range);
        child = parent;
    }
    result
}

fn decoded_tokens(tokens: SemanticTokens) -> Vec<(u32, u32, u32, u32, u32)> {
    let mut line = 0;
    let mut column = 0;
    tokens
        .data
        .into_iter()
        .map(|token| {
            line += token.delta_line;
            column = if token.delta_line == 0 {
                column + token.delta_start
            } else {
                token.delta_start
            };
            (
                line,
                column,
                token.length,
                token.token_type,
                token.token_modifiers_bitset,
            )
        })
        .collect()
}

#[test]
fn selection_uses_original_utf16_positions_and_includes_final_empty_line() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("selection-crlf.adoc")?;
    let text = "= Doc\r\n\r\n== Heading\r\n\r\n😀 *bold*\r\n";
    workspace.update_document(uri.clone(), text.into(), 1);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let selections = selection_range::compute_selection_ranges(
        &doc,
        &[Position::new(4, 5), Position::new(5, 0)],
    );
    let chain = selection_chain(selections.first().ok_or("selection")?);
    assert_eq!(
        chain.first(),
        Some(&Range::new(Position::new(4, 4), Position::new(4, 8)))
    );
    assert_eq!(
        chain.last(),
        Some(&Range::new(Position::new(0, 0), Position::new(5, 0)))
    );
    let eof = selections.get(1).ok_or("EOF selection")?;
    assert_eq!(eof.range.end, Position::new(5, 0));
    Ok(())
}

#[test]
fn selection_extent_handles_empty_and_unterminated_unicode_input() -> TestResult {
    for (text, position) in [
        ("", Position::new(0, 0)),
        ("😀", Position::new(0, 2)),
        ("😀\r\n", Position::new(1, 0)),
        ("é😀", Position::new(0, 3)),
        ("é😀\n", Position::new(1, 0)),
    ] {
        let workspace = Workspace::new();
        let uri = file_uri("selection-extent.adoc")?;
        workspace.update_document(uri.clone(), text.into(), 1);
        let doc = workspace.get_document(&uri).ok_or("document")?;
        let selections = selection_range::compute_selection_ranges(&doc, &[position]);
        let chain = selection_chain(selections.first().ok_or("selection")?);
        assert_eq!(
            chain.last(),
            Some(&Range::new(Position::new(0, 0), position))
        );
    }
    Ok(())
}

#[test]
fn invalid_selection_cursors_use_zero_width_fallbacks() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("selection-invalid.adoc")?;
    workspace.update_document(uri.clone(), "😀 *bold*\r\n".into(), 1);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let positions = [Position::new(0, 1), Position::new(2, 0)];
    let selections = selection_range::compute_selection_ranges(&doc, &positions);
    for (selection, position) in selections.iter().zip(positions) {
        assert_eq!(selection.range, Range::new(position, position));
        assert!(selection.parent.is_none());
    }
    Ok(())
}

#[test]
fn selected_reindented_includes_preserve_primary_selection_highlights_and_folds() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("selection-parent.adoc")?;
    let text = "= Book\r\n\r\n== Parent\r\n\r\ninclude::child.adoc[lines=3..5,indent=0]\r\n\r\n😀 *local*\r\n";
    let child = "// omitted\r\n// omitted\r\n    === Child\r\n\r\n    😀 *foreign*\r\n";
    workspace.update_document_with_options(
        uri.clone(),
        text.into(),
        1,
        &options_with_sources(&[("child.adoc", child)])?,
    );
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let selections = selection_range::compute_selection_ranges(&doc, &[Position::new(6, 6)]);
    let chain = selection_chain(selections.first().ok_or("selection")?);
    assert_eq!(
        chain.first(),
        Some(&Range::new(Position::new(6, 4), Position::new(6, 9)))
    );
    assert!(
        chain
            .iter()
            .all(|range| range.start.line == 0 || range.start.line == 2 || range.start.line == 6)
    );
    let ast = doc.ast().ok_or("AST")?;
    let tokens = decoded_tokens(semantic_tokens::compute_semantic_tokens(
        ast.document(),
        &doc.conditionals,
        doc.text(),
    ));
    assert!(tokens.contains(&(6, 3, 7, 4, 0)));
    assert!(tokens.iter().all(|(line, ..)| *line == 2 || *line == 6));
    let folds = folding::compute_folding_ranges(ast.document(), doc.text());
    assert!(folds.iter().all(|fold| fold.start_line == 2));
    assert!(
        folds
            .iter()
            .all(|fold| fold.start_character.is_none() && fold.end_character.is_none())
    );
    Ok(())
}

#[test]
fn nested_disabled_highlighting_has_unique_line_bounded_utf16_tokens() -> TestResult {
    let workspace = Workspace::new();
    let uri = file_uri("highlight-disabled.adoc")?;
    let text =
        "ifdef::missing[]\r\nifdef::also-missing[]\r\n😀 disabled\r\nendif::[]\r\nendif::[]\r\n";
    workspace.update_document(uri.clone(), text.into(), 1);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let ast = doc.ast().ok_or("AST")?;
    let tokens = decoded_tokens(semantic_tokens::compute_semantic_tokens(
        ast.document(),
        &doc.conditionals,
        doc.text(),
    ));
    let distinct: HashSet<_> = tokens.iter().copied().collect();
    assert_eq!(tokens.len(), distinct.len(), "duplicate tokens: {tokens:?}");
    assert!(tokens.contains(&(2, 0, 11, 5, 4)));
    for (line, start, length, ..) in tokens {
        let width = text
            .lines()
            .nth(line as usize)
            .ok_or("token line")?
            .encode_utf16()
            .count();
        assert!(length > 0 && (start + length) as usize <= width);
    }
    Ok(())
}
