use std::ops::ControlFlow;

use acdc_parser::ast::{
    BlockKind, ContentRef, DocumentRef, HyperlinkRef, ImageRef, InlineKind, StructuralRef,
};

pub(super) enum LinkOccurrence<'doc> {
    Hyperlink(HyperlinkRef<'doc>),
    Image(ImageRef<'doc>),
}

/// Visit navigation links in block bodies, excluding titles, table cells, and callouts.
pub(super) fn visit_links<'doc, B>(
    document: DocumentRef<'doc>,
    mut visit: impl FnMut(LinkOccurrence<'doc>) -> ControlFlow<B>,
) -> ControlFlow<B> {
    for block in document.blocks() {
        visit_structure(block.into(), &mut visit)?;
    }
    ControlFlow::Continue(())
}

fn visit_structure<'doc, B>(
    node: StructuralRef<'doc>,
    visit: &mut impl FnMut(LinkOccurrence<'doc>) -> ControlFlow<B>,
) -> ControlFlow<B> {
    match node {
        StructuralRef::Block(block) => match block.kind() {
            BlockKind::Paragraph(paragraph) => visit_content(paragraph.content(), visit)?,
            BlockKind::Image(image) => visit(LinkOccurrence::Image(image))?,
            BlockKind::Listing(block) => visit_content(block.content(), visit)?,
            BlockKind::Literal(block) => visit_content(block.content(), visit)?,
            BlockKind::Passthrough(block) => visit_content(block.content(), visit)?,
            BlockKind::Verse(block) => visit_content(block.content(), visit)?,
            BlockKind::DelimitedComment(block) => visit_content(block.content(), visit)?,
            BlockKind::Section(_)
            | BlockKind::Admonition(_)
            | BlockKind::Example(_)
            | BlockKind::Open(_)
            | BlockKind::Sidebar(_)
            | BlockKind::Quote(_)
            | BlockKind::UnorderedList(_)
            | BlockKind::OrderedList(_)
            | BlockKind::DescriptionList(_) => {}
            BlockKind::Table(_)
            | BlockKind::CalloutList(_)
            | BlockKind::DiscreteHeader(_)
            | BlockKind::DocumentAttribute(_)
            | BlockKind::TableOfContents(_)
            | BlockKind::ThematicBreak(_)
            | BlockKind::PageBreak(_)
            | BlockKind::Audio(_)
            | BlockKind::Video(_)
            | BlockKind::Comment(_)
            | BlockKind::Stem(_)
            | _ => return ControlFlow::Continue(()),
        },
        StructuralRef::ListItem(item) => visit_content(item.principal(), visit)?,
        StructuralRef::DescriptionListItem(item) => visit_content(item.principal(), visit)?,
        StructuralRef::Document(_)
        | StructuralRef::Header(_)
        | StructuralRef::CalloutListItem(_)
        | StructuralRef::TableRow(_)
        | StructuralRef::TableCell(_)
        | _ => return ControlFlow::Continue(()),
    }
    for child in node.children() {
        visit_structure(child, visit)?;
    }
    ControlFlow::Continue(())
}

fn visit_content<'doc, B>(
    content: ContentRef<'doc>,
    visit: &mut impl FnMut(LinkOccurrence<'doc>) -> ControlFlow<B>,
) -> ControlFlow<B> {
    for inline in content.inlines() {
        if let Some(link) = inline.as_hyperlink() {
            visit(LinkOccurrence::Hyperlink(link))?;
            continue;
        }
        match inline.kind() {
            InlineKind::Bold(span) => visit_content(span.content(), visit)?,
            InlineKind::Italic(span) => visit_content(span.content(), visit)?,
            InlineKind::Monospace(span) => visit_content(span.content(), visit)?,
            InlineKind::Highlight(span) => visit_content(span.content(), visit)?,
            InlineKind::Subscript(span) => visit_content(span.content(), visit)?,
            InlineKind::Superscript(span) => visit_content(span.content(), visit)?,
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Callout(_)
            | InlineKind::Footnote(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | _ => {}
        }
    }
    ControlFlow::Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_lookup_stops_before_later_links() -> Result<(), acdc_parser::Error> {
        let parsed = acdc_parser::parse(
            "* *link:first.adoc[First]*\n+\nlink:second.adoc[Second]\n",
            &acdc_parser::Options::default(),
        )?;
        let mut visited = 0;
        let result = visit_links(parsed.document_ref(), |link| {
            visited += 1;
            ControlFlow::Break(match link {
                LinkOccurrence::Hyperlink(link) => Some(link.target().to_string()),
                LinkOccurrence::Image(_) => None,
            })
        });
        assert_eq!(
            result.break_value().flatten().as_deref(),
            Some("first.adoc")
        );
        assert_eq!(visited, 1);
        Ok(())
    }
}
