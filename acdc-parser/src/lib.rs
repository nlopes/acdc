use std::{collections::HashMap, path::Path, string::ToString};

use pest::{
    iterators::{Pair, Pairs},
    Parser as _,
};
use pest_derive::Parser;
use tracing::instrument;

mod error;
mod model;
mod preprocessor;

use model::BlockExt;
use preprocessor::Preprocessor;

pub use error::{Detail as ErrorDetail, Error};
pub use model::{
    Anchor, AttributeEntry, Author, Block, BlockMetadata, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DescriptionListDescription, DescriptionListItem, DiscreteHeader, Document,
    DocumentAttribute, Header, Image, ImageSource, InlineNode, ListItem, Location, OrderedList,
    PageBreak, Paragraph, Parser, PlainText, Position, Revision, Section, ThematicBreak,
    UnorderedList,
};

#[derive(Debug)]
pub struct PestParser;

#[derive(Parser, Debug)]
#[grammar = "../grammar/block.pest"]
#[grammar = "../grammar/core.pest"]
#[grammar = "../grammar/list.pest"]
#[grammar = "../grammar/delimited.pest"]
#[grammar = "../grammar/document.pest"]
#[grammar = "../grammar/asciidoc.pest"]
struct InnerPestParser;

impl crate::model::Parser for PestParser {
    #[instrument]
    fn parse(&self, input: &str) -> Result<Document, Error> {
        let input = Preprocessor.process(input)?;
        match InnerPestParser::parse(Rule::document, &input) {
            Ok(pairs) => Document::parse(pairs),
            Err(e) => {
                tracing::error!("error preprocessing document: {e}");
                Err(Error::Parse(e.to_string()))
            }
        }
    }

    #[instrument(skip(file_path))]
    fn parse_file<P: AsRef<Path>>(&self, file_path: P) -> Result<Document, Error> {
        let input = Preprocessor.process_file(file_path)?;
        tracing::trace!(?input, "post preprocessor");
        match InnerPestParser::parse(Rule::document, &input) {
            Ok(pairs) => Document::parse(pairs),
            Err(e) => {
                tracing::error!("error preprocessing document: {e}");
                Err(Error::Parse(e.to_string()))
            }
        }
    }
}

impl Document {
    fn parse(pairs: Pairs<Rule>) -> Result<Self, Error> {
        let mut document_header = None;
        let mut content = Vec::new();

        for pair in pairs {
            match pair.as_rule() {
                Rule::document_header => {
                    document_header = Some(parse_document_header(pair.into_inner()));
                }
                Rule::blocks => {
                    content.extend(parse_blocks(pair.into_inner())?);
                }
                Rule::comment | Rule::EOI => {}
                unknown => unimplemented!("{:?}", unknown),
            }
        }

        build_section_tree(&mut content)?;
        validate_section_block_level(&content, None)?;

        Ok(Self {
            header: document_header,
            content,
        })
    }
}

fn build_section_tree_delimited(block: Block, kept_layers: &mut Vec<Block>) -> Result<(), Error> {
    if let Block::DelimitedBlock(delimited_block) = block {
        match &delimited_block.inner {
            DelimitedBlockType::DelimitedExample(blocks) => {
                let mut blocks = blocks.clone();
                build_section_tree(&mut blocks)?;
                kept_layers.push(Block::DelimitedBlock(DelimitedBlock {
                    metadata: delimited_block.metadata,
                    inner: DelimitedBlockType::DelimitedExample(blocks),
                    title: delimited_block.title,
                    attributes: delimited_block.attributes,
                    location: delimited_block.location,
                }));
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                let mut blocks = blocks.clone();
                build_section_tree(&mut blocks)?;
                kept_layers.push(Block::DelimitedBlock(DelimitedBlock {
                    metadata: delimited_block.metadata,
                    inner: DelimitedBlockType::DelimitedQuote(blocks),
                    title: delimited_block.title,
                    attributes: delimited_block.attributes,
                    location: delimited_block.location,
                }));
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                let mut blocks = blocks.clone();
                build_section_tree(&mut blocks)?;
                kept_layers.push(Block::DelimitedBlock(DelimitedBlock {
                    metadata: delimited_block.metadata,
                    inner: DelimitedBlockType::DelimitedOpen(blocks),
                    title: delimited_block.title,
                    attributes: delimited_block.attributes,
                    location: delimited_block.location,
                }));
            }
            DelimitedBlockType::DelimitedSidebar(blocks) => {
                let mut blocks = blocks.clone();
                build_section_tree(&mut blocks)?;
                kept_layers.push(Block::DelimitedBlock(DelimitedBlock {
                    metadata: delimited_block.metadata,
                    inner: DelimitedBlockType::DelimitedSidebar(blocks),
                    title: delimited_block.title,
                    attributes: delimited_block.attributes,
                    location: delimited_block.location,
                }));
            }
            _ => {
                kept_layers.push(Block::DelimitedBlock(delimited_block));
            }
        }
    } else {
        tracing::error!("expected a delimited block");
        return Err(Error::UnexpectedBlock(block.to_string()));
    }
    Ok(())
}

// Build a tree of sections from the content blocks.
fn build_section_tree(document: &mut Vec<Block>) -> Result<(), Error> {
    let mut current_layers = document.clone();
    let mut stack: Vec<Block> = Vec::new();

    current_layers.reverse();

    let mut kept_layers = Vec::new();
    for block in current_layers.drain(..) {
        match (block, stack.is_empty()) {
            (delimited_block @ Block::DelimitedBlock(_), true) => {
                build_section_tree_delimited(delimited_block, &mut kept_layers)?;
            }
            (Block::Section(section), true) => {
                kept_layers.push(Block::Section(section));
            }
            (Block::Section(section), false) => {
                if let Some(style) = &section.metadata.style {
                    if style == "discrete" {
                        stack.push(Block::DiscreteHeader(DiscreteHeader {
                            anchors: section.metadata.anchors.clone(),
                            title: section.title.clone(),
                            level: section.level,
                            location: section.location.clone(),
                        }));
                        continue;
                    }
                }
                let mut section = section;
                while let Some(block_from_stack) = stack.pop() {
                    section.location.end = match &block_from_stack {
                        Block::Section(section) => section.location.end.clone(),
                        Block::DelimitedBlock(delimited_block) => {
                            delimited_block.location.end.clone()
                        }
                        // We don't use paragraph because we don't calculate positions for paragraphs yet
                        Block::Paragraph(_) => section.location.end.clone(),
                        Block::OrderedList(ordered_list) => ordered_list.location.end.clone(),
                        Block::UnorderedList(unordered_list) => unordered_list.location.end.clone(),
                        Block::DocumentAttribute(attribute) => attribute.location.end.clone(),
                        unknown => unimplemented!("{:?}", unknown),
                    };
                    section.content.push(block_from_stack);
                }
                kept_layers.push(Block::Section(section));
            }
            (block, _) => {
                stack.push(block);
            }
        }
    }

    stack.reverse();

    // Add the remaining blocks to the kept_layers
    while let Some(block_from_stack) = stack.pop() {
        kept_layers.push(block_from_stack);
    }

    if !kept_layers.is_empty() {
        let mut i = 0;
        while i < kept_layers.len() - 1 {
            let should_move = {
                if let (Some(Block::Section(section)), Some(Block::Section(next_section))) =
                    (kept_layers.get(i), kept_layers.get(i + 1))
                {
                    // TODO(nlopes): this if here is probably wrong - I added it because I
                    // was tired of debugging but this smells like a bug.
                    if section.level == 0 {
                        false
                    } else {
                        match next_section.level.cmp(&(section.level - 1)) {
                            std::cmp::Ordering::Greater => false,
                            std::cmp::Ordering::Equal => true,
                            std::cmp::Ordering::Less => {
                                let error_detail = ErrorDetail {
                                    location: next_section.location.clone(),
                                };
                                return Err(Error::NestedSectionLevelMismatch(
                                    error_detail,
                                    section.level - 1,
                                    section.level,
                                ));
                            }
                        }
                    }
                } else {
                    false
                }
            };

            if should_move {
                section_tree_move(&mut kept_layers, i)?;
            } else {
                i += 1;
            }
        }
        kept_layers.reverse();
    }
    *document = kept_layers;
    Ok(())
}

fn section_tree_move(kept_layers: &mut Vec<Block>, i: usize) -> Result<(), Error> {
    if let Some(Block::Section(current_section)) = kept_layers.get(i).cloned() {
        if let Some(Block::Section(parent_section)) = kept_layers.get_mut(i + 1) {
            parent_section.location.end = match &current_section.content.last() {
                Some(Block::Section(section)) => section.location.end.clone(),
                Some(Block::DelimitedBlock(delimited_block)) => {
                    delimited_block.location.end.clone()
                }
                Some(Block::Paragraph(paragraph)) => paragraph.location.end.clone(),
                _ => todo!(),
            };
            parent_section.content.push(Block::Section(current_section));
            kept_layers.remove(i);
        } else {
            return Err(Error::Parse("expected a section".to_string()));
        }
    }
    Ok(())
}
fn parse_document_header(pairs: Pairs<Rule>) -> Header {
    let mut title = None;
    let mut subtitle = None;
    let mut authors = Vec::new();
    let mut revision = None;
    let mut attributes = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::document_title_token => {
                for inner_pair in pair.into_inner() {
                    match inner_pair.as_rule() {
                        Rule::document_title => {
                            title = Some(inner_pair.as_str().to_string());
                            // find the subtitle by looking for the last colon in title
                            // andsetting title to everything before the last colon and
                            // subtitle to everything after the last colon
                            if let Some(colon_index) = title.as_ref().unwrap().rfind(':') {
                                subtitle = Some(
                                    title.as_ref().unwrap()[colon_index + 1..]
                                        .trim()
                                        .to_string(),
                                );
                                title =
                                    Some(title.as_ref().unwrap()[..colon_index].trim().to_string());
                            }
                        }
                        unknown => unreachable!("{:?}", unknown),
                    }
                }
            }
            Rule::author => {
                let author = parse_author(pair.into_inner());
                authors.push(author);
            }
            Rule::revision_line => {
                let inner_pairs = pair.into_inner();
                let mut revision_number = String::new();
                let mut revision_date = None;
                let mut revision_remark = None;

                for pair in inner_pairs {
                    match pair.as_rule() {
                        Rule::revision_number => {
                            revision_number = pair.as_str().to_string();
                        }
                        Rule::revision_date => {
                            revision_date = Some(pair.as_str().to_string());
                        }
                        Rule::revision_remark => {
                            revision_remark = Some(pair.as_str().to_string());
                        }
                        unknown => unreachable!("{:?}", unknown),
                    }
                }
                revision = Some(Revision {
                    number: revision_number,
                    date: revision_date,
                    remark: revision_remark,
                });
            }
            Rule::document_attribute => {
                let mut inner_pairs = pair.into_inner();
                let name = inner_pairs.next().map(|p| p.as_str().to_string());
                let value = inner_pairs.next().map(|p| p.as_str().to_string());
                attributes.push(AttributeEntry { name, value });
            }
            unknown => unreachable!("{:?}", unknown),
        }
    }

    Header {
        title,
        subtitle,
        authors,
        revision,
        attributes,
    }
}

fn parse_author(pairs: Pairs<Rule>) -> Author {
    let mut first_name = String::new();
    let mut middle_name = None;
    let mut last_name = String::new();
    let mut email = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::author_first_name => {
                first_name = pair.as_str().to_string();
            }
            Rule::author_middle_name => middle_name = Some(pair.as_str().to_string()),
            Rule::author_last_name => {
                last_name = pair.as_str().to_string();
            }
            Rule::author_email => {
                email = Some(pair.as_str().to_string()).map(|s| s.to_string());
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }

    Author {
        first_name,
        middle_name,
        last_name,
        email,
    }
}

#[instrument(level = "trace")]
fn parse_block(pairs: Pairs<Rule>) -> Result<Block, Error> {
    let mut title = None;
    let mut anchors = Vec::new();
    let mut metadata = BlockMetadata::default();
    let mut attributes = Vec::new();
    let mut style_found = false;
    let mut location = Location {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    };
    let mut block = Block::Paragraph(Paragraph {
        metadata: BlockMetadata::default(),
        attributes: Vec::new(),
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
            Rule::anchor => anchors.push(parse_anchor(pair.into_inner())),
            Rule::section => block = parse_section(&pair)?,
            Rule::delimited_block => block = parse_delimited_block(pair.into_inner())?,
            Rule::paragraph => block = parse_paragraph(pair, &mut metadata, &mut attributes),
            Rule::list => block = parse_list(pair.into_inner())?,
            Rule::image_block => {
                block = parse_image_block(pair.into_inner(), &mut metadata, &mut attributes);
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
                        attributes.push(AttributeEntry {
                            name: None,
                            value: Some(value),
                        });
                    }
                }
            }
            Rule::named_attribute => {
                parse_named_attribute(pair.into_inner(), &mut attributes, &mut metadata);
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

fn parse_blocks(pairs: Pairs<Rule>) -> Result<Vec<Block>, Error> {
    if pairs.len() == 0 {
        return Ok(Vec::new());
    }
    if pairs.peek().is_none() {
        // TODO(nlopes): confirm if this is the correct behavior
        tracing::warn!(?pairs, "empty block");
        return Ok(vec![]);
    }
    let mut blocks = Vec::new();
    for pair in pairs {
        match pair.as_rule() {
            Rule::blocks => {
                blocks.extend(parse_blocks(pair.into_inner())?);
            }
            Rule::block => {
                blocks.push(parse_block(pair.into_inner())?);
            }
            Rule::document_attribute => {
                let mut inner_pairs = pair.clone().into_inner();
                let name = inner_pairs
                    .next()
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_default(); // TODO(nlopes): this will probably end up
                                          // causing a bug
                let value = inner_pairs.next().map(|p| p.as_str().to_string());
                let attribute = DocumentAttribute {
                    name,
                    value,
                    location: Location {
                        start: Position {
                            line: pair.as_span().start_pos().line_col().0,
                            column: pair.as_span().start_pos().line_col().1,
                        },
                        end: Position {
                            line: pair.as_span().end_pos().line_col().0,
                            column: pair.as_span().end_pos().line_col().1,
                        },
                    },
                };
                blocks.push(Block::DocumentAttribute(attribute));
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(blocks)
}

fn parse_image_block(
    pairs: Pairs<Rule>,
    metadata: &mut BlockMetadata,
    attributes: &mut Vec<AttributeEntry>,
) -> Block {
    let mut source = ImageSource::Path(String::new());
    let mut title = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::anchor => {
                tracing::error!("unexpected anchor in image block");
                //parse_metadata_anchor(pair.into_inner(), &mut metadata)
                let anchor = parse_anchor(pair.into_inner());
                metadata.anchors.push(anchor);
            }
            Rule::title => title = Some(pair.as_str().to_string()),
            Rule::image => parse_image(pair.into_inner(), attributes, &mut source, metadata),
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    if let Some(anchor) = metadata.anchors.last() {
        metadata.id = Some(anchor.clone());
    }
    Block::Image(Image {
        location: Location {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        },
        title,
        source,
        metadata: metadata.clone(),
        attributes: attributes.clone(),
    })
}

fn parse_image(
    pairs: Pairs<Rule>,
    attributes: &mut Vec<AttributeEntry>,
    source: &mut ImageSource,
    metadata: &mut BlockMetadata,
) {
    let mut attribute_idx = 0;
    let mut attribute_mapping = HashMap::new();
    attribute_mapping.insert(0, "alt");
    attribute_mapping.insert(1, "width");
    attribute_mapping.insert(2, "height");

    for pair in pairs {
        match pair.as_rule() {
            Rule::path => *source = ImageSource::Path(pair.as_str().to_string()),
            Rule::url => *source = ImageSource::Url(pair.as_str().to_string()),
            Rule::named_attribute => {
                parse_named_attribute(pair.into_inner(), attributes, metadata);
            }
            Rule::positional_attribute_value => {
                let value = pair.as_str().to_string();
                attributes.push(AttributeEntry {
                    name: attribute_mapping
                        .get(&attribute_idx)
                        .map(ToString::to_string),
                    value: Some(value),
                });
                attribute_idx += 1;
            }
            unknown => unreachable!("{unknown:?}"),
        };
    }
}

fn parse_paragraph_inner(pair: Pair<Rule>, metadata: &mut BlockMetadata) -> Vec<InlineNode> {
    let pairs = pair.into_inner();

    let mut content = Vec::new();
    let mut first = true;

    for pair in pairs {
        let start = pair.as_span().start_pos();
        let end = pair.as_span().end_pos();

        let location = Location {
            start: Position {
                line: start.line_col().0,
                column: start.line_col().1,
            },
            end: Position {
                line: end.line_col().0,
                column: end.line_col().1,
            },
        };

        if first {
            let value = pair.as_str().trim_end().to_string();
            if value.starts_with(' ') {
                metadata.style = Some("literal".to_string());
            }
            first = false;
        }

        match pair.as_rule() {
            Rule::plain_text => content.push(InlineNode::PlainText(PlainText {
                content: pair.as_str().to_string().trim().to_string(),
                location,
            })),
            Rule::inline_line_break => content.push(InlineNode::InlineLineBreak(location)),
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    content
}

fn parse_paragraph(
    pair: Pair<Rule>,
    metadata: &mut BlockMetadata,
    attributes: &mut Vec<AttributeEntry>,
) -> Block {
    let start = pair.as_span().start_pos();
    let end = pair.as_span().end_pos();
    let pairs = pair.into_inner();

    let mut content = Vec::new();
    let mut style_found = false;
    let mut title = None;

    let mut admonition = None;

    let location = Location {
        start: Position {
            line: start.line_col().0,
            column: start.line_col().1,
        },
        end: Position {
            line: end.line_col().0,
            column: end.line_col().1,
        },
    };

    for pair in pairs {
        match pair.as_rule() {
            Rule::admonition => {
                admonition = Some(pair.as_str().to_string());
            }
            Rule::paragraph_inner => {
                content.extend(parse_paragraph_inner(pair, metadata));
            }
            Rule::role => metadata.roles.push(pair.as_str().to_string()),
            Rule::option => metadata.options.push(pair.as_str().to_string()),
            Rule::named_attribute => {
                parse_named_attribute(pair.into_inner(), attributes, metadata);
            }
            Rule::empty_style => {
                style_found = true;
            }
            Rule::positional_attribute_value => {
                let value = pair.as_str().to_string();
                if !value.is_empty() {
                    if metadata.style.is_none() && !style_found {
                        metadata.style = Some(value);
                    } else {
                        attributes.push(AttributeEntry {
                            name: None,
                            value: Some(value),
                        });
                    }
                }
            }
            Rule::title => {
                title = Some(pair.as_str().to_string());
            }
            Rule::EOI | Rule::comment => {}
            unknown => {
                unreachable!("{unknown:?}");
            }
        }
    }
    Block::Paragraph(Paragraph {
        metadata: metadata.clone(),
        attributes: attributes.clone(),
        title,
        content,
        location,
        admonition,
    })
}

fn parse_named_attribute(
    pairs: Pairs<Rule>,
    attributes: &mut Vec<AttributeEntry>,
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
            metadata.roles.push(value.unwrap());
        } else {
            attributes.push(AttributeEntry {
                name: Some(name),
                value,
            });
        }
    }
}

fn parse_list(pairs: Pairs<Rule>) -> Result<Block, Error> {
    let mut title = None;
    let mut metadata = BlockMetadata::default();
    let mut attributes = Vec::new();
    let mut style_found = false;
    let mut block = Block::UnorderedList(UnorderedList {
        title: None,
        metadata: metadata.clone(),
        attributes: attributes.clone(),
        items: Vec::new(),
        location: Location {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        },
    });

    for pair in pairs {
        match pair.as_rule() {
            Rule::list_title | Rule::blocktitle | Rule::title => {
                title = Some(pair.as_str().to_string());
            }
            Rule::unordered_list | Rule::ordered_list => {
                block = parse_simple_list(
                    title.clone(),
                    metadata.clone(),
                    attributes.clone(),
                    pair.into_inner(),
                )?;
            }
            Rule::named_attribute => {
                parse_named_attribute(pair.into_inner(), &mut attributes, &mut metadata);
            }
            Rule::empty_style => {
                style_found = true;
            }
            Rule::positional_attribute_value => {
                let value = pair.as_str().to_string();
                if !value.is_empty() {
                    if metadata.style.is_none() && !style_found {
                        metadata.style = Some(value);
                    } else {
                        attributes.push(AttributeEntry {
                            name: None,
                            value: Some(value),
                        });
                    }
                }
            }
            Rule::role => metadata.roles.push(pair.as_str().to_string()),
            Rule::option => metadata.options.push(pair.as_str().to_string()),
            Rule::description_list => {
                block = parse_description_list(
                    title.clone(),
                    metadata.clone(),
                    attributes.clone(),
                    pair.into_inner(),
                )?;
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(block)
}

fn parse_description_list(
    title: Option<String>,
    metadata: BlockMetadata,
    attributes: Vec<AttributeEntry>,
    pairs: Pairs<Rule>,
) -> Result<Block, Error> {
    let mut location = Location {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    };

    let mut items = Vec::new();

    for pair in pairs {
        let location = Location {
            start: Position {
                line: pair.as_span().start_pos().line_col().0,
                column: pair.as_span().start_pos().line_col().1,
            },
            end: Position {
                line: pair.as_span().end_pos().line_col().0,
                column: pair.as_span().end_pos().line_col().1,
            },
        };
        let mut blocks = Vec::new();
        match pair.as_rule() {
            Rule::description_list_item => {
                let mut anchors = Vec::new();
                let mut term = String::new();
                let mut delimiter = "";
                for inner_pair in pair.clone().into_inner() {
                    let location = location.clone();
                    match inner_pair.as_rule() {
                        Rule::description_list_term => {
                            term = inner_pair.as_str().to_string();
                        }
                        Rule::description_list_term_anchor => {
                            anchors.push(parse_anchor(inner_pair.into_inner()));
                        }
                        Rule::description_list_delimiter => {
                            delimiter = inner_pair.as_str();
                        }
                        Rule::blocks => {
                            let description = parse_blocks(inner_pair.into_inner())?;
                            items.push(DescriptionListItem {
                                anchors: anchors.clone(),
                                term: term.to_string(),
                                delimiter: delimiter.to_string(),
                                description: DescriptionListDescription::Blocks(description),
                                location,
                            });
                        }
                        Rule::description_list_inline => {
                            let description = inner_pair.as_str();
                            items.push(DescriptionListItem {
                                anchors: anchors.clone(),
                                term: term.to_string(),
                                delimiter: delimiter.to_string(),
                                description: DescriptionListDescription::Inline(
                                    description.to_string(),
                                ),
                                location,
                            });
                        }
                        Rule::EOI | Rule::comment => {}
                        _ => {
                            // If we get here, it means we have a block that is not a
                            // description list
                            blocks.push(parse_block(inner_pair.into_inner())?);
                        } //unknown => unreachable!("{unknown:?}"),
                    }
                }
                if !blocks.is_empty() {
                    items.push(DescriptionListItem {
                        anchors,
                        term: term.to_string(),
                        delimiter: delimiter.to_string(),
                        description: DescriptionListDescription::Blocks(blocks),
                        location,
                    });
                }
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }

    location.start = items
        .first()
        .map_or(location.start, |item| item.location.start.clone());
    location.end = items
        .last()
        .map_or(location.end, |item| item.location.end.clone());

    Ok(Block::DescriptionList(DescriptionList {
        title,
        metadata,
        attributes,
        items,
        location,
    }))
}

fn parse_anchor(pairs: Pairs<Rule>) -> Anchor {
    let mut anchor = Anchor::default();
    let len = pairs.clone().count();
    for (i, pair) in pairs.enumerate() {
        if i == 0 {
            anchor.location.start = Position {
                line: pair.as_span().start_pos().line_col().0,
                column: pair.as_span().start_pos().line_col().1,
            };
        }
        if i == len - 1 {
            anchor.location.end = Position {
                line: pair.as_span().end_pos().line_col().0,
                column: pair.as_span().end_pos().line_col().1,
            };
        }
        match pair.as_rule() {
            Rule::id => {
                anchor.id = pair.as_str().to_string();
            }
            Rule::xreflabel => {
                anchor.xreflabel = Some(pair.as_str().to_string());
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    anchor
}

fn parse_simple_list(
    title: Option<String>,
    metadata: BlockMetadata,
    attributes: Vec<AttributeEntry>,
    pairs: Pairs<Rule>,
) -> Result<Block, Error> {
    let mut location = Location {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    };

    let mut items = Vec::new();
    let mut kind = "unordered";

    for pair in pairs {
        let span = pair.as_span();
        if location.start == location.end {
            location = Location {
                start: Position {
                    line: span.start_pos().line_col().0,
                    column: span.start_pos().line_col().1,
                },
                end: Position {
                    line: span.end_pos().line_col().0,
                    column: span.end_pos().line_col().1,
                },
            };
        }

        if span.start_pos().line_col().0 < location.start.line {
            location.start.line = span.start_pos().line_col().0;
        }
        if span.start_pos().line_col().1 < location.start.column {
            location.start.column = span.start_pos().line_col().1;
        }
        location.end.line = span.end_pos().line_col().0;
        location.end.column = span.end_pos().line_col().1;

        match pair.as_rule() {
            Rule::unordered_list_item => {
                items.push(parse_list_item(pair.into_inner())?);
            }
            Rule::ordered_list_item => {
                kind = "ordered";
                items.push(parse_list_item(pair.into_inner())?);
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(match kind {
        "ordered" => Block::OrderedList(OrderedList {
            title,
            metadata,
            attributes,
            items,
            location,
        }),
        _ => Block::UnorderedList(UnorderedList {
            title,
            metadata,
            attributes,
            items,
            location,
        }),
    })
}

fn parse_list_item(pairs: Pairs<Rule>) -> Result<ListItem, Error> {
    let mut content = Vec::new();
    let mut level = 0;
    let mut checked = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::list_item => {
                content.push(pair.as_str().trim().to_string());
            }
            Rule::unordered_level | Rule::ordered_level => {
                level = u8::try_from(pair.as_str().chars().count())
                    .map_err(|e| Error::Parse(format!("error with list level depth: {e}")))?;
            }
            Rule::ordered_level_number => {
                let number_string = pair.as_str();
                level = number_string.parse::<u8>().map_err(|e| {
                    Error::Parse(format!(
                        "error with ordered level number {number_string}: {e}"
                    ))
                })?;
                // TODO(nlopes): implement ordered_level_number
                //
                // Do I need to? Does this make a difference? (Perhaps in providing errors
                // to the user)
            }
            Rule::checklist_item_checked => checked = Some(true),
            Rule::checklist_item_unchecked => checked = Some(false),
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(ListItem {
        level,
        checked,
        content,
    })
}

fn parse_section(pair: &Pair<Rule>) -> Result<Block, Error> {
    let metadata = BlockMetadata::default();
    let attributes = Vec::new();
    let mut title = String::new();
    let mut level = 0;
    let mut content = Vec::new();

    for inner_pair in pair.clone().into_inner() {
        match inner_pair.as_rule() {
            Rule::section_title => {
                title = inner_pair.as_str().to_string();
            }
            Rule::section_level => {
                level = u8::try_from(inner_pair.as_str().chars().count())
                    .map_err(|e| Error::Parse(format!("error with section level depth: {e}")))?
                    - 1;
            }
            Rule::section_content => {
                let inner = inner_pair.clone().into_inner();
                if inner.peek().is_none() {
                    let pairs = InnerPestParser::parse(Rule::document, inner_pair.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    content.extend(parse_blocks(pairs)?);
                } else {
                    for pair in inner {
                        content.extend(parse_blocks(pair.into_inner())?);
                    }
                }
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{:?}", unknown),
        }
    }

    Ok(Block::Section(Section {
        metadata,
        attributes,
        title,
        level,
        content,
        location: Location {
            start: Position {
                line: pair.as_span().start_pos().line_col().0,
                column: pair.as_span().start_pos().line_col().1,
            },
            end: Position {
                line: pair.as_span().end_pos().line_col().0,
                column: pair.as_span().end_pos().line_col().1,
            },
        },
    }))
}

// Validate that the block level is correct for the section level.
//
// For example, a section level 1 should only contain blocks of level 2 or higher.
fn validate_section_block_level(content: &[Block], prior_level: Option<u8>) -> Result<(), Error> {
    let mut prior_level = prior_level;
    for (i, block) in content.iter().enumerate() {
        if let Block::Section(section) = block {
            if let Some(Block::Section(next_section)) = content.get(i + 1) {
                if next_section.level > section.level + 1 {
                    let error_detail = ErrorDetail {
                        location: next_section.location.clone(),
                    };
                    return Err(Error::NestedSectionLevelMismatch(
                        error_detail,
                        section.level,
                        section.level + 1,
                    ));
                }
            }
            if let Some(parent_level) = prior_level {
                if section.level == parent_level + 1 {
                    prior_level = Some(section.level);
                    return validate_section_block_level(&section.content, prior_level);
                }
                let error_detail = ErrorDetail {
                    location: section.location.clone(),
                };
                return Err(Error::NestedSectionLevelMismatch(
                    error_detail,
                    section.level,
                    parent_level + 1,
                ));
            }
            prior_level = Some(section.level);
            return validate_section_block_level(&section.content, prior_level);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn parse_delimited_block(pairs: Pairs<Rule>) -> Result<Block, Error> {
    let mut inner = DelimitedBlockType::DelimitedComment(String::new());
    let mut metadata = BlockMetadata::default();
    let mut title = None;
    let mut attributes = Vec::new();
    let mut location = Location {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    };
    let mut style_found = false;

    for pair in pairs {
        if pair.as_rule() == Rule::EOI || pair.as_rule() == Rule::comment {
            continue;
        }
        if location.start.line == 0
            && location.start.column == 0
            && location.end.line == 0
            && location.end.column == 0
        {
            location.start.line = pair.as_span().start_pos().line_col().0;
            location.start.column = pair.as_span().start_pos().line_col().1;
            location.end.line = pair.as_span().end_pos().line_col().0;
            location.end.column = pair.as_span().end_pos().line_col().1;
        }
        if pair.as_span().start_pos().line_col().0 < location.start.line {
            location.start.line = pair.as_span().start_pos().line_col().0;
        }
        if pair.as_span().start_pos().line_col().1 < location.start.column {
            location.start.column = pair.as_span().start_pos().line_col().1;
        }
        location.end.line = pair.as_span().end_pos().line_col().0;
        location.end.column = pair.as_span().end_pos().line_col().1;

        match pair.as_rule() {
            Rule::delimited_comment => {
                inner =
                    DelimitedBlockType::DelimitedComment(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_example => {
                let mut text = pair.into_inner().as_str().to_string();
                text.push('\n');
                let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                    .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                inner = DelimitedBlockType::DelimitedExample(parse_blocks(pairs)?);
            }
            Rule::delimited_pass => {
                inner = DelimitedBlockType::DelimitedPass(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_quote => {
                let mut text = pair.into_inner().as_str().to_string();
                text.push('\n');
                let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                    .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                inner = DelimitedBlockType::DelimitedQuote(parse_blocks(pairs)?);
            }
            Rule::delimited_listing => {
                inner =
                    DelimitedBlockType::DelimitedListing(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_literal => {
                inner =
                    DelimitedBlockType::DelimitedLiteral(pair.into_inner().as_str().to_string());
            }
            Rule::delimited_open => {
                let mut text = pair.into_inner().as_str().to_string();
                text.push('\n');
                let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                    .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                inner = DelimitedBlockType::DelimitedOpen(parse_blocks(pairs)?);
            }
            Rule::delimited_sidebar => {
                let mut text = pair.into_inner().as_str().to_string();
                text.push('\n');
                let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                    .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                inner = DelimitedBlockType::DelimitedSidebar(parse_blocks(pairs)?);
            }
            Rule::delimited_table => {
                inner = DelimitedBlockType::DelimitedTable(pair.into_inner().as_str().to_string());
            }
            Rule::title => {
                title = Some(pair.as_str().to_string());
            }
            Rule::empty_style => {
                style_found = true;
            }
            Rule::positional_attribute_value => {
                let value = pair.as_str().to_string();
                if !value.is_empty() {
                    if metadata.style.is_none() && !style_found {
                        metadata.style = Some(value);
                    } else {
                        attributes.push(AttributeEntry {
                            name: None,
                            value: Some(value),
                        });
                    }
                }
            }
            Rule::named_attribute => {
                parse_named_attribute(pair.into_inner(), &mut attributes, &mut metadata);
            }
            Rule::option => metadata.options.push(pair.as_str().to_string()),
            Rule::anchor => {
                let anchor = parse_anchor(pair.into_inner());
                metadata.id = Some(anchor.clone());
                metadata.anchors.push(anchor);
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }

    Ok(Block::DelimitedBlock(DelimitedBlock {
        metadata,
        inner,
        title,
        attributes,
        location,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Parser;
    use pretty_assertions::assert_eq;

    #[rstest::rstest]
    #[trace]
    fn for_each_file(#[files("fixtures/tests/**/*.adoc")] path: std::path::PathBuf) {
        let parser = PestParser;
        let test_file_path = path.with_extension("test");

        // We do this check because we have files that won't have a test file, namely ones
        // that are supposed to error out!
        if test_file_path.exists() {
            let result = parser
                .parse(&std::fs::read_to_string(&path).unwrap())
                .unwrap();
            let test: Document =
                serde_json::from_str(&std::fs::read_to_string(test_file_path).unwrap()).unwrap();
            assert_eq!(test, result);
        } else {
            tracing::warn!("no test file found for {:?}", path);
        }
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_section_with_invalid_subsection() {
        let parser = PestParser;
        let result = parser
            .parse(include_str!(
                "../fixtures/tests/section_with_invalid_subsection.adoc"
            ))
            .unwrap_err();
        if let Error::NestedSectionLevelMismatch(ref detail, 2, 3) = result {
            assert_eq!(
                &ErrorDetail {
                    location: Location {
                        start: Position { line: 3, column: 1 },
                        end: Position { line: 5, column: 1 }
                    }
                },
                detail
            );
        } else {
            panic!("unexpected error: {result:?}");
        }
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_book() {
        let result = PestParser
            .parse_file("fixtures/samples/book-starter/index.adoc")
            .unwrap();
    }

    // #[test]
    // #[tracing_test::traced_test]
    // fn test_sample1() {
    //     let result = PestParser
    //         .parse_file("fixtures/samples/sample1/index.adoc")
    //         .unwrap();
    //     dbg!(&result);
    //     panic!()
    // }

    // #[test]
    // fn test_stuff() {
    //     let result = PestParser.parse("NOTE: This is a note.").unwrap();
    //     dbg!(&result);
    //     panic!()
    // }

    //     #[test]
    //     fn test_blah() {
    //         let result = PestParser
    //             .parse(
    //                 "[[cpu,CPU]]Central Processing Unit (CPU)::
    // The brain of the computer.

    // [[hard-drive]]Hard drive::
    // Permanent storage for operating system and/or user files.",
    //             )
    //             .unwrap();
    //         panic!()
    //     }

    // #[test]
    // fn test_mdbasics_adoc() {
    //     let result = PestParser
    //         .parse(include_str!("../fixtures/samples/mdbasics/mdbasics.adoc"))
    //         .unwrap();
    //     dbg!(&result);
    //     panic!()
    // }
}
