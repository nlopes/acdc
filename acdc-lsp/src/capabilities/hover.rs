//! Hover: show information about elements at cursor position

use std::ops::ControlFlow;

use acdc_converters_core::inlines_to_string;
use acdc_parser::{
    Block, Document, Location,
    ast::{DocumentRef, HyperlinkRef, IdOrigin, SectionRef},
};
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Uri};

use super::link_occurrences::{LinkOccurrence, visit_links};

use crate::{
    convert::{location_to_utf16_range, position_in_location, position_to_scalar_position},
    state::{DocumentState, Workspace, XrefTarget},
};

/// Compute hover information for a position
#[must_use]
pub(crate) fn compute_hover(
    doc: &DocumentState,
    doc_uri: &Uri,
    workspace: &Workspace,
    position: Position,
) -> Option<Hover> {
    let position = position_to_scalar_position(doc.text(), position)?;
    let ast_guard = doc.ast()?;
    let ast = ast_guard.document();

    // Check for xref at this position
    if let Some((target, xref_loc)) = find_xref_at_position(ast, &position) {
        let parsed = XrefTarget::parse(&target);
        let content = build_xref_hover_content(&target, &parsed, doc, doc_uri, workspace, ast);

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(location_to_utf16_range(doc.text(), &xref_loc)?),
        });
    }

    // Check for anchor at this position
    if let Some(anchor) = find_anchor_at_position(ast, &position) {
        let id = anchor.id;
        let anchor_loc = anchor.location;
        let total_refs = super::references::collect_xref_locations(workspace, id).len();
        let refs_text = match total_refs {
            0 => "No references".to_string(),
            1 => "1 reference".to_string(),
            n => format!("{n} references"),
        };

        let content = format!("**Anchor**\n\nID: `{id}`\n\n{refs_text}");

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(location_to_utf16_range(doc.text(), &anchor_loc)?),
        });
    }

    // Check for attribute reference at this position
    if let Some((attr_name, attr_ref_loc)) = find_attribute_ref_at_position(doc, &position) {
        let mut value = String::new();
        let content = if let Some(attribute) = ast.attributes.get(&attr_name) {
            attribute.write_text(&mut value).ok()?;
            format!("**Attribute**\n\n`:{attr_name}:` = `{value}`")
        } else {
            format!("**Attribute** (undefined)\n\n`{{{attr_name}}}`")
        };

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(location_to_utf16_range(doc.text(), &attr_ref_loc)?),
        });
    }

    // Check for URL/link at this position
    if let Some((url, link_loc)) = find_link_at_position(ast.into(), &position) {
        let content = format!("**Link**\n\nURL: {url}");

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(location_to_utf16_range(doc.text(), &link_loc)?),
        });
    }

    None
}

/// Build hover content for an xref, with cross-file awareness
fn build_xref_hover_content(
    raw_target: &str,
    parsed: &XrefTarget,
    doc: &DocumentState,
    doc_uri: &Uri,
    workspace: &Workspace,
    ast: &Document,
) -> String {
    if let Some(file_path) = &parsed.file {
        // Cross-file xref
        let file_info = format!("File: `{file_path}`");
        if let Some(anchor_id) = &parsed.anchor {
            if let Some(target_uri) = crate::convert::resolve_relative_uri(doc_uri, file_path) {
                // find_anchor_in_document checks open docs then falls back to disk
                if workspace
                    .find_anchor_in_document(&target_uri, anchor_id)
                    .is_some()
                {
                    format!("**Cross-reference**\n\n{file_info}\n\nTarget: `{anchor_id}`")
                } else {
                    format!(
                        "**Cross-reference** (anchor not found)\n\n{file_info}\n\nTarget: `{anchor_id}`"
                    )
                }
            } else {
                format!(
                    "**Cross-reference** (cannot resolve file)\n\n{file_info}\n\nTarget: `{anchor_id}`"
                )
            }
        } else {
            format!("**Cross-reference**\n\n{file_info}")
        }
    } else if let Some(anchor_id) = &parsed.anchor {
        // Local xref
        if let Some(anchor_loc) = doc.anchors.get(anchor_id) {
            if let Some(title) = find_section_title_at_location(ast, &anchor_loc.location) {
                format!("**Cross-reference**\n\nTarget: `{anchor_id}`\n\nSection: {title}")
            } else {
                format!("**Cross-reference**\n\nTarget: `{anchor_id}`")
            }
        } else {
            // Try workspace-wide
            let global = workspace.find_anchor_globally(anchor_id);
            if let Some((uri, _)) = global.first() {
                format!(
                    "**Cross-reference**\n\nTarget: `{anchor_id}`\n\nDefined in: `{}`",
                    uri.as_str()
                )
            } else {
                format!("**Cross-reference** (unresolved)\n\nTarget: `{raw_target}`")
            }
        }
    } else {
        format!("**Cross-reference** (unresolved)\n\nTarget: `{raw_target}`")
    }
}

/// Find the innermost retained xref at a position in the editor's input.
pub(crate) fn find_xref_at_position(
    doc: &Document,
    position: &acdc_parser::Position,
) -> Option<(String, Location)> {
    // An explicit ID belongs to this occurrence, not to its surrounding xref's target.
    if find_anchor_at_position(doc, position).is_some_and(|id| id.origin == IdOrigin::Explicit) {
        return None;
    }
    super::definition::xref_occurrences(doc.into())
        .filter(|xref| position_in_location(position, xref.location()))
        .min_by_key(|xref| {
            let location = xref.location();
            location
                .absolute_end()
                .saturating_sub(location.absolute_start())
        })
        .map(|xref| (xref.target().to_owned(), xref.location().into()))
}

/// Find the smallest retained ID occurrence under the primary-file cursor.
pub(crate) fn find_anchor_at_position<'doc>(
    doc: &'doc Document<'doc>,
    position: &acdc_parser::Position,
) -> Option<super::identity::IdOccurrence<'doc>> {
    super::identity::occurrences(doc.into())
        .filter(|id| position_in_location(position, &id.location))
        .min_by_key(|id| {
            id.location
                .absolute_end
                .saturating_sub(id.location.absolute_start)
        })
}

/// Find a link or URL at an original-source scalar position.
pub(crate) fn find_link_at_position(
    doc: DocumentRef<'_>,
    position: &acdc_parser::Position,
) -> Option<(String, Location)> {
    visit_links(doc, |occurrence| {
        if let LinkOccurrence::Hyperlink(link) = occurrence {
            match link {
                HyperlinkRef::Link(_) | HyperlinkRef::Url(_) | HyperlinkRef::Autolink(_) => {
                    if position_in_location(position, link.location()) {
                        return ControlFlow::Break((
                            link.target().to_string(),
                            link.location().into(),
                        ));
                    }
                }
                HyperlinkRef::Mailto(_) | _ => {}
            }
        }
        ControlFlow::Continue(())
    })
    .break_value()
}

/// Find a raw attribute reference at an original-source scalar position.
fn find_attribute_ref_at_position(
    doc: &DocumentState,
    position: &acdc_parser::Position,
) -> Option<(String, Location)> {
    doc.attribute_refs.iter().find_map(|(name, loc)| {
        if position_in_location(position, loc) {
            Some((name.clone(), loc.clone()))
        } else {
            None
        }
    })
}

/// Find section title at a given location
fn find_section_title_at_location(doc: &Document, loc: &Location) -> Option<String> {
    for block in &doc.blocks {
        if let Some(title) = find_section_title_in_block(block, loc) {
            return Some(title);
        }
    }
    None
}

fn find_section_title_in_block(block: &Block, target_loc: &Location) -> Option<String> {
    match block {
        Block::Section(section) => {
            if section.location.start == target_loc.start
                || SectionRef::from(section).anchors()
                    .any(|anchor| Location::from(anchor.location()) == *target_loc)
            {
                return Some(inlines_to_string(&section.title));
            }
            // Recurse into children
            for child in &section.content {
                if let Some(title) = find_section_title_in_block(child, target_loc) {
                    return Some(title);
                }
            }
            None
        }
        Block::TableOfContents(_)
        | Block::Admonition(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::UnorderedList(_)
        | Block::OrderedList(_)
        | Block::CalloutList(_)
        | Block::DescriptionList(_)
        | Block::DelimitedBlock(_)
        | Block::Paragraph(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        // non_exhaustive
        | _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_hover_on_xref() -> Result<(), Box<dyn Error>> {
        let content = r"[[my-section]]
== My Section

See <<my-section>> for details.
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on the xref (line 3, somewhere in <<my-section>>)
        let position = Position {
            line: 3,
            character: 8,
        };

        let result = compute_hover(&doc, &uri, &workspace, position);
        assert!(result.is_some(), "Expected hover result for xref position");
        let hover = result;
        assert!(
            matches!(&hover, Some(h) if matches!(&h.contents, HoverContents::Markup(_))),
            "Expected HoverContents::Markup"
        );
        if let Some(hover) = hover
            && let HoverContents::Markup(markup) = hover.contents
        {
            assert!(markup.value.contains("Cross-reference"));
            assert!(markup.value.contains("my-section"));
        }
        Ok(())
    }

    #[test]
    fn xref_macro_hover_keeps_the_cross_file_target() -> Result<(), Box<dyn Error>> {
        let content = "See xref:Other.adoc[].\n\n== Other.adoc\n";
        let workspace = Workspace::new();
        let uri = "file:///project/source.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hover = compute_hover(
            &doc,
            &uri,
            &workspace,
            Position {
                line: 0,
                character: 10,
            },
        )
        .ok_or("expected hover for xref macro")?;
        let HoverContents::Markup(markup) = hover.contents else {
            return Err("expected markup hover".into());
        };

        assert!(markup.value.contains("File: `Other.adoc`"), "{markup:?}");
        assert!(!markup.value.contains("Section: Other.adoc"), "{markup:?}");
        Ok(())
    }

    #[test]
    fn compat_mode_natural_xref_hover_is_unresolved() -> Result<(), Box<dyn Error>> {
        let content = "= Document\n:compat-mode:\n\nNatural: <<Syntax Highlighting>>.\nExplicit: <<_syntax_highlighting>>.\n\n== Syntax Highlighting\n";
        let workspace = Workspace::new();
        let uri = "file:///project/source.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let natural = compute_hover(
            &doc,
            &uri,
            &workspace,
            Position {
                line: 3,
                character: 12,
            },
        )
        .ok_or("expected natural-reference hover")?;
        let HoverContents::Markup(natural) = natural.contents else {
            return Err("expected markup hover".into());
        };
        assert!(
            natural.value.contains("**Cross-reference** (unresolved)")
                && natural.value.contains("Target: `Syntax Highlighting`"),
            "{natural:?}"
        );

        let explicit = compute_hover(
            &doc,
            &uri,
            &workspace,
            Position {
                line: 4,
                character: 12,
            },
        )
        .ok_or("expected explicit-ID hover")?;
        let HoverContents::Markup(explicit) = explicit.contents else {
            return Err("expected markup hover".into());
        };
        assert!(
            explicit.value.contains("Target: `_syntax_highlighting`")
                && explicit.value.contains("Section: Syntax Highlighting"),
            "{explicit:?}"
        );
        Ok(())
    }

    #[test]
    fn named_section_reftext_controls_xref_hover() -> Result<(), Box<dyn Error>> {
        let content = "Named: <<Custom Label>>.\nTitle: <<Actual Title>>.\nExplicit: <<id>>.\n\n[#id,reftext=\"Custom Label\"]\n== Actual Title\n";
        let workspace = Workspace::new();
        let uri = "file:///project/source.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let named = compute_hover(
            &doc,
            &uri,
            &workspace,
            Position {
                line: 0,
                character: 12,
            },
        )
        .ok_or("expected named-reftext hover")?;
        let HoverContents::Markup(named) = named.contents else {
            return Err("expected markup hover".into());
        };
        assert!(
            named.value.contains("Target: `id`") && named.value.contains("Section: Actual Title"),
            "{named:?}"
        );

        let title = compute_hover(
            &doc,
            &uri,
            &workspace,
            Position {
                line: 1,
                character: 12,
            },
        )
        .ok_or("expected rejected-title hover")?;
        let HoverContents::Markup(title) = title.contents else {
            return Err("expected markup hover".into());
        };
        assert!(
            title.value.contains("**Cross-reference** (unresolved)")
                && title.value.contains("Target: `Actual Title`"),
            "{title:?}"
        );
        Ok(())
    }

    #[test]
    fn passthroughs_are_restored_before_natural_xref_hover() -> Result<(), Box<dyn Error>> {
        let content = "Resolved: <<Pass raw Title>>.\nMissing: <<Missing pass:[raw] Title>>.\n\n== Pass pass:[raw] Title\n";
        let workspace = Workspace::new();
        let uri = "file:///project/source.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let resolved = compute_hover(
            &doc,
            &uri,
            &workspace,
            Position {
                line: 0,
                character: 14,
            },
        )
        .ok_or("expected resolved hover")?;
        let HoverContents::Markup(resolved) = resolved.contents else {
            return Err("expected markup hover".into());
        };
        assert!(
            resolved.value.contains("Target: `_pass_raw_title`")
                && resolved.value.contains("Section: Pass raw Title"),
            "{resolved:?}"
        );

        let missing = compute_hover(
            &doc,
            &uri,
            &workspace,
            Position {
                line: 1,
                character: 14,
            },
        )
        .ok_or("expected unresolved hover")?;
        let HoverContents::Markup(missing) = missing.contents else {
            return Err("expected markup hover".into());
        };
        assert!(
            missing.value.contains("**Cross-reference** (unresolved)")
                && missing.value.contains("Target: `Missing raw Title`"),
            "{missing:?}"
        );
        assert!(!missing.value.contains('\u{fffd}'), "{missing:?}");
        Ok(())
    }

    #[test]
    fn test_hover_on_attribute_ref() -> Result<(), Box<dyn Error>> {
        let content = ":imagesdir: ./images\n\n== Section\n\nImage in {imagesdir}/logo.png\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on {imagesdir} — line 4 (0-indexed), character 10
        let position = Position {
            line: 4,
            character: 10,
        };

        let result = compute_hover(&doc, &uri, &workspace, position);
        assert!(result.is_some(), "Expected hover result for attribute ref");

        if let Some(hover) = result
            && let HoverContents::Markup(markup) = hover.contents
        {
            assert!(
                markup.value.contains("Attribute"),
                "Expected 'Attribute' in hover, got: {}",
                markup.value
            );
            assert!(
                markup.value.contains("imagesdir"),
                "Expected attribute name in hover, got: {}",
                markup.value
            );
            assert!(
                markup.value.contains("./images"),
                "Expected attribute value in hover, got: {}",
                markup.value
            );
        }
        Ok(())
    }

    #[test]
    fn test_hover_on_undefined_attribute_ref() -> Result<(), Box<dyn Error>> {
        let content = "== Section\n\nSee {undefined-attr} here.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        // Position on {undefined-attr} — line 2, character 6
        let position = Position {
            line: 2,
            character: 6,
        };

        let result = compute_hover(&doc, &uri, &workspace, position);
        assert!(
            result.is_some(),
            "Expected hover result for undefined attribute ref"
        );

        if let Some(hover) = result
            && let HoverContents::Markup(markup) = hover.contents
        {
            assert!(
                markup.value.contains("undefined"),
                "Expected 'undefined' in hover, got: {}",
                markup.value
            );
        }
        Ok(())
    }

    #[test]
    fn test_hover_formats_typed_default() -> Result<(), Box<dyn Error>> {
        let content = "depth={max-include-depth}\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let hover = compute_hover(
            &doc,
            &uri,
            &workspace,
            Position {
                line: 0,
                character: 8,
            },
        )
        .ok_or("expected typed attribute hover")?;
        let HoverContents::Markup(markup) = hover.contents else {
            return Err("expected markup hover".into());
        };
        assert!(markup.value.contains("`64`"), "{markup:?}");
        Ok(())
    }
}
