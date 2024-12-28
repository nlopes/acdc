use pest::iterators::Pairs;

use crate::{
    blocks, Anchor, Block, BlockMetadata, DescriptionList, DescriptionListDescription,
    DescriptionListItem, DocumentAttributes, Error, InlineNode, Location, Rule,
};

impl DescriptionList {
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        title: Vec<InlineNode>,
        metadata: BlockMetadata,
        _parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut location = Location::default();
        // TODO(nlopes): handle parent_location
        let mut items = Vec::new();

        for pair in pairs {
            let location = Location::from_pair(&pair);
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
                                anchors.push(Anchor::parse(inner_pair.into_inner()));
                            }
                            Rule::description_list_delimiter => {
                                delimiter = inner_pair.as_str();
                            }
                            Rule::blocks => {
                                let description = blocks::parse(
                                    inner_pair.into_inner(),
                                    Some(&location),
                                    parent_attributes,
                                )?;
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
                                blocks.push(Block::parse(
                                    inner_pair.into_inner(),
                                    Some(&location),
                                    parent_attributes,
                                )?);
                            }
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

        Ok(Block::DescriptionList(Self {
            title,
            metadata,
            items,
            location,
        }))
    }
}
