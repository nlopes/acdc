use pest::{iterators::Pairs, Parser as _};
use tracing::instrument;

use crate::{
    inline_preprocessing, inlines::parse_inlines, AttributeValue, Author, DocumentAttribute,
    DocumentAttributes, Error, Header, InlineNode, InnerPestParser, Location, ParserState, Plain,
    Rule,
};

impl Header {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Option<Self>, Error> {
        let mut title = Vec::new();
        let mut subtitle = None;
        let mut authors = Vec::new();
        let mut location = Location::default();

        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.set_start_from_pos(&pair.as_span().start_pos());
            }
            if i == len - 1 {
                location.set_end_from_pos(&pair.as_span().end_pos());
            }
            match pair.as_rule() {
                Rule::document_title_token => {
                    for inner_pair in pair.into_inner() {
                        match inner_pair.as_rule() {
                            Rule::document_title => {
                                let title_content = inner_pair.as_str().to_string();
                                // find the subtitle by looking for the last colon in title
                                // andsetting title to everything before the last colon and
                                // subtitle to everything after the last colon
                                if let Some(colon_index) = title_content.rfind(':') {
                                    subtitle =
                                        Some(title_content[colon_index + 1..].trim().to_string());
                                    // TODO(nlopes): none of this is necessary if I parse
                                    // subtitle in the grammar
                                    //
                                    // title_content = title_content[..colon_index].trim().to_string();
                                }
                                let title_location = Location::from_pair(&inner_pair);
                                title = if inner_pair.clone().into_inner().as_str().is_empty() {
                                    vec![InlineNode::PlainText(Plain {
                                        content: title_content.clone(),
                                        location: title_location.clone(),
                                    })]
                                } else {
                                    let text = inner_pair.as_str();
                                    let start_pos = inner_pair.as_span().start_pos().pos();

                                    let mut state = ParserState::new();
                                    state.set_initial_position(&title_location, start_pos);
                                    // Run inline preprocessor before parsing inlines
                                    let processed =
                                        inline_preprocessing::run(text, parent_attributes, &state)
                                            .map_err(|e| {
                                                tracing::error!(
                                                    "error processing document title: {}",
                                                    e
                                                );
                                                Error::Parse(e.to_string())
                                            })?;

                                    let mut pairs =
                                        InnerPestParser::parse(Rule::inlines, &processed.text)
                                            .map_err(|e| Error::Parse(e.to_string()))?;

                                    parse_inlines(
                                        pairs.next().ok_or_else(|| {
                                            tracing::error!("error parsing document title");
                                            Error::Parse("error parsing document title".to_string())
                                        })?,
                                        Some(&processed),
                                        None,
                                        parent_attributes,
                                    )?
                                };
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
                                parent_attributes.insert(
                                    "revnumber".to_string(),
                                    AttributeValue::String(pair.as_str().to_string()),
                                );
                            }
                            Rule::revision_date => {
                                parent_attributes.insert(
                                    "revdate".to_string(),
                                    AttributeValue::String(pair.as_str().to_string()),
                                );
                            }
                            Rule::revision_remark => {
                                parent_attributes.insert(
                                    "revremark".to_string(),
                                    AttributeValue::String(pair.as_str().to_string()),
                                );
                            }
                            unknown => unreachable!("{:?}", unknown),
                        }
                    }
                }
                Rule::document_attribute => {
                    let (name, value) =
                        DocumentAttribute::parse(pair.into_inner(), parent_attributes);
                    parent_attributes.insert(name, value);
                }
                unknown => unreachable!("{:?}", unknown),
            }
        }

        Ok(
            if title.is_empty() && subtitle.is_none() && authors.is_empty() {
                // We do this here because we do may capture document attributes while parsing
                // the document header, and in that case we want to make sure we don't return
                // an empty header
                None
            } else {
                Some(Self {
                    title,
                    subtitle,
                    authors,
                    location,
                })
            },
        )
    }
}
