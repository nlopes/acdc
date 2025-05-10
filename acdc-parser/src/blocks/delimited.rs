use pest::{Parser as _, iterators::Pairs};

use crate::{
    Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, DocumentAttributes,
    ElementAttributes, Error, InlineNode, InnerPestParser, Location, Options, Plain, Raw, Rule,
    Table, blocks,
};

impl DelimitedBlock {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        options: &Options,
        title: Vec<InlineNode>,
        metadata: &BlockMetadata,
        attributes: &ElementAttributes,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut inner = DelimitedBlockType::DelimitedComment(Vec::new());
        let mut delimiter = String::new();
        let mut location = Location::default();

        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.set_start_from_pos(&pair.as_span().start_pos());
                //location.shift_start(parent_location);
            }
            if i == len - 1 {
                location.set_end_from_pos(&pair.as_span().end_pos());
                //location.shift_end(parent_location);
            }
            let rule = pair.as_rule();
            if rule == Rule::EOI || rule == Rule::comment {
                continue;
            }

            // TODO(nlopes): these are 2 blocks that are very similar, we should refactor
            // them
            let pair = if rule == Rule::delimited_table {
                // TODO(nlopes): must fix this - we're not extracting the delimiter so we
                // need to change this section.
                let mut pair_inner = pair.into_inner();
                let delimiter_pair = pair_inner.next().ok_or_else(|| {
                    Error::Parse(String::from("delimited block must have a delimiter"))
                })?;
                delimiter = delimiter_pair.as_str().to_string();
                pair_inner.next().ok_or_else(|| {
                    Error::Parse(String::from("delimited block must have content"))
                })?
            } else {
                let mut pair_inner = pair.into_inner();
                let delimiter_pair = pair_inner.next().ok_or_else(|| {
                    Error::Parse(String::from("delimited block must have a delimiter"))
                })?;
                delimiter = delimiter_pair.as_str().to_string();
                pair_inner.next().ok_or_else(|| {
                    Error::Parse(String::from("delimited block must have content"))
                })?
            };

            let mut inner_location = Location::from_pair(&pair);
            let text = pair.as_str().to_string();
            inner_location.shift(parent_location);

            match rule {
                Rule::delimited_comment => {
                    // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                    inner =
                        DelimitedBlockType::DelimitedComment(vec![InlineNode::PlainText(Plain {
                            location: inner_location.clone(),
                            content: text.clone(),
                        })]);
                }
                Rule::delimited_example => {
                    let pairs = InnerPestParser::parse(Rule::blocks, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedExample(blocks::parse(
                        pairs,
                        options,
                        Some(&location),
                        parent_attributes,
                    )?);
                }
                Rule::delimited_pass => {
                    // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                    inner = DelimitedBlockType::DelimitedPass(vec![InlineNode::RawText(Raw {
                        location: inner_location.clone(),
                        content: text.clone(),
                    })]);
                }
                Rule::delimited_quote => {
                    if let Some(ref verse) = metadata.style {
                        if verse == "verse" {
                            // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                            inner =
                                DelimitedBlockType::DelimitedVerse(vec![InlineNode::PlainText(
                                    Plain {
                                        location: location.clone(),
                                        content: text.clone(),
                                    },
                                )]);
                            continue;
                        }
                    }
                    let pairs = InnerPestParser::parse(Rule::blocks, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedQuote(blocks::parse(
                        pairs,
                        options,
                        Some(&location),
                        parent_attributes,
                    )?);
                }
                Rule::delimited_listing => {
                    // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                    inner =
                        DelimitedBlockType::DelimitedListing(vec![InlineNode::PlainText(Plain {
                            location: inner_location.clone(),
                            content: text.clone(),
                        })]);
                }
                Rule::delimited_literal => {
                    // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                    inner =
                        DelimitedBlockType::DelimitedLiteral(vec![InlineNode::PlainText(Plain {
                            location: inner_location.clone(),
                            content: text.clone(),
                        })]);
                }
                Rule::delimited_open => {
                    let pairs = InnerPestParser::parse(Rule::blocks, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedOpen(blocks::parse(
                        pairs,
                        options,
                        Some(&location),
                        parent_attributes,
                    )?);
                }
                Rule::delimited_sidebar => {
                    // Adjust one line here for the start of the delimiter
                    //location.start.line += 1;
                    let pairs = InnerPestParser::parse(Rule::blocks, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedSidebar(blocks::parse(
                        pairs,
                        options,
                        Some(&location),
                        parent_attributes,
                    )?);
                }
                Rule::delimited_table => {
                    inner = DelimitedBlockType::DelimitedTable(Table::parse(
                        &pair,
                        options,
                        metadata,
                        attributes,
                        parent_attributes,
                    )?);
                }
                unknown => unreachable!("{unknown:?}"),
            }
        }

        Ok(Block::DelimitedBlock(DelimitedBlock {
            metadata: metadata.clone(),
            delimiter,
            inner,
            title,
            location,
        }))
    }
}
