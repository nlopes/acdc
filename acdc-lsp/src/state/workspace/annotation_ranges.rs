use std::error::Error;

use tower_lsp_server::ls_types::{InlayHintLabel, Position, Range};

use super::{
    Workspace,
    source_locations::{file_uri, options_with_sources},
};
use crate::{
    capabilities::{code_lens, document_links, hover, inlay_hints},
    convert::location_to_utf16_range,
    state::document::{extract_attribute_defs, extract_attribute_refs},
};

#[test]
fn raw_attribute_locations_preserve_crlf_unicode_and_inclusive_endpoints()
-> Result<(), Box<dyn Error>> {
    let text = ":名: 😀\r\n\r\n😀 {名}!\r\n";
    let definitions = extract_attribute_defs(text);
    let (_, definition) = definitions.first().ok_or("attribute definition")?;
    assert_eq!(definition.end.column, 5);
    assert_eq!(
        text.get(definition.absolute_start..=definition.absolute_end),
        Some(":名: 😀")
    );
    let references = extract_attribute_refs(text);
    let (_, reference) = references.first().ok_or("attribute reference")?;
    assert_eq!(
        text.get(reference.absolute_start..=reference.absolute_end),
        Some("{名}")
    );
    assert_eq!(reference.start.column, 3);
    assert_eq!(reference.end.column, 5);
    assert_eq!(
        location_to_utf16_range(text, reference),
        Some(Range::new(Position::new(2, 3), Position::new(2, 6)))
    );
    Ok(())
}

#[test]
fn attribute_hover_stops_after_the_closing_brace() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("attribute-hover-boundary.adoc")?;
    workspace.update_document(
        uri.clone(),
        "= Doc\r\n:name: Value\r\n\r\n😀 {name}!".into(),
        1,
    );
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let hovered = hover::compute_hover(&doc, &uri, &workspace, Position::new(3, 8))
        .ok_or("attribute hover")?;
    assert_eq!(
        hovered.range,
        Some(Range::new(Position::new(3, 3), Position::new(3, 9)))
    );
    assert!(hover::compute_hover(&doc, &uri, &workspace, Position::new(3, 9)).is_none());
    Ok(())
}

#[test]
fn annotations_keep_resumed_primary_content_after_selected_reindented_includes()
-> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("annotation-parent.adoc")?;
    let source = "= Book\r\n:name: Value\r\n\r\n== Parent\r\n\r\ninclude::child.adoc[lines=3..5,indent=0]\r\n\r\n😀 [[local]] <<_parent>> {name} link:local.adoc[Local]\r\n";
    let child = "// omitted\r\n// omitted\r\n    === Child\r\n\r\n    😀 [[hidden]] <<_parent>> link:hidden.adoc[Hidden]\r\n";
    workspace.update_document_with_options(
        uri.clone(),
        source.into(),
        1,
        &options_with_sources(&[("child.adoc", child)])?,
    );
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let lenses = code_lens::compute_code_lenses(&doc, &uri, &workspace);
    assert_eq!(lenses.len(), 3);
    assert!(
        lenses
            .iter()
            .any(|lens| lens.range == Range::new(Position::new(7, 3), Position::new(7, 3)))
    );
    let hints = inlay_hints::compute_inlay_hints(
        &doc,
        &Range::new(Position::new(7, 0), Position::new(7, 80)),
    );
    assert_eq!(hints.len(), 2);
    assert!(
        hints
            .iter()
            .any(|hint| hint.position == Position::new(7, 24)
                && matches!(&hint.label, InlayHintLabel::String(label) if label == "→ Parent"))
    );
    assert!(
        hints
            .iter()
            .any(|hint| hint.position == Position::new(7, 31)
                && matches!(&hint.label, InlayHintLabel::String(label) if label == "= Value")),
        "attribute hints: {hints:?}"
    );
    let links = document_links::collect_document_links(&doc, &uri);
    assert_eq!(links.len(), 2);
    let link = links
        .iter()
        .find(|link| {
            link.target
                .as_ref()
                .is_some_and(|target| target.as_str().ends_with("local.adoc"))
        })
        .ok_or("local link")?;
    assert_eq!(
        link.range,
        Range::new(Position::new(7, 32), Position::new(7, 54))
    );
    let hovered =
        hover::compute_hover(&doc, &uri, &workspace, Position::new(7, 18)).ok_or("local hover")?;
    assert_eq!(
        hovered.range,
        Some(Range::new(Position::new(7, 13), Position::new(7, 24)))
    );
    Ok(())
}

#[test]
fn hover_uses_original_positions_in_crlf_input() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("annotation-offset.adoc")?;
    let text = "= Doc\r\n:name: Value\r\n\r\n[[target]]\r\n== Target\r\n\r\n😀 [[inline]] text\r\n\r\n😀 <<target>> {name}\r\n\r\n😀 link:next.adoc[Next]\r\n";
    workspace.update_document(uri.clone(), text.into(), 1);
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let position = Position::new(8, 6);
    let hovered = hover::compute_hover(&doc, &uri, &workspace, position).ok_or_else(|| {
        format!(
            "missing hover: input offset {:?}, xrefs {:?}",
            crate::convert::position_to_offset(text, position),
            doc.xrefs
        )
    })?;
    assert_eq!(
        hovered.range,
        Some(Range::new(Position::new(8, 3), Position::new(8, 13)))
    );
    Ok(())
}

#[test]
fn document_link_include_ranges_use_raw_unicode_crlf_source() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let uri = file_uri("annotation-include.adoc")?;
    workspace.update_document(
        uri.clone(),
        "= Book\r\n\r\ninclude::😀.adoc[]\r\n".into(),
        1,
    );
    let doc = workspace.get_document(&uri).ok_or("document")?;
    let links = document_links::collect_document_links(&doc, &uri);
    let link = links.first().ok_or("include link")?;
    assert_eq!(links.len(), 1);
    assert_eq!(
        link.range,
        Range::new(Position::new(2, 9), Position::new(2, 16))
    );
    assert_eq!(link.target, Some(file_uri("😀.adoc")?));
    Ok(())
}
