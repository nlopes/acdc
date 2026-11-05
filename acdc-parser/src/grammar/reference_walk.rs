use crate::{
    Anchor, Block, BlockMetadata, Caption, ColumnStyle, ContentAttribute, CrossReference,
    DelimitedBlockType, Header, InlineMacro, InlineMetadata, InlineNode, Location, Plain, Section,
    Title,
};

use super::marked_text::MarkedText;

pub(crate) trait ReferenceVisitor<'a> {
    type Error;

    fn header(&mut self, _header: &Header<'a>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn section(
        &mut self,
        _section: &mut Section<'a>,
        _record_toc: bool,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn anchor(
        &mut self,
        _anchor: &Anchor<'a>,
        _label: Option<&ContentAttribute<'a>>,
        _metadata: Option<&BlockMetadata<'a>>,
        _title: Option<Title<'a>>,
        _caption: Option<Caption<'a>>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn untitled(&mut self, _id: &'a str, _location: &Location) -> Result<(), Self::Error> {
        Ok(())
    }

    fn cross_reference(&mut self, _reference: &CrossReference<'a>) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(crate) fn metadata_xreflabel<'a, 'b>(
    metadata: &'b BlockMetadata<'a>,
) -> Option<&'b ContentAttribute<'a>> {
    metadata.reftext.as_ref().or_else(|| {
        metadata
            .anchors
            .last()
            .and_then(|anchor| anchor.xreflabel.as_ref())
    })
}

pub(crate) fn header_title<'a>(header: &Header<'a>) -> Title<'a> {
    let mut inlines = header.title.clone().into_inlines();
    if let Some(subtitle) = &header.subtitle {
        inlines.push(InlineNode::PlainText(Plain::new(
            ": ",
            header.location.clone(),
        )));
        inlines.extend(subtitle.clone().into_inlines());
    }
    Title::new(inlines)
}

pub(crate) fn header<'a, V: ReferenceVisitor<'a>>(
    visitor: &mut V,
    header: &Header<'a>,
) -> Result<(), V::Error> {
    visitor.header(header)?;
    inlines(visitor, header.title.as_inlines())?;
    if let Some(subtitle) = &header.subtitle {
        inlines(visitor, subtitle.as_inlines())?;
    }
    metadata(visitor, &header.metadata)
}

pub(crate) fn metadata<'a, V: ReferenceVisitor<'a>>(
    visitor: &mut V,
    metadata: &BlockMetadata<'a>,
) -> Result<(), V::Error> {
    if let Some(label) = metadata_xreflabel(metadata) {
        inlines(visitor, label.as_inlines())?;
    }
    if let Some(attribution) = &metadata.attribution {
        inlines(visitor, attribution.as_inlines())?;
    }
    if let Some(citetitle) = &metadata.citetitle {
        inlines(visitor, citetitle.as_inlines())?;
    }
    Ok(())
}

pub(crate) fn blocks<'a, V: ReferenceVisitor<'a>>(
    visitor: &mut V,
    blocks: &mut [Block<'a>],
    record_toc: bool,
) -> Result<(), V::Error> {
    for block in blocks {
        if let Block::Section(section) = block {
            visitor.section(section, record_toc)?;
        } else if let Some(anchor) = block.anchor() {
            let metadata = block.metadata();
            let label = metadata
                .and_then(metadata_xreflabel)
                .or(anchor.xreflabel.as_ref());
            visitor.anchor(
                anchor,
                label,
                metadata,
                block.title().cloned(),
                metadata.and_then(|metadata| metadata.caption.clone()),
            )?;
        }
        if let Some(title) = block.title() {
            inlines(visitor, title.as_inlines())?;
        }
        if let Some(value) = block.metadata() {
            metadata(visitor, value)?;
        }
        match block {
            Block::Section(section) => self::blocks(visitor, &mut section.content, record_toc)?,
            Block::Paragraph(paragraph) => inlines(visitor, &paragraph.content)?,
            Block::Admonition(admonition) => {
                self::blocks(visitor, &mut admonition.blocks, record_toc)?;
            }
            Block::UnorderedList(list) => {
                for item in &mut list.items {
                    inlines(visitor, &item.principal)?;
                    self::blocks(visitor, &mut item.blocks, record_toc)?;
                }
            }
            Block::OrderedList(list) => {
                for item in &mut list.items {
                    inlines(visitor, &item.principal)?;
                    self::blocks(visitor, &mut item.blocks, record_toc)?;
                }
            }
            Block::CalloutList(list) => {
                for item in &mut list.items {
                    inlines(visitor, &item.principal)?;
                    self::blocks(visitor, &mut item.blocks, record_toc)?;
                }
            }
            Block::DescriptionList(list) => {
                for item in &mut list.items {
                    for anchor in &item.anchors {
                        visitor.anchor(anchor, anchor.xreflabel.as_ref(), None, None, None)?;
                    }
                    inlines(visitor, &item.term)?;
                    inlines(visitor, &item.principal_text)?;
                    self::blocks(visitor, &mut item.description, record_toc)?;
                }
            }
            Block::DelimitedBlock(block) => delimited(visitor, &mut block.inner, record_toc)?,
            Block::DiscreteHeader(_)
            | Block::ThematicBreak(_)
            | Block::PageBreak(_)
            | Block::Image(_)
            | Block::Audio(_)
            | Block::Video(_)
            | Block::TableOfContents(_)
            | Block::DocumentAttribute(_)
            | Block::Comment(_) => {}
        }
    }
    Ok(())
}

fn delimited<'a, V: ReferenceVisitor<'a>>(
    visitor: &mut V,
    inner: &mut DelimitedBlockType<'a>,
    record_toc: bool,
) -> Result<(), V::Error> {
    match inner {
        DelimitedBlockType::DelimitedExample(content)
        | DelimitedBlockType::DelimitedOpen(content)
        | DelimitedBlockType::DelimitedSidebar(content)
        | DelimitedBlockType::DelimitedQuote(content) => blocks(visitor, content, record_toc),
        DelimitedBlockType::DelimitedListing(content)
        | DelimitedBlockType::DelimitedLiteral(content)
        | DelimitedBlockType::DelimitedPass(content)
        | DelimitedBlockType::DelimitedVerse(content)
        | DelimitedBlockType::DelimitedComment(content) => inlines(visitor, content),
        DelimitedBlockType::DelimitedTable(table) => {
            for row in table
                .header
                .iter_mut()
                .chain(table.rows.iter_mut())
                .chain(table.footer.iter_mut())
            {
                for column in &mut row.columns {
                    blocks(
                        visitor,
                        &mut column.content,
                        record_toc && column.style != Some(ColumnStyle::AsciiDoc),
                    )?;
                }
            }
            Ok(())
        }
        DelimitedBlockType::DelimitedStem(_) => Ok(()),
    }
}

fn formatted<'a, V, T>(visitor: &mut V, text: &T) -> Result<(), V::Error>
where
    V: ReferenceVisitor<'a>,
    T: MarkedText<'a, Content = Vec<InlineNode<'a>>>,
{
    if let Some(id) = text.id() {
        visitor.untitled(id, text.location())?;
    }
    inlines(visitor, text.content())
}

fn link<'a, V: ReferenceVisitor<'a>>(
    visitor: &mut V,
    metadata: &InlineMetadata<'a>,
    location: &Location,
    text: &[InlineNode<'a>],
) -> Result<(), V::Error> {
    if let Some(id) = &metadata.id {
        visitor.untitled(id.id, location)?;
    }
    inlines(visitor, text)
}

pub(crate) fn inlines<'a, V: ReferenceVisitor<'a>>(
    visitor: &mut V,
    content: &[InlineNode<'a>],
) -> Result<(), V::Error> {
    for inline in content {
        match inline {
            InlineNode::InlineAnchor(anchor) => {
                visitor.anchor(anchor, anchor.xreflabel.as_ref(), None, None, None)?;
                if let Some(label) = anchor.bibliography_label() {
                    inlines(visitor, label)?;
                }
            }
            InlineNode::Macro(InlineMacro::CrossReference(reference)) => {
                visitor.cross_reference(reference)?;
                inlines(visitor, &reference.text)?;
            }
            InlineNode::Macro(InlineMacro::Footnote(note)) => inlines(visitor, &note.content)?,
            InlineNode::Macro(InlineMacro::IndexTerm(term)) if term.is_visible() => {
                inlines(visitor, term.term())?;
            }
            InlineNode::Macro(InlineMacro::Link(node)) => {
                link(visitor, &node.metadata, &node.location, &node.text)?;
            }
            InlineNode::Macro(InlineMacro::Url(node)) => {
                link(visitor, &node.metadata, &node.location, &node.text)?;
            }
            InlineNode::Macro(InlineMacro::Mailto(node)) => {
                link(visitor, &node.metadata, &node.location, &node.text)?;
            }
            InlineNode::BoldText(node) => formatted(visitor, node)?,
            InlineNode::ItalicText(node) => formatted(visitor, node)?,
            InlineNode::MonospaceText(node) => formatted(visitor, node)?,
            InlineNode::HighlightText(node) => formatted(visitor, node)?,
            InlineNode::SubscriptText(node) => formatted(visitor, node)?,
            InlineNode::SuperscriptText(node) => formatted(visitor, node)?,
            InlineNode::CurvedQuotationText(node) => formatted(visitor, node)?,
            InlineNode::CurvedApostropheText(node) => formatted(visitor, node)?,
            InlineNode::RawText(node) => {
                if let Some(id) = node
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.id.as_ref())
                {
                    visitor.untitled(id.id, &node.location)?;
                }
            }
            InlineNode::PlainText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::Symbol(_)
            | InlineNode::CharacterReference(_)
            | InlineNode::LineBreak(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_) => {}
        }
    }
    Ok(())
}
