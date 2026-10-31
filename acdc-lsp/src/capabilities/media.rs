use std::convert::Infallible;

use acdc_converters_core::{TraversalContext, visitor::BlockVisitor};
use acdc_parser::{
    ColumnStyle, Location,
    ast::{
        BlockKind, BlockRef, ColumnFormatRef, ContentRef, ContentRole, DocumentRef, InlineKind,
        SourceKind, SourceRef, StructuralRef, TableCellRef, TableRef,
    },
};

use crate::state::document::OwnedSource;

/// Index selected media content with source-ordered attributes and cell scopes.
pub(crate) fn collect_media_sources(document: DocumentRef<'_>) -> Vec<(OwnedSource, Location)> {
    let mut sources = Vec::new();
    let mut context = TraversalContext::new(document.attributes());
    let Ok(()) = context.visit_blocks(&mut MediaVisitor(&mut sources), document.blocks());
    sources
}

struct MediaVisitor<'out>(&'out mut Vec<(OwnedSource, Location)>);

impl<'doc> BlockVisitor<'doc> for MediaVisitor<'_> {
    type Error = Infallible;

    fn visit_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        collect_node(block.into(), context, self.0);
        Ok(())
    }
}

fn collect_node<'doc>(
    node: StructuralRef<'doc>,
    context: &mut TraversalContext<'doc>,
    sources: &mut Vec<(OwnedSource, Location)>,
) {
    collect_field(node, ContentRole::Caption, context, sources);
    match node {
        StructuralRef::Block(block) => match block.kind() {
            BlockKind::Image(image) => {
                collect_field(node, ContentRole::Alt, context, sources);
                sources.push((
                    resolve_source(image.source(), context),
                    image.location().into(),
                ));
            }
            BlockKind::Audio(audio) => {
                sources.push((
                    resolve_source(audio.source(), context),
                    audio.location().into(),
                ));
            }
            BlockKind::Video(video) => {
                sources.push((
                    resolve_source(video.source(), context),
                    video.location().into(),
                ));
            }
            BlockKind::Paragraph(_)
            | BlockKind::Listing(_)
            | BlockKind::Literal(_)
            | BlockKind::Passthrough(_)
            | BlockKind::Verse(_)
            | BlockKind::DelimitedComment(_) => {
                collect_field(node, ContentRole::Body, context, sources);
            }
            BlockKind::Table(table) => {
                collect_table(table, context, sources);
                return;
            }
            BlockKind::DocumentAttribute(_)
            | BlockKind::Section(_)
            | BlockKind::Admonition(_)
            | BlockKind::Example(_)
            | BlockKind::Open(_)
            | BlockKind::Sidebar(_)
            | BlockKind::Quote(_)
            | BlockKind::OrderedList(_)
            | BlockKind::UnorderedList(_)
            | BlockKind::DescriptionList(_)
            | BlockKind::CalloutList(_) => {}
            BlockKind::DiscreteHeader(_)
            | BlockKind::TableOfContents(_)
            | BlockKind::ThematicBreak(_)
            | BlockKind::PageBreak(_)
            | BlockKind::Comment(_)
            | BlockKind::Stem(_)
            | _ => return,
        },
        StructuralRef::ListItem(_)
        | StructuralRef::CalloutListItem(_)
        | StructuralRef::DescriptionListItem(_) => {
            collect_field(node, ContentRole::ListPrincipal, context, sources);
        }
        StructuralRef::Document(_)
        | StructuralRef::Header(_)
        | StructuralRef::TableRow(_)
        | StructuralRef::TableCell(_)
        | _ => return,
    }
    for child in node.children() {
        if let StructuralRef::Block(block) = child {
            let Ok(()) = context.visit_block(&mut MediaVisitor(sources), block);
        } else {
            collect_node(child, context, sources);
        }
    }
}

fn collect_field(
    node: StructuralRef<'_>,
    role: ContentRole,
    context: &TraversalContext<'_>,
    sources: &mut Vec<(OwnedSource, Location)>,
) {
    for field in node.content_fields().filter(|field| field.role() == role) {
        collect_inlines(field.content(), context, sources);
    }
}

fn collect_table<'doc>(
    table: TableRef<'doc>,
    context: &mut TraversalContext<'doc>,
    sources: &mut Vec<(OwnedSource, Location)>,
) {
    for row in table
        .header()
        .into_iter()
        .chain(table.rows())
        .chain(table.footer())
    {
        for (index, cell) in row.cells().enumerate() {
            let style = cell.style().unwrap_or_else(|| {
                table
                    .columns()
                    .nth(index)
                    .map_or(ColumnStyle::Default, ColumnFormatRef::style)
            });
            if style == ColumnStyle::AsciiDoc {
                context.with_table_cell(cell, |context| collect_cell(cell, context, sources));
            } else {
                collect_cell(cell, context, sources);
            }
        }
    }
}

fn collect_cell<'doc>(
    cell: TableCellRef<'doc>,
    context: &mut TraversalContext<'doc>,
    sources: &mut Vec<(OwnedSource, Location)>,
) {
    let Ok(()) = context.visit_blocks(&mut MediaVisitor(sources), cell.blocks());
}

fn collect_inlines(
    nodes: ContentRef<'_>,
    context: &TraversalContext<'_>,
    sources: &mut Vec<(OwnedSource, Location)>,
) {
    for inline in nodes.inlines() {
        let selected_span = matches!(
            inline.kind(),
            InlineKind::Bold(_)
                | InlineKind::Italic(_)
                | InlineKind::Monospace(_)
                | InlineKind::Highlight(_)
                | InlineKind::Subscript(_)
                | InlineKind::Superscript(_)
        );
        for field in inline.content_fields() {
            if matches!(
                field.role(),
                ContentRole::Alt
                    | ContentRole::Tooltip
                    | ContentRole::ButtonLabel
                    | ContentRole::KeyboardKey
                    | ContentRole::MenuTarget
                    | ContentRole::MenuItem
            ) || (selected_span && field.role() == ContentRole::Span)
            {
                collect_inlines(field.content(), context, sources);
            }
        }
        if let InlineKind::Image(image) = inline.kind() {
            sources.push((
                resolve_source(image.source(), context),
                image.location().into(),
            ));
        }
    }
}

fn resolve_source(source: SourceRef<'_>, context: &TraversalContext<'_>) -> OwnedSource {
    let path = match source.kind() {
        SourceKind::Path(path) => path,
        SourceKind::Url(url) => return OwnedSource::Url(url.to_owned()),
        SourceKind::Name(name) => return OwnedSource::Name(name.to_owned()),
        _ => return OwnedSource::Name(source.to_string()),
    };
    if path.is_absolute() {
        return OwnedSource::Path(path.to_path_buf());
    }
    let Some(directory) = context
        .get("imagesdir")
        .and_then(|value| value.text())
        .map(acdc_parser::strip_quotes)
        .filter(|directory| !directory.is_empty())
    else {
        return OwnedSource::Path(path.to_path_buf());
    };
    if directory.contains("://") || directory.starts_with("//") {
        OwnedSource::Url(format!(
            "{}/{}",
            directory.trim_end_matches('/'),
            path.display()
        ))
    } else {
        OwnedSource::Path(std::path::Path::new(directory).join(path))
    }
}
