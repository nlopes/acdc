mod description;
mod item;
mod simple;

use pest::{Parser as _, iterators::Pairs};

use crate::{
    AttributeValue, Block, BlockMetadata, DescriptionList, DocumentAttributes, ElementAttributes,
    Error, InlinePreprocessorParserState, InnerPestParser, Location, Options, Rule, UnorderedList,
    inline_preprocessing, inlines::parse_inlines,
};

use super::block::BlockExt;

pub(crate) fn parse_list(
    pairs: Pairs<Rule>,
    options: &Options,
    parent_location: Option<&Location>,
    parent_attributes: &mut DocumentAttributes,
) -> Result<Block, Error> {
    let mut title = Vec::new();
    let mut metadata = BlockMetadata::default();
    let mut attributes = ElementAttributes::default();
    let mut style_found = false;
    let mut block = Block::UnorderedList(UnorderedList {
        title: Vec::new(),
        metadata: metadata.clone(),
        items: Vec::new(),
        marker: String::new(),
        location: Location::default(),
    });

    for pair in pairs {
        match pair.as_rule() {
            Rule::list_title | Rule::blocktitle | Rule::title => {
                let text = pair.as_str();
                let start_pos = pair.as_span().start_pos().pos();
                let mut location = Location::from_pair(&pair);
                location.shift(parent_location);

                // Run inline preprocessor before parsing inlines
                let mut state = InlinePreprocessorParserState::new();
                state.set_initial_position(&location, start_pos);
                let processed = inline_preprocessing::run(text, parent_attributes, &state)
                    .map_err(|e| {
                        tracing::error!("error processing list title: {}", e);
                        Error::Parse(e.to_string())
                    })?;

                let mut pairs = InnerPestParser::parse(Rule::inlines, &processed.text)
                    .map_err(|e| Error::Parse(e.to_string()))?;

                title = parse_inlines(
                    pairs.next().ok_or_else(|| {
                        tracing::error!("error parsing list title");
                        Error::Parse("error parsing list title".to_string())
                    })?,
                    Some(&processed),
                    Some(&location),
                    parent_attributes,
                )?;
            }
            Rule::unordered_list | Rule::ordered_list => {
                block = Block::parse_simple_list(
                    pair.into_inner(),
                    title.clone(),
                    metadata.clone(),
                    attributes.clone(),
                    parent_location,
                    parent_attributes,
                )?;
            }
            Rule::named_attribute => {
                Block::parse_named_attribute(pair.into_inner(), &mut attributes, &mut metadata);
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
                        attributes.insert(value, AttributeValue::None);
                    }
                }
            }
            Rule::role => metadata.roles.push(pair.as_str().to_string()),
            Rule::option => metadata.options.push(pair.as_str().to_string()),
            Rule::description_list => {
                block = DescriptionList::parse(
                    pair.into_inner(),
                    options,
                    title.clone(),
                    metadata.clone(),
                    parent_location,
                    parent_attributes,
                )?;
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    block.set_attributes(attributes);
    Ok(block)
}
