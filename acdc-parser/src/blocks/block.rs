use std::collections::HashMap;

use acdc_core::{AttributeName, DocumentAttributes, Location, Position};
use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    blocks::list::parse_list,
    model::{
        Anchor, Audio, Block, BlockMetadata, DelimitedBlock, Image, PageBreak, Paragraph, Section,
        ThematicBreak, Video,
    },
    Error, Rule,
};

impl BlockExt for Block {
    fn set_metadata(&mut self, metadata: BlockMetadata) {
        match self {
            Block::DiscreteHeader(_header) => {}
            Block::DocumentAttribute(_attr) => {}
            Block::ThematicBreak(_thematic_break) => {}
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
        }
    }

    fn set_attributes(&mut self, attributes: HashMap<AttributeName, Option<String>>) {
        match self {
            Block::DiscreteHeader(_header) => {}
            Block::DocumentAttribute(_attr) => {}
            Block::ThematicBreak(_thematic_break) => {}
            Block::PageBreak(page_break) => page_break.attributes = attributes,
            Block::UnorderedList(unordered_list) => unordered_list.attributes = attributes,
            Block::OrderedList(ordered_list) => ordered_list.attributes = attributes,
            Block::DescriptionList(description_list) => description_list.attributes = attributes,
            Block::Section(section) => section.attributes = attributes,
            Block::DelimitedBlock(delimited_block) => delimited_block.attributes = attributes,
            Block::Paragraph(paragraph) => paragraph.attributes = attributes,
            Block::Image(image) => image.attributes = attributes,
            Block::Audio(audio) => audio.attributes = attributes,
            Block::Video(video) => video.attributes = attributes,
        }
    }

    fn set_anchors(&mut self, anchors: Vec<Anchor>) {
        match self {
            Block::DiscreteHeader(header) => header.anchors = anchors,
            Block::DocumentAttribute(_attr) => {}
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
        }
    }

    fn set_title(&mut self, title: String) {
        match self {
            Block::DiscreteHeader(header) => header.title = title,
            Block::DocumentAttribute(_attr) => {}
            Block::ThematicBreak(thematic_break) => thematic_break.title = Some(title),
            Block::PageBreak(page_break) => page_break.title = Some(title),
            Block::UnorderedList(unordered_list) => unordered_list.title = Some(title),
            Block::OrderedList(ordered_list) => ordered_list.title = Some(title),
            Block::DescriptionList(description_list) => description_list.title = Some(title),
            Block::Section(section) => section.title = title,
            Block::DelimitedBlock(delimited_block) => delimited_block.title = Some(title),
            Block::Paragraph(paragraph) => paragraph.title = Some(title),
            Block::Image(image) => image.title = Some(title),
            Block::Audio(audio) => audio.title = Some(title),
            Block::Video(video) => video.title = Some(title),
        }
    }

    fn set_location(&mut self, location: Location) {
        match self {
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
        }
    }
}

pub(crate) trait BlockExt {
    fn set_location(&mut self, location: Location);
    fn set_anchors(&mut self, anchor: Vec<Anchor>);
    fn set_title(&mut self, title: String);
    fn set_attributes(&mut self, attributes: HashMap<AttributeName, Option<String>>);
    fn set_metadata(&mut self, metadata: BlockMetadata);
}

impl std::fmt::Display for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
        }
    }
}

impl Block {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut title = None;
        let mut anchors = Vec::new();
        let mut metadata = BlockMetadata::default();
        let mut attributes = HashMap::new();
        let mut style_found = false;
        let mut location = Location::default();
        let mut block = Block::Paragraph(Paragraph {
            metadata: BlockMetadata::default(),
            attributes: HashMap::new(),
            title: None,
            content: Vec::new(),
            location: location.clone(),
            admonition: None,
        });

        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.start = Position {
                    line: pair.as_span().start_pos().line_col().0,
                    column: pair.as_span().start_pos().line_col().1,
                };
            }
            if i == len - 1 {
                location.end = Position {
                    line: pair.as_span().end_pos().line_col().0,
                    column: pair.as_span().end_pos().line_col().1,
                };
            }
            match pair.as_rule() {
                Rule::anchor => anchors.push(Anchor::parse(pair.into_inner())),
                Rule::section => block = Section::parse(&pair, parent_attributes)?,
                Rule::delimited_block => {
                    block = DelimitedBlock::parse(
                        pair.into_inner(),
                        title.clone(),
                        &metadata,
                        &attributes,
                        parent_attributes,
                    )?;
                }
                Rule::paragraph => {
                    block =
                        Paragraph::parse(pair, &mut metadata, &mut attributes, parent_attributes)?;
                }
                Rule::list => block = parse_list(pair.into_inner(), parent_attributes)?,
                Rule::image_block => {
                    block = Image::parse(
                        pair.into_inner(),
                        &mut metadata,
                        &mut attributes,
                        parent_attributes,
                    );
                }
                Rule::audio_block => {
                    block = Audio::parse(
                        pair.into_inner(),
                        &mut metadata,
                        &mut attributes,
                        parent_attributes,
                    );
                }
                Rule::video_block => {
                    block = Video::parse(
                        pair.into_inner(),
                        &mut metadata,
                        &mut attributes,
                        parent_attributes,
                    );
                }
                Rule::option => metadata.options.push(pair.as_str().to_string()),
                Rule::role => metadata.roles.push(pair.as_str().to_string()),
                Rule::empty_style => {
                    style_found = true;
                }
                Rule::title => {
                    title = Some(pair.as_str().to_string());
                }
                Rule::thematic_break_block => {
                    let thematic_break = ThematicBreak {
                        anchors: anchors.clone(),
                        title: title.clone(),
                        location: location.clone(),
                    };
                    block = Block::ThematicBreak(thematic_break);
                }
                Rule::page_break_block => {
                    block = Block::PageBreak(PageBreak {
                        title: title.clone(),
                        metadata: metadata.clone(),
                        attributes: attributes.clone(),
                        location: location.clone(),
                    });
                }
                Rule::positional_attribute_value => {
                    let value = pair.as_str().to_string();
                    if !value.is_empty() {
                        if metadata.style.is_none() && !style_found {
                            metadata.style = Some(value);
                        } else {
                            attributes.insert(value, None);
                        }
                    }
                }
                Rule::named_attribute => {
                    Self::parse_named_attribute(pair.into_inner(), &mut attributes, &mut metadata);
                }

                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        block.set_location(location);
        block.set_anchors(anchors);
        block.set_attributes(attributes);
        block.set_metadata(metadata);
        if let Some(title) = title {
            block.set_title(title);
        }
        Ok(block)
    }

    pub(crate) fn parse_named_attribute(
        pairs: Pairs<Rule>,
        attributes: &mut HashMap<AttributeName, Option<String>>,
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
            } else {
                attributes.insert(name, value);
            }
        }
    }
}
