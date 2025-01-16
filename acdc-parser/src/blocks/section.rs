use pest::{iterators::Pair, Parser as _};

use crate::{
    inlines::parse_inlines, model::DiscreteHeaderSection, Anchor, AttributeValue, Block,
    BlockMetadata, DocumentAttributes, ElementAttributes, Error, InlinePreprocessor,
    InnerPestParser, Location, Rule, Section,
};

// TODO(nlopes): this might be parser as part of the inner content of a delimited block
// due to how I've been parsing. (i.e: our pest grammar is not entirely correct)
//
// What typically happens is that we parse a section but it is instead a discrete header
// with other content following.
impl Section {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse(
        pair: &Pair<Rule>,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut anchors = Vec::new();
        let mut metadata = BlockMetadata::default();
        let mut attributes = ElementAttributes::default();
        let mut style_found = false;
        let mut title = Vec::new();
        let mut level = 0;
        let mut discrete = false;
        let mut content = Vec::new();

        let mut location = Location::from_pair(pair);
        location.shift(parent_location);

        for inner_pair in pair.clone().into_inner() {
            match inner_pair.as_rule() {
                Rule::section_title => {
                    let mut inner_location = Location::from_pair(&inner_pair);
                    inner_location.shift(parent_location);
                    let mut preprocessor = InlinePreprocessor::new(parent_attributes.clone());
                    let processed =
                        preprocessor.process(inner_pair.as_str(), inner_pair.as_span().start())?;

                    let mut pairs = InnerPestParser::parse(Rule::inlines, &processed.text)
                        .map_err(|e| Error::Parse(e.to_string()))?;

                    title = parse_inlines(
                        pairs.next().ok_or_else(|| {
                            tracing::error!("error parsing section title");
                            Error::Parse("error parsing section title".to_string())
                        })?,
                        Some(&processed),
                        Some(&inner_location),
                        parent_attributes,
                    )?;
                    if discrete {
                        if let Some(last) = title.last() {
                            location.end = last.location().end.clone();
                        }
                    }
                }
                Rule::section_level => {
                    level = u8::try_from(inner_pair.as_str().chars().count()).map_err(|e| {
                        Error::Parse(format!("error with section level depth: {e}"))
                    })? - 1;
                }
                Rule::section_content => {
                    for pair in inner_pair.clone().into_inner() {
                        match pair.as_rule() {
                            Rule::section => {
                                content.push(Section::parse(
                                    &pair,
                                    parent_location,
                                    parent_attributes,
                                )?);
                            }
                            Rule::block => {
                                content.push(Block::parse(
                                    pair.into_inner(),
                                    parent_location,
                                    parent_attributes,
                                )?);
                            }
                            Rule::EOI | Rule::comment => {}
                            unknown => unreachable!("{:?}", unknown),
                        }
                    }
                }
                Rule::positional_attribute_value => {
                    let value = inner_pair.as_str().to_string();
                    if !value.is_empty() {
                        if value == "discrete" {
                            discrete = true;
                        }
                        // if we have a positional attribute and it is the first one, then
                        // it's the style
                        if metadata.style.is_none() && !style_found {
                            metadata.style = Some(value);
                            style_found = true;
                        } else {
                            attributes.insert(value, AttributeValue::None);
                        }
                    }
                }
                Rule::named_attribute => {
                    Block::parse_named_attribute(
                        pair.clone().into_inner(),
                        &mut attributes,
                        &mut metadata,
                    );
                }
                Rule::anchor => anchors.push(Anchor::parse(pair.clone().into_inner())),

                Rule::EOI | Rule::comment | Rule::open_sb | Rule::close_sb => {}
                unknown => unreachable!("{:?}", unknown),
            }
        }

        if discrete {
            #[allow(clippy::used_underscore_items)]
            return Ok(Block::_DiscreteHeaderSection(DiscreteHeaderSection {
                anchors,
                title,
                level,
                location,
                content,
            }));
        }
        Ok(Block::Section(Self {
            metadata,
            title,
            level,
            content,
            location,
        }))
    }
}
