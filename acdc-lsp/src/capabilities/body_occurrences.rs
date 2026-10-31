use std::ops::ControlFlow;

use acdc_parser::ast::{
    BlockKind, ContentRef, DocumentRef, ImageRef, InlineKind, InlineRef, SectionRef, StructuralRef,
};

pub(super) enum BodyOccurrence<'doc> {
    Section(SectionRef<'doc>),
    Inline(InlineRef<'doc>),
    Image(ImageRef<'doc>),
}

/// Visit sections, body inlines, and block images for navigation displays.
/// Title inlines, table cells, callouts, and macro or curved-span content are excluded.
pub(super) fn visit_body<'doc, B>(
    document: DocumentRef<'doc>,
    mut visit: impl FnMut(BodyOccurrence<'doc>) -> ControlFlow<B>,
) -> ControlFlow<B> {
    for block in document.blocks() {
        visit_structure(block.into(), &mut visit)?;
    }
    ControlFlow::Continue(())
}

fn visit_structure<'doc, B>(
    node: StructuralRef<'doc>,
    visit: &mut impl FnMut(BodyOccurrence<'doc>) -> ControlFlow<B>,
) -> ControlFlow<B> {
    match node {
        StructuralRef::Block(block) => match block.kind() {
            BlockKind::Paragraph(paragraph) => visit_content(paragraph.content(), visit)?,
            BlockKind::Image(image) => visit(BodyOccurrence::Image(image))?,
            BlockKind::Listing(block) => visit_content(block.content(), visit)?,
            BlockKind::Literal(block) => visit_content(block.content(), visit)?,
            BlockKind::Passthrough(block) => visit_content(block.content(), visit)?,
            BlockKind::Verse(block) => visit_content(block.content(), visit)?,
            BlockKind::DelimitedComment(block) => visit_content(block.content(), visit)?,
            BlockKind::Section(section) => visit(BodyOccurrence::Section(section))?,
            BlockKind::Admonition(_)
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
    visit: &mut impl FnMut(BodyOccurrence<'doc>) -> ControlFlow<B>,
) -> ControlFlow<B> {
    for inline in content.inlines() {
        visit(BodyOccurrence::Inline(inline))?;
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
