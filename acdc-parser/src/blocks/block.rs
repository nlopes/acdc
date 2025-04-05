use std::str::FromStr;

use pest::{Parser as _, iterators::{Pair, Pairs}};
use tracing::instrument;

use crate::{
    Admonition, AdmonitionVariant, Anchor, AttributeValue, Audio, Block, BlockMetadata,
    DelimitedBlock, DelimitedBlockType, DocumentAttributes, ElementAttributes, Error, Image,
    InlineNode, InlinePreprocessorParserState, InnerPestParser, Location, Options, PageBreak,
    Paragraph, Rule, Section, TableOfContents, ThematicBreak, Video, blocks::list::parse_list,
    inline_preprocessing, inlines::parse_inlines,
};

impl BlockExt for Block {
    fn set_metadata(&mut self, metadata: BlockMetadata) {
        match self {
            Block::TableOfContents(_)
            | Block::DiscreteHeader(_)
            | Block::DocumentAttribute(_)
            | Block::ThematicBreak(_)
            | Block::_DiscreteHeaderSection(_) => {}
            Block::PageBreak(page_break) => page_break.metadata = metadata,
            Block::UnorderedList(unordered_list) => unordered_list.metadata = metadata,
            Block::OrderedList(ordered_list) => ordered_list.metadata = metadata,
            Block::DescriptionList(description_list) => description_list.metadata = metadata,
            Block::Section(section) => section.metadata = metadata,
            Block::DelimitedBlock(delimited_block) => delimited_block.metadata = metadata,
            Block::Paragraph(paragraph) => paragraph.metadata = metadata,
            Block::Image(image) => image.metadata = metadata,
            Block::Audio(audio) => audio.metadata = metadata,
            Block::Video(video) => video.metadata = metadata,
            Block::Admonition(admonition) => admonition.metadata = metadata,
        }
    }

    fn set_attributes(&mut self, attributes: ElementAttributes) {
        match self {
            Block::TableOfContents(_)
            | Block::DiscreteHeader(_)
            | Block::DocumentAttribute(_)
            | Block::ThematicBreak(_)
            | Block::_DiscreteHeaderSection(_) => {}
            Block::PageBreak(page_break) => page_break.metadata.attributes = attributes,
            Block::UnorderedList(unordered_list) => unordered_list.metadata.attributes = attributes,
            Block::OrderedList(ordered_list) => ordered_list.metadata.attributes = attributes,
            Block::DescriptionList(description_list) => {
                description_list.metadata.attributes = attributes;
            }
            Block::Section(section) => section.metadata.attributes = attributes,
            Block::DelimitedBlock(delimited_block) => {
                delimited_block.metadata.attributes = attributes;
            }
            Block::Paragraph(paragraph) => paragraph.metadata.attributes = attributes,
            Block::Image(image) => image.metadata.attributes = attributes,
            Block::Audio(audio) => audio.metadata.attributes = attributes,
            Block::Video(video) => video.metadata.attributes = attributes,
            Block::Admonition(admonition) => admonition.metadata.attributes = attributes,
        }
    }

    fn set_anchors(&mut self, anchors: Vec<Anchor>) {
        match self {
            Block::TableOfContents(_) | Block::DocumentAttribute(_) => {}

            Block::DiscreteHeader(header) => header.anchors = anchors,
            Block::ThematicBreak(thematic_break) => thematic_break.anchors = anchors,
            Block::PageBreak(page_break) => page_break.metadata.anchors = anchors,
            Block::UnorderedList(unordered_list) => unordered_list.metadata.anchors = anchors,
            Block::OrderedList(ordered_list) => ordered_list.metadata.anchors = anchors,
            Block::DescriptionList(description_list) => description_list.metadata.anchors = anchors,
            Block::Section(section) => section.metadata.anchors = anchors,
            Block::DelimitedBlock(delimited_block) => delimited_block.metadata.anchors = anchors,
            Block::Paragraph(paragraph) => paragraph.metadata.anchors = anchors,
            Block::Image(image) => image.metadata.anchors = anchors,
            Block::Audio(audio) => audio.metadata.anchors = anchors,
            Block::Video(video) => video.metadata.anchors = anchors,
            Block::Admonition(admonition) => admonition.metadata.anchors = anchors,
            Block::_DiscreteHeaderSection(section) => section.anchors = anchors,
        }
    }

    fn set_title(&mut self, title: Vec<InlineNode>) {
        match self {
            Block::TableOfContents(_) | Block::DocumentAttribute(_) => {}
            Block::DiscreteHeader(header) => header.title = title,
            Block::ThematicBreak(thematic_break) => thematic_break.title = title,
            Block::PageBreak(page_break) => page_break.title = title,
            Block::UnorderedList(unordered_list) => unordered_list.title = title,
            Block::OrderedList(ordered_list) => ordered_list.title = title,
            Block::DescriptionList(description_list) => description_list.title = title,
            Block::Section(section) => section.title = title,
            Block::DelimitedBlock(delimited_block) => delimited_block.title = title,
            Block::Paragraph(paragraph) => paragraph.title = title,
            Block::Image(image) => image.title = title,
            Block::Audio(audio) => audio.title = title,
            Block::Video(video) => video.title = title,
            Block::Admonition(admonition) => admonition.title = title,
            Block::_DiscreteHeaderSection(section) => section.title = title,
        }
    }

    fn set_location(&mut self, location: Location) {
        match self {
            Block::TableOfContents(toc) => toc.location = location,
            Block::DiscreteHeader(header) => header.location = location,
            Block::DocumentAttribute(attr) => attr.location = location,
            Block::ThematicBreak(thematic_break) => thematic_break.location = location,
            Block::PageBreak(page_break) => page_break.location = location,
            Block::UnorderedList(unordered_list) => unordered_list.location = location,
            Block::OrderedList(ordered_list) => ordered_list.location = location,
            Block::DescriptionList(description_list) => description_list.location = location,
            Block::Section(section) => section.location = location,
            Block::DelimitedBlock(delimited_block) => delimited_block.location = location,
            Block::Paragraph(paragraph) => paragraph.location = location,
            Block::Image(image) => image.location = location,
            Block::Audio(audio) => audio.location = location,
            Block::Video(video) => video.location = location,
            Block::Admonition(admonition) => admonition.location = location,
            Block::_DiscreteHeaderSection(section) => section.location = location,
        }
    }

    fn is_admonition(&self) -> bool {
        matches!(self, Block::Admonition(_))
    }

    fn set_admonition_blocks(&mut self, blocks: Vec<Block>) {
        if let Block::Admonition(admonition) = self {
            admonition.blocks = blocks;
        }
    }
}

pub(crate) trait BlockExt {
    fn set_location(&mut self, location: Location);
    fn set_anchors(&mut self, anchor: Vec<Anchor>);
    fn set_title(&mut self, title: Vec<InlineNode>);
    fn set_attributes(&mut self, attributes: ElementAttributes);
    fn set_metadata(&mut self, metadata: BlockMetadata);
    fn is_admonition(&self) -> bool;
    fn set_admonition_blocks(&mut self, blocks: Vec<Block>);
}

impl std::fmt::Display for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Block::TableOfContents(_) => write!(f, "TableOfContents"),
            Block::DiscreteHeader(_) => write!(f, "DiscreteHeader"),
            Block::DocumentAttribute(_) => write!(f, "DocumentAttribute"),
            Block::ThematicBreak(_) => write!(f, "ThematicBreak"),
            Block::PageBreak(_) => write!(f, "PageBreak"),
            Block::UnorderedList(_) => write!(f, "UnorderedList"),
            Block::OrderedList(_) => write!(f, "OrderedList"),
            Block::DescriptionList(_) => write!(f, "DescriptionList"),
            Block::Section(_) => write!(f, "Section"),
            Block::DelimitedBlock(_) => write!(f, "DelimitedBlock"),
            Block::Paragraph(_) => write!(f, "Paragraph"),
            Block::Image(_) => write!(f, "Image"),
            Block::Audio(_) => write!(f, "Audio"),
            Block::Video(_) => write!(f, "Video"),
            Block::Admonition(_) => write!(f, "Admonition"),
            Block::_DiscreteHeaderSection(_) => write!(f, "_DiscreteHeaderSection"),
        }
    }
}

/// Parameters for delimited block parsing
struct DelimitedBlockParams<'a> {
    options: &'a Options,
    title: Vec<InlineNode>,
    metadata: &'a BlockMetadata,
    attributes: &'a ElementAttributes,
    parent_location: Option<&'a Location>,
    parent_attributes: &'a mut DocumentAttributes,
    location: &'a Location,
}

impl Block {
    /// Parse a block title from a title rule
    fn parse_block_title(
        pair: &Pair<Rule>,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Vec<InlineNode>, Error> {
        let text = pair.as_str();
        let start_pos = pair.as_span().start_pos().pos();
        let mut location = Location::from_pair(pair);
        location.shift(parent_location);

        // Run inline preprocessor before parsing inlines
        let mut state = InlinePreprocessorParserState::new();
        state.set_initial_position(&location, start_pos);
        let processed = inline_preprocessing::run(text, parent_attributes, &state)
            .map_err(|e| {
                tracing::error!("error processing block title: {}", e);
                Error::Parse(e.to_string())
            })?;

        let mut pairs = InnerPestParser::parse(Rule::inlines, &processed.text)
            .map_err(|e| Error::Parse(e.to_string()))?;

        parse_inlines(
            pairs.next().ok_or_else(|| {
                tracing::error!("error parsing block title");
                Error::Parse("error parsing block title".to_string())
            })?,
            Some(&processed),
            Some(&location),
            parent_attributes,
        )
    }

    /// Handle delimited block parsing with admonition support
    fn parse_delimited_block_with_admonition(
        pair: Pair<Rule>,
        params: DelimitedBlockParams,
        block: &mut Block,
    ) -> Result<(), Error> {
        let delimited_block = DelimitedBlock::parse(
            pair.into_inner(),
            params.options,
            params.title,
            params.metadata,
            params.attributes,
            params.parent_location,
            params.parent_attributes,
        )?;

        if block.is_admonition() {
            if let Block::DelimitedBlock(maybe_example_block) = delimited_block {
                if let DelimitedBlockType::DelimitedExample(blocks) = maybe_example_block.inner {
                    block.set_admonition_blocks(blocks);
                    // Need to set location here because we might have a
                    // parent location and therefore the check at the return
                    // point of this function fails.
                    block.set_location(params.location.clone());
                }
            } else {
                tracing::warn!(
                    "admonition block with non-example delimited block, skipping"
                );
            }
        } else {
            *block = delimited_block;
        }
        Ok(())
    }

    /// Handle paragraph parsing with admonition support
    fn parse_paragraph_with_admonition(
        pair: Pair<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut ElementAttributes,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
        block: &mut Block,
    ) -> Result<(), Error> {
        let paragraph = Paragraph::parse(
            pair,
            metadata,
            attributes,
            parent_location,
            parent_attributes,
        )?;

        if block.is_admonition() {
            block.set_admonition_blocks(vec![paragraph]);
        } else {
            *block = paragraph;
        }
        Ok(())
    }

    /// Parse media blocks (image, audio, video)
    fn parse_media_block(
        rule: Rule,
        pair: Pair<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut ElementAttributes,
        parent_attributes: &mut DocumentAttributes,
    ) -> Block {
        match rule {
            Rule::image_block => Image::parse(
                pair.into_inner(),
                metadata,
                attributes,
                parent_attributes,
            ),
            Rule::audio_block => Audio::parse(
                pair.into_inner(),
                metadata,
                attributes,
                parent_attributes,
            ),
            Rule::video_block => Video::parse(
                pair.into_inner(),
                metadata,
                attributes,
                parent_attributes,
            ),
            _ => unreachable!("parse_media_block called with non-media rule: {rule:?}"),
        }
    }

    /// Handle ID and block style ID rules
    fn handle_id_rule(
        pair: &Pair<Rule>,
        metadata: &mut BlockMetadata,
        location: &Location,
    ) {
        if metadata.id.is_some() {
            tracing::warn!(
                id = pair.as_str(),
                "block already has an id, ignoring this one"
            );
            return;
        }
        let anchor = Anchor {
            id: pair.as_str().to_string(),
            location: location.clone(),
            ..Default::default()
        };
        metadata.anchors.push(anchor.clone());
        metadata.id = Some(anchor);
    }

    /// Handle break blocks (thematic and page breaks)
    fn parse_break_block(rule: Rule, anchors: Vec<Anchor>, title: Vec<InlineNode>, metadata: BlockMetadata, location: Location) -> Block {
        match rule {
            Rule::thematic_break_block => Block::ThematicBreak(ThematicBreak {
                anchors,
                title,
                location,
            }),
            Rule::page_break_block => Block::PageBreak(PageBreak {
                title,
                metadata,
                location,
            }),
            _ => unreachable!("parse_break_block called with non-break rule: {rule:?}"),
        }
    }

    /// Handle positional attribute value parsing with admonition support
    fn handle_positional_attribute(
        pair: &Pair<Rule>,
        attributes: &mut ElementAttributes,
        metadata: &mut BlockMetadata,
        style_found: bool,
        title: &[InlineNode],
        location: &Location,
        block: &mut Block,
    ) -> Result<(), Error> {
        let value = pair.as_str().to_string();
        if value.is_empty() {
            return Ok(());
        }

        // if we have a positional attribute and it is the first one, then it's the style
        if metadata.style.is_none() && !style_found {
            if AdmonitionVariant::from_str(&value).is_ok() {
                *block = Block::Admonition(Admonition {
                    metadata: metadata.clone(),
                    title: title.to_vec(),
                    blocks: Vec::new(),
                    location: location.clone(),
                    variant: AdmonitionVariant::from_str(&value)?,
                });
            } else {
                metadata.style = Some(value);
            }
        } else {
            attributes.insert(value, AttributeValue::None);
        }
        Ok(())
    }

    /// Finalize block with location, anchors, metadata, attributes, and title
    fn finalize_block(
        block: &mut Block,
        parent_location: Option<&Location>,
        location: Location,
        anchors: Vec<Anchor>,
        metadata: BlockMetadata,
        attributes: ElementAttributes,
        title: Vec<InlineNode>,
    ) {
        // If we have a block that does not have a parent_location set, then we want to
        // set the location to surround everything we've found.
        if parent_location.is_none() {
            block.set_location(location);
        }
        block.set_anchors(anchors);
        block.set_metadata(metadata);
        block.set_attributes(attributes);
        if !title.is_empty() {
            block.set_title(title);
        }
    }

    /// Handle simple metadata rules (option, role, `empty_style`)
    fn handle_simple_metadata_rules(rule: Rule, pair: &Pair<Rule>, metadata: &mut BlockMetadata, style_found: &mut bool) {
        match rule {
            Rule::option => metadata.options.push(pair.as_str().to_string()),
            Rule::role => metadata.roles.push(pair.as_str().to_string()),
            Rule::empty_style => *style_found = true,
            _ => {}
        }
    }
    #[must_use]
    pub fn location(&self) -> &Location {
        match self {
            Block::TableOfContents(toc) => &toc.location,
            Block::Admonition(admonition) => &admonition.location,
            Block::DiscreteHeader(header) => &header.location,
            Block::DocumentAttribute(attr) => &attr.location,
            Block::ThematicBreak(br) => &br.location,
            Block::PageBreak(br) => &br.location,
            Block::UnorderedList(list) => &list.location,
            Block::OrderedList(list) => &list.location,
            Block::DescriptionList(list) => &list.location,
            Block::Section(section) => &section.location,
            Block::DelimitedBlock(block) => &block.location,
            Block::Paragraph(paragraph) => &paragraph.location,
            Block::Image(image) => &image.location,
            Block::Audio(audio) => &audio.location,
            Block::Video(video) => &video.location,
            Block::_DiscreteHeaderSection(section) => &section.location,
        }
    }

    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        options: &Options,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut title = Vec::new();
        let mut anchors = Vec::new();
        let mut metadata = BlockMetadata::default();
        let mut attributes = ElementAttributes::default();
        let mut style_found = false;
        let mut location = Location::default();
        let mut block = Block::Paragraph(Paragraph {
            metadata: BlockMetadata::default(),
            title: Vec::new(),
            content: Vec::new(),
            location: location.clone(),
        });

        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.set_start_from_pos(&pair.as_span().start_pos());
            }
            if i == len - 1 {
                location.set_end_from_pos(&pair.as_span().end_pos());
            }

            match pair.as_rule() {
                Rule::anchor => anchors.push(Anchor::parse(pair.into_inner())),
                Rule::section => {
                    block = Section::parse(&pair, options, parent_location, parent_attributes)?;
                }
                Rule::delimited_block => {
                    let params = DelimitedBlockParams {
                        options,
                        title: title.clone(),
                        metadata: &metadata,
                        attributes: &attributes,
                        parent_location,
                        parent_attributes,
                        location: &location,
                    };
                    Self::parse_delimited_block_with_admonition(pair, params, &mut block)?;
                }
                Rule::paragraph => {
                    Self::parse_paragraph_with_admonition(
                        pair,
                        &mut metadata,
                        &mut attributes,
                        parent_location,
                        parent_attributes,
                        &mut block,
                    )?;
                }
                Rule::list => {
                    block = parse_list(
                        pair.into_inner(),
                        options,
                        parent_location,
                        parent_attributes,
                    )?;
                }
                Rule::image_block => {
                    block = Self::parse_media_block(
                        Rule::image_block,
                        pair,
                        &mut metadata,
                        &mut attributes,
                        parent_attributes,
                    );
                }
                Rule::audio_block => {
                    block = Self::parse_media_block(
                        Rule::audio_block,
                        pair,
                        &mut metadata,
                        &mut attributes,
                        parent_attributes,
                    );
                }
                Rule::toc_block => {
                    block = Block::TableOfContents(TableOfContents {
                        metadata: metadata.clone(),
                        location: location.clone(),
                    });
                }
                Rule::video_block => {
                    block = Self::parse_media_block(
                        Rule::video_block,
                        pair,
                        &mut metadata,
                        &mut attributes,
                        parent_attributes,
                    );
                }
                Rule::option | Rule::role | Rule::empty_style => {
                    Self::handle_simple_metadata_rules(pair.as_rule(), &pair, &mut metadata, &mut style_found);
                }
                Rule::id | Rule::block_style_id => {
                    Self::handle_id_rule(&pair, &mut metadata, &location);
                }
                Rule::title => {
                    title = Self::parse_block_title(&pair, parent_location, parent_attributes)?;
                }
                Rule::thematic_break_block => {
                    block = Self::parse_break_block(Rule::thematic_break_block, anchors.clone(), title.clone(), metadata.clone(), location.clone());
                }
                Rule::page_break_block => {
                    block = Self::parse_break_block(Rule::page_break_block, anchors.clone(), title.clone(), metadata.clone(), location.clone());
                }
                Rule::positional_attribute_value => {
                    Self::handle_positional_attribute(&pair, &mut attributes, &mut metadata, style_found, &title, &location, &mut block)?;
                }
                Rule::named_attribute => {
                    Self::parse_named_attribute(pair.into_inner(), &mut attributes, &mut metadata);
                }
                Rule::EOI | Rule::comment | Rule::open_sb | Rule::close_sb => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }

        Self::finalize_block(&mut block, parent_location, location, anchors, metadata, attributes, title);
        Ok(block)
    }

    pub(crate) fn parse_named_attribute(
        pairs: Pairs<Rule>,
        attributes: &mut ElementAttributes,
        metadata: &mut BlockMetadata,
    ) {
        let mut name = None;
        let mut value = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::id => {
                    let anchor = Anchor {
                        id: pair.as_str().to_string(),
                        ..Default::default()
                    };
                    metadata.anchors.push(anchor.clone());
                    metadata.id = Some(anchor);
                }
                Rule::role => metadata.roles.push(pair.as_str().to_string()),
                Rule::option => metadata.options.push(pair.as_str().to_string()),
                Rule::attribute_name => name = Some(pair.as_str().to_string()),
                Rule::named_attribute_value => value = Some(pair.as_str().to_string()),
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }

        if let Some(name) = name {
            if name == "role" {
                if let Some(value) = value {
                    metadata.roles.push(value);
                } else {
                    tracing::warn!("named 'role' attribute without value");
                }
            } else if let Some(value) = value {
                attributes.insert(name, AttributeValue::String(value));
            }
        }
    }
}
