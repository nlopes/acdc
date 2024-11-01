use std::collections::HashMap;

use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    model::{
        AttributeName, AttributeValue, Author, DocumentAttribute, Header, Location, Position, Title,
    },
    Rule,
};

impl Header {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        attributes: &mut HashMap<AttributeName, AttributeValue>,
    ) -> Self {
        let mut title = None;
        let mut subtitle = None;
        let mut authors = Vec::new();
        let mut location = Location::default();

        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.start = Position {
                    line: pair.as_span().start_pos().line_col().0,
                    column: pair.as_span().start_pos().line_col().1,
                };
            }
            location.end = Position {
                line: pair.as_span().end_pos().line_col().0,
                column: pair.as_span().end_pos().line_col().1,
            };
            match pair.as_rule() {
                Rule::document_title_token => {
                    for inner_pair in pair.into_inner() {
                        match inner_pair.as_rule() {
                            Rule::document_title => {
                                let mut title_content = inner_pair.as_str().to_string();
                                // find the subtitle by looking for the last colon in title
                                // andsetting title to everything before the last colon and
                                // subtitle to everything after the last colon
                                if let Some(colon_index) = title_content.rfind(':') {
                                    subtitle =
                                        Some(title_content[colon_index + 1..].trim().to_string());
                                    title_content = title_content[..colon_index].trim().to_string();
                                }
                                title = Some(Title {
                                    name: "text".to_string(),
                                    r#type: "string".to_string(),
                                    title: title_content.clone(),
                                    location: Location {
                                        start: Position {
                                            line: inner_pair.as_span().start_pos().line_col().0,
                                            column: inner_pair.as_span().start_pos().line_col().1,
                                        },
                                        end: Position {
                                            line: inner_pair.as_span().end_pos().line_col().0,
                                            column: inner_pair.as_span().end_pos().line_col().1,
                                        },
                                    },
                                });
                            }
                            unknown => unreachable!("{:?}", unknown),
                        }
                    }
                }
                Rule::author => {
                    let author = Author::parse(pair.into_inner());
                    authors.push(author);
                }
                Rule::revision_line => {
                    let inner_pairs = pair.into_inner();
                    for pair in inner_pairs {
                        match pair.as_rule() {
                            Rule::revision_number => {
                                attributes.insert(
                                    "revnumber".to_string(),
                                    AttributeValue::String(pair.as_str().to_string()),
                                );
                            }
                            Rule::revision_date => {
                                attributes.insert(
                                    "revdate".to_string(),
                                    AttributeValue::String(pair.as_str().to_string()),
                                );
                            }
                            Rule::revision_remark => {
                                attributes.insert(
                                    "revremark".to_string(),
                                    AttributeValue::String(pair.as_str().to_string()),
                                );
                            }
                            unknown => unreachable!("{:?}", unknown),
                        }
                    }
                }
                Rule::document_attribute => {
                    let (name, value) = DocumentAttribute::parse(pair.into_inner());
                    attributes.insert(name, value);
                }
                unknown => unreachable!("{:?}", unknown),
            }
        }

        Self {
            title,
            subtitle,
            authors,
            location,
        }
    }
}
