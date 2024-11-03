use std::collections::HashMap;

use pest::{
    iterators::{Pair, Pairs},
    Parser as _,
};
use tracing::instrument;

use crate::{
    model::{AttributeName, Block, BlockMetadata, InlineNode, Location, Paragraph, Position},
    Error, Rule,
};

impl Paragraph {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pair: Pair<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut HashMap<AttributeName, Option<String>>,
    ) -> Result<Block, Error> {
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
                    content.extend(Self::parse_inner(pair, metadata)?);
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
                            attributes.insert(value, None);
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
        Ok(Block::Paragraph(Self {
            metadata: metadata.clone(),
            attributes: attributes.clone(),
            title,
            content,
            location,
            admonition,
        }))
    }

    // TODO(nlopes): we probably need to offset the location so that it starts at whatever
    // offset we provide - that's because we call this recursively
    #[instrument(level = "trace")]
    pub(crate) fn parse_inner(
        pair: Pair<Rule>,
        metadata: &mut BlockMetadata,
    ) -> Result<Vec<InlineNode>, Error> {
        let pairs = pair.into_inner();

        let mut content = Vec::new();
        let mut first = true;

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
                Rule::non_plain_text => {
                    content.push(InlineNode::parse(pair.into_inner(), metadata)?);
                }
                Rule::plain_text => {
                    content.push(InlineNode::parse(Pairs::single(pair), metadata)?);
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Ok(content)
    }

    #[instrument(level = "trace")]
    pub(crate) fn get_content(
        text_style: &str,
        unconstrained: bool,
        pair: &Pair<Rule>,
        metadata: &mut BlockMetadata,
    ) -> Result<Vec<InlineNode>, Error> {
        let mut content = Vec::new();
        let len = pair.as_str().len();
        let token_length = if unconstrained { 2 } else { 1 };
        match crate::InnerPestParser::parse(
            Rule::paragraph_inner,
            &pair.as_str()[token_length..len - token_length],
        ) {
            Ok(pairs) => {
                for pair in pairs {
                    content.extend(Self::parse_inner(pair, metadata)?);
                }
            }
            Err(e) => {
                tracing::error!(text_style, "error parsing text: {e}");
                return Err(Error::Parse(e.to_string()));
            }
        }
        Ok(content)
    }
}
