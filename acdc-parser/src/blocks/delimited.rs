use std::collections::HashMap;

use pest::{iterators::Pairs, Parser as _};

use crate::{
    blocks,
    model::{Anchor, Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, Location},
    Error, InnerPestParser, Rule,
};

impl DelimitedBlock {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse(pairs: Pairs<Rule>) -> Result<Block, Error> {
        let mut inner = DelimitedBlockType::DelimitedComment(String::new());
        let mut metadata = BlockMetadata::default();
        let mut title = None;
        let mut attributes = HashMap::new();
        let mut location = Location::default();
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
                    inner = DelimitedBlockType::DelimitedComment(
                        pair.into_inner().as_str().to_string(),
                    );
                }
                Rule::delimited_example => {
                    let mut text = pair.into_inner().as_str().to_string();
                    text.push('\n');
                    let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedExample(blocks::parse(pairs)?);
                }
                Rule::delimited_pass => {
                    inner =
                        DelimitedBlockType::DelimitedPass(pair.into_inner().as_str().to_string());
                }
                Rule::delimited_quote => {
                    let mut text = pair.into_inner().as_str().to_string();
                    text.push('\n');
                    let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedQuote(blocks::parse(pairs)?);
                }
                Rule::delimited_listing => {
                    inner = DelimitedBlockType::DelimitedListing(
                        pair.into_inner().as_str().to_string(),
                    );
                }
                Rule::delimited_literal => {
                    inner = DelimitedBlockType::DelimitedLiteral(
                        pair.into_inner().as_str().to_string(),
                    );
                }
                Rule::delimited_open => {
                    let mut text = pair.into_inner().as_str().to_string();
                    text.push('\n');
                    let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedOpen(blocks::parse(pairs)?);
                }
                Rule::delimited_sidebar => {
                    let mut text = pair.into_inner().as_str().to_string();
                    text.push('\n');
                    let pairs = InnerPestParser::parse(Rule::document, text.as_str())
                        .map_err(|e| Error::Parse(format!("error parsing section content: {e}")))?;
                    inner = DelimitedBlockType::DelimitedSidebar(blocks::parse(pairs)?);
                }
                Rule::delimited_table => {
                    inner =
                        DelimitedBlockType::DelimitedTable(pair.into_inner().as_str().to_string());
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
                            attributes.insert(value, None);
                        }
                    }
                }
                Rule::named_attribute => {
                    Block::parse_named_attribute(pair.into_inner(), &mut attributes, &mut metadata);
                }
                Rule::option => metadata.options.push(pair.as_str().to_string()),
                Rule::anchor => {
                    let anchor = Anchor::parse(pair.into_inner());
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
}
