use std::{collections::HashMap, str::FromStr};

use pest::{
    iterators::{Pair, Pairs},
    Parser,
};
use tracing::instrument;

use crate::{
    inlines::parse_inlines, Admonition, AdmonitionVariant, Anchor, AttributeValue, Block,
    BlockMetadata, DocumentAttributes, ElementAttributes, Error, InlineNode, InlinePreprocessor,
    InnerPestParser, Location, Paragraph, ProcessedContent, Rule,
};

impl Paragraph {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pair: Pair<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut ElementAttributes,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut outer_location = Location::from_pair(&pair);
        let pairs = pair.into_inner();

        let mut content = Vec::new();
        let mut style_found = false;
        let mut title = Vec::new();
        let mut admonition = None;

        for pair in pairs {
            let mut location = Location::from_pair(&pair);
            match pair.as_rule() {
                Rule::admonition => {
                    admonition = Some(pair.as_str());
                }
                Rule::inlines => {
                    let text = pair.as_str();
                    let start_pos = pair.as_span().start_pos().pos();

                    // Run inline preprocessor before parsing inlines
                    let mut preprocessor = InlinePreprocessor::new(parent_attributes.clone());
                    let processed = preprocessor.process(text, start_pos)?;
                    // Now parse the processed text
                    let mut pairs = InnerPestParser::parse(Rule::inlines, &processed.text)
                        .map_err(|e| Error::Parse(e.to_string()))?;

                    // We need to shift the location of the inlines so that they are
                    // correct.
                    location.shift(parent_location);
                    // TODO(nlopes): we should merge the parent_attributes, with the
                    // attributes we have here?!?
                    content.extend(Self::parse_inner(
                        pairs.next().ok_or_else(|| {
                            tracing::error!("error parsing paragraph content");
                            Error::Parse("error parsing paragraph content".to_string())
                        })?,
                        metadata,
                        &processed,
                        Some(&location),
                        parent_attributes,
                    )?);
                }
                Rule::role => metadata.roles.push(pair.as_str().to_string()),
                Rule::option => metadata.options.push(pair.as_str().to_string()),
                Rule::named_attribute => {
                    Block::parse_named_attribute(pair.into_inner(), attributes, metadata);
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
                Rule::title => {
                    // TODO(nlopes): insted of None, `processed` should be passed here
                    //
                    // In order to do that, we need to pre-process the title and then
                    // pass it to `parse_inlines` as `Some(processed)`
                    title = parse_inlines(pair, None, parent_location, parent_attributes)?;
                }
                Rule::EOI | Rule::comment | Rule::open_sb | Rule::close_sb => {}
                unknown => {
                    unreachable!("{unknown:?}");
                }
            }
        }
        outer_location.shift(parent_location);
        if let Some(admonition) = admonition {
            Ok(Block::Admonition(Admonition {
                metadata: metadata.clone(),
                title,
                blocks: vec![Block::Paragraph(Self {
                    metadata: metadata.clone(),
                    title: Vec::new(),
                    content,
                    location: outer_location.clone(),
                })],
                location: outer_location.clone(),
                variant: AdmonitionVariant::from_str(admonition)?,
            }))
        } else {
            Ok(Block::Paragraph(Self {
                metadata: metadata.clone(),
                title,
                content,
                location: outer_location.clone(),
            }))
        }
    }

    #[instrument(level = "trace")]
    pub(crate) fn parse_inner(
        pair: Pair<Rule>,
        metadata: &mut BlockMetadata,
        processed: &ProcessedContent,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Vec<InlineNode>, Error> {
        let pairs = pair.into_inner();
        let mut attributes = HashMap::new();

        let mut content = Vec::new();
        let mut first = true;

        // We need to do this because the inlines locations below will calculate their
        // lines by assuming there is already a newline but in this specific case
        // (paragraph) there isn't.
        // let parent_location = parent_location.map(|l| {
        //     let mut l = l.clone();
        //     l.start.line += 1;
        //     l.end.line += 1;
        //     l
        // });

        for pair in pairs {
            if first {
                // Remove the trailing newline if there is one.
                let value = pair.as_str().to_string();
                if value.starts_with(' ') {
                    metadata.style = Some("literal".to_string());
                }
                first = false;
            }

            match pair.as_rule() {
                Rule::option => metadata.options.push(pair.as_str().to_string()),
                Rule::role => metadata.roles.push(pair.as_str().to_string()),
                Rule::id | Rule::block_style_id => {
                    if metadata.id.is_some() {
                        tracing::warn!(
                            id = pair.as_str(),
                            "block already has an id, ignoring this one"
                        );
                        continue;
                    }
                    let mut location = Location::from_pair(&pair);
                    location.shift(parent_location);
                    let anchor = Anchor {
                        id: pair.as_str().to_string(),
                        location,
                        ..Default::default()
                    };
                    metadata.anchors.push(anchor.clone());
                    metadata.id = Some(anchor);
                }
                Rule::positional_attribute_value => {
                    let value = pair.as_str().to_string();
                    if !value.is_empty() {
                        if metadata.style.is_none() {
                            metadata.style = Some(value);
                        } else {
                            attributes.insert(value, AttributeValue::None);
                        }
                    }
                }
                Rule::non_plain_text => {
                    content.push(InlineNode::parse(
                        pair.into_inner(),
                        Some(processed),
                        parent_location,
                        parent_attributes,
                    )?);
                }
                Rule::plain_text => {
                    content.push(InlineNode::parse(
                        Pairs::single(pair),
                        Some(processed),
                        parent_location,
                        parent_attributes,
                    )?);
                }
                Rule::EOI | Rule::comment | Rule::open_sb | Rule::close_sb => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Ok(content)
    }
}
