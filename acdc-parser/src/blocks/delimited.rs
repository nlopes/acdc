use std::collections::HashMap;

use acdc_core::{AttributeName, DocumentAttributes, Location};
use pest::{iterators::Pairs, Parser as _};

use crate::{
    blocks,
    model::{
        Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, InlineNode,
        OptionalAttributeValue, Plain, Table,
    },
    Error, InnerPestParser, Rule,
};

impl DelimitedBlock {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        title: Vec<InlineNode>,
        metadata: &BlockMetadata,
        attributes: &HashMap<AttributeName, OptionalAttributeValue>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut inner = DelimitedBlockType::DelimitedComment(Vec::new());
        let mut location = Location::default();

        for pair in pairs {
            let rule = pair.as_rule();
            if rule == Rule::EOI || rule == Rule::comment {
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

            let pair = if rule == Rule::delimited_table {
                pair
            } else {
                pair.into_inner().next().ok_or_else(|| {
                    Error::Parse(String::from("delimited block must have content"))
                })?
            };

            let (start_line, start_column) = pair.as_span().start_pos().line_col();
            let (end_line, end_column) = pair.as_span().end_pos().line_col();
            location.start.line = start_line;
            location.start.column = start_column;
            location.end.line = end_line;
            location.end.column = end_column;
            let text = pair.as_str().to_string();

            match rule {
                Rule::delimited_comment => {
                    // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                    inner =
                        DelimitedBlockType::DelimitedComment(vec![InlineNode::PlainText(Plain {
                            location: location.clone(),
                            content: text.clone(),
                        })]);
                }
                Rule::delimited_example => {
                    let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedExample(blocks::parse(
                        pairs,
                        parent_attributes,
                    )?);
                }
                Rule::delimited_pass => {
                    // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                    inner = DelimitedBlockType::DelimitedPass(vec![InlineNode::PlainText(Plain {
                        location: location.clone(),
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
                    let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedQuote(blocks::parse(
                        pairs,
                        parent_attributes,
                    )?);
                }
                Rule::delimited_listing => {
                    // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                    inner =
                        DelimitedBlockType::DelimitedListing(vec![InlineNode::PlainText(Plain {
                            location: location.clone(),
                            content: text.clone(),
                        })]);
                }
                Rule::delimited_literal => {
                    // IMPORTANT(nlopes): this assumes only one string in the verse, I'm not 100% sure this is a fact.
                    inner =
                        DelimitedBlockType::DelimitedLiteral(vec![InlineNode::PlainText(Plain {
                            location: location.clone(),
                            content: text.clone(),
                        })]);
                }
                Rule::delimited_open => {
                    let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner =
                        DelimitedBlockType::DelimitedOpen(blocks::parse(pairs, parent_attributes)?);
                }
                Rule::delimited_sidebar => {
                    let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedSidebar(blocks::parse(
                        pairs,
                        parent_attributes,
                    )?);
                }
                Rule::delimited_table => {
                    inner = DelimitedBlockType::DelimitedTable(Table::parse(
                        &pair.into_inner(),
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
            inner,
            title,
            location,
        }))
    }
}
