mod autolink;
mod button;
mod icon;
mod image;
mod keyboard;
mod link;
mod menu;
mod pass;
mod url;

use pest::{
    iterators::{Pair, Pairs},
    Parser as _,
};
use tracing::instrument;

use crate::{
    error::Error, AttributeValue, Autolink, Bold, Button, DocumentAttributes, ElementAttributes,
    Highlight, Icon, Image, InlineMacro, InlineNode, Italic, Keyboard, LineBreak, Link, Location,
    Menu, Monospace, Pass, PassthroughKind, Plain, Position, ProcessedContent, ProcessedKind, Raw,
    Rule, Subscript, Superscript, Url,
};

impl InlineNode {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        processed: Option<&ProcessedContent>,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
        last_index_seen: &mut Option<usize>,
    ) -> Result<InlineNode, Error> {
        let mut role = None;

        for pair in pairs {
            let mut location = Location::from_pair(&pair);
            location.shift_inline(parent_location);

            let rule = pair.as_rule();
            let mut index = None;

            if rule == Rule::placeholder {
                let inner = pair.clone().into_inner().next().unwrap();
                index = Some(inner.as_str().parse::<usize>().unwrap_or_default());
                *last_index_seen = index;
            }
            let mapped_location =
                map_inline_location(&location, processed, index, *last_index_seen)
                    .unwrap_or((Some(pair.as_str().to_string()), location.clone()));
            match rule {
                Rule::plain_text | Rule::one_line_plain_text => {
                    let content = pair.as_str();
                    let content = content
                        .strip_suffix("\r\n")
                        .or(content.strip_suffix("\n"))
                        .unwrap_or(content)
                        .to_string();
                    return Ok(InlineNode::PlainText(Plain {
                        content,
                        location: mapped_location.1,
                    }));
                }
                Rule::highlight_text | Rule::highlight_text_unconstrained => {
                    let unconstrained = pair.as_rule() == Rule::highlight_text_unconstrained;
                    let content = get_content(
                        "highlight",
                        unconstrained,
                        &pair,
                        processed,
                        Some(&location),
                        parent_attributes,
                    )?;
                    return Ok(InlineNode::HighlightText(Highlight {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::italic_text | Rule::italic_text_unconstrained => {
                    let unconstrained = pair.as_rule() == Rule::italic_text_unconstrained;
                    let content = get_content(
                        "italic",
                        unconstrained,
                        &pair,
                        processed,
                        Some(&location),
                        parent_attributes,
                    )?;
                    return Ok(InlineNode::ItalicText(Italic {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::bold_text | Rule::bold_text_unconstrained => {
                    let unconstrained = pair.as_rule() == Rule::bold_text_unconstrained;
                    let content = get_content(
                        "bold",
                        unconstrained,
                        &pair,
                        processed,
                        Some(&mapped_location.1),
                        parent_attributes,
                    )?;
                    return Ok(InlineNode::BoldText(Bold {
                        role,
                        content,
                        location: mapped_location.1,
                    }));
                }
                Rule::monospace_text | Rule::monospace_text_unconstrained => {
                    let unconstrained = pair.as_rule() == Rule::monospace_text_unconstrained;
                    let content = get_content(
                        "monospace",
                        unconstrained,
                        &pair,
                        processed,
                        Some(&location),
                        parent_attributes,
                    )?;
                    return Ok(InlineNode::MonospaceText(Monospace {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::subscript_text => {
                    let content = get_content(
                        "subscript",
                        false,
                        &pair,
                        processed,
                        Some(&location),
                        parent_attributes,
                    )?;
                    return Ok(InlineNode::SubscriptText(Subscript {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::superscript_text => {
                    let content = get_content(
                        "superscript",
                        false,
                        &pair,
                        processed,
                        Some(&location),
                        parent_attributes,
                    )?;
                    return Ok(InlineNode::SuperscriptText(Superscript {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::icon_inline
                | Rule::image_inline
                | Rule::keyboard_inline
                | Rule::btn_inline
                | Rule::menu_inline
                | Rule::url_macro
                | Rule::link_macro
                | Rule::autolink
                | Rule::pass_inline => return Self::parse_macro(pair),
                Rule::placeholder => {
                    let kind = processed
                        .unwrap()
                        .passthroughs
                        .get(index.unwrap())
                        .unwrap()
                        .kind
                        .clone();
                    if kind == PassthroughKind::Single || kind == PassthroughKind::Double {
                        return Ok(InlineNode::PlainText(Plain {
                            content: mapped_location.0.unwrap_or_default(),
                            location: mapped_location.1,
                        }));
                    }
                    return Ok(InlineNode::RawText(Raw {
                        content: pair.as_str().to_string(),
                        location: mapped_location.1,
                    }));
                }
                Rule::role => role = Some(pair.as_str().to_string()),
                Rule::inline_line_break | Rule::hard_wrap => {
                    return Ok(InlineNode::LineBreak(LineBreak { location }));
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Err(Error::Parse(format!(
            "no valid inline text found{}",
            if let Some(location) = parent_location {
                format!(": {location}")
            } else {
                String::new()
            }
        )))
    }

    #[instrument(level = "trace")]
    fn parse_macro(pair: Pair<Rule>) -> Result<InlineNode, Error> {
        let location = Location::from_pair(&pair);
        match pair.as_rule() {
            Rule::icon_inline => Ok(InlineNode::Macro(InlineMacro::Icon(Icon::parse_inline(
                pair.into_inner(),
                location,
            )))),
            Rule::image_inline => Ok(InlineNode::Macro(InlineMacro::Image(Box::new(
                Image::parse_inline(pair.into_inner(), location),
            )))),
            Rule::keyboard_inline => Ok(InlineNode::Macro(InlineMacro::Keyboard(
                Keyboard::parse_inline(pair.into_inner(), location),
            ))),
            Rule::btn_inline => Ok(InlineNode::Macro(InlineMacro::Button(
                Button::parse_inline(pair.into_inner(), location),
            ))),
            Rule::menu_inline => Ok(InlineNode::Macro(InlineMacro::Menu(Menu::parse_inline(
                pair.into_inner(),
                location,
            )))),
            Rule::url_macro => Ok(InlineNode::Macro(InlineMacro::Url(Url::parse_inline(
                pair.into_inner(),
                location,
            )))),
            Rule::link_macro => Ok(InlineNode::Macro(InlineMacro::Link(Link::parse_inline(
                pair.into_inner(),
                location,
            )))),
            Rule::autolink => Ok(InlineNode::Macro(InlineMacro::Autolink(
                Autolink::parse_inline(pair.into_inner(), location),
            ))),
            Rule::pass_inline => Ok(InlineNode::Macro(InlineMacro::Pass(Pass::parse_inline(
                pair.into_inner(),
                location,
            )))),
            unknown => unreachable!("{unknown:?}"),
        }
    }
}

fn parse_named_attribute(pairs: Pairs<Rule>, attributes: &mut ElementAttributes) {
    let mut name = String::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::id => {
                attributes.insert(
                    "id".to_string(),
                    AttributeValue::String(pair.as_str().to_string()),
                );
            }
            Rule::role => {
                attributes.insert(
                    "role".to_string(),
                    AttributeValue::String(pair.as_str().to_string()),
                );
            }
            Rule::option => {
                attributes.insert(
                    "option".to_string(),
                    AttributeValue::String(pair.as_str().to_string()),
                );
            }
            Rule::attribute_name => name = pair.as_str().to_string(),
            Rule::named_attribute_value => {
                attributes.insert(
                    name.clone(),
                    AttributeValue::String(pair.as_str().to_string()),
                );
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
}

#[instrument(level = "trace")]
pub(crate) fn parse_inlines(
    pair: Pair<Rule>,
    processed: Option<&ProcessedContent>,
    parent_location: Option<&Location>,
    parent_attributes: &mut DocumentAttributes,
) -> Result<Vec<InlineNode>, Error> {
    let pairs = pair.into_inner();
    let mut content = Vec::new();
    let mut last_index_seen = None;

    for pair in pairs {
        match pair.as_rule() {
            Rule::non_plain_text => {
                let entry = InlineNode::parse(
                    pair.into_inner(),
                    processed,
                    parent_location,
                    parent_attributes,
                    &mut last_index_seen,
                )?;
                content.push(entry);
            }
            Rule::plain_text | Rule::one_line_plain_text => {
                let entry = InlineNode::parse(
                    Pairs::single(pair),
                    processed,
                    parent_location,
                    parent_attributes,
                    &mut last_index_seen,
                )?;
                content.push(entry);
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
    processed: Option<&ProcessedContent>,
    parent_location: Option<&Location>,
    parent_attributes: &mut DocumentAttributes,
) -> Result<Vec<InlineNode>, Error> {
    let mut content = Vec::new();
    let len = pair.as_str().len();
    let token_length = if unconstrained { 2 } else { 1 };
    let parent_location = if let Some(location) = parent_location {
        let mut location = location.clone();
        location.absolute_end -= token_length * 2;
        Some(location)
    } else {
        None
    };
    match crate::InnerPestParser::parse(
        Rule::inlines,
        &pair.as_str()[token_length..len - token_length],
    ) {
        Ok(pairs) => {
            for pair in pairs {
                content.extend(parse_inlines(
                    pair,
                    processed,
                    parent_location.as_ref(),
                    parent_attributes,
                )?);
            }
        }
        Err(e) => {
            tracing::error!(text_style, "error parsing text: {e}");
            return Err(Error::Parse(e.to_string()));
        }
    }
    Ok(content)
}

fn map_inline_location2(
    location: &Location,
    processed: Option<&ProcessedContent>,
    index: Option<usize>,
) -> Option<(Option<String>, Location)> {
    if let Some(processed) = processed {
        let effective_start = location.absolute_start;
        let (matching_pass, passthroughs_before_count) = processed.passthroughs.iter().fold(
            (None, 0),
            |(matching_pass, passthroughs_before_count), pass| {
                if pass.location.absolute_start < effective_start {
                    (None, passthroughs_before_count + 1)
                }
                // We adjust this by multiplying the passthroughs_before_count by 3 because that's the
                // length of the passthrough according to pest (len_utf8)
                else if pass.location.absolute_start
                    == effective_start + 3 * passthroughs_before_count
                    || pass.location.absolute_start == effective_start + 3
                    || pass.location.absolute_start
                        == effective_start
                            + 3 * (if passthroughs_before_count > 0 {
                                passthroughs_before_count - 1
                            } else {
                                passthroughs_before_count
                            })
                {
                    (Some(pass), passthroughs_before_count)
                } else {
                    (matching_pass, passthroughs_before_count)
                }
            },
        );
        if let Some(pass) = matching_pass {
            let mapped_start = processed
                .source_map
                .map_position(effective_start - 2 * passthroughs_before_count)
                + 1;
            let mapped_end = processed.source_map.map_position(location.absolute_end);

            let location = Location {
                start: Position {
                    line: location.start.line,
                    column: mapped_start,
                },
                end: Position {
                    line: location.end.line,
                    column: mapped_end,
                },
                absolute_start: pass.location.absolute_start,
                absolute_end: pass.location.absolute_end,
            };
            return Some((pass.text.clone(), location.clone()));
        }

        // Add the following code to sort the location of the attributes
        // if we're here and we're within the location of an attributes (per the source map having an entry for it) then, we need to adjust start and end columns
        // to the original source location
        //
        // This is because the source map only maps the start and end columns of the attributes
        // and not the content within the attributes
        if let Some((_, offset, _)) =
            processed
                .source_map
                .offsets
                .iter()
                .find(|(absolute_start, _offset, kind)| {
                    *kind == ProcessedKind::Attribute && *absolute_start == location.absolute_start
                })
        {
            let mut adjusted_location = location.clone();
            let end = i32::try_from(adjusted_location.end.column).unwrap() - offset;
            adjusted_location.end.column = usize::try_from(end).unwrap();

            return Some((None, adjusted_location));
        }

        // If we're here, we're adjusting the location of plain text usually.
        let mut adjusted_location = location.clone();

        // Map positions back to the original source using the source map
        adjusted_location.start.column = processed
            .source_map
            .map_position(adjusted_location.start.column);
        adjusted_location.end.column = processed
            .source_map
            .map_position(adjusted_location.end.column);

        // This feels like a hack but it makes no sense to have the end column be the same
        // as the start column, so we adjust it by one. TODO(nlopes): Investigate why this is
        // happening.
        if adjusted_location.start.column == adjusted_location.end.column {
            adjusted_location.end.column += 1;
        }

        // Adjust the absolute start and end positions to the original source
        adjusted_location.absolute_start = processed
            .source_map
            .map_position(adjusted_location.absolute_start)
            - 2 * passthroughs_before_count;
        adjusted_location.absolute_end = processed
            .source_map
            .map_position(adjusted_location.absolute_end)
            - 2 * passthroughs_before_count;
        Some((None, adjusted_location))
    } else {
        None
    }
}

#[instrument(level = "trace")]
#[allow(clippy::too_many_arguments)]
fn map_inline_location(
    location: &Location,
    processed: Option<&ProcessedContent>,
    index: Option<usize>,
    last_index_seen: Option<usize>,
) -> Option<(Option<String>, Location)> {
    match (processed, index) {
        (Some(processed), Some(index)) => {
            let item = processed
                .passthroughs
                .get(index)
                .expect("passthrough index out of bounds");
            Some((Some(item.text.clone().unwrap()), item.location.clone()))
        }
        (Some(processed), None) => {
            // If we get here, we might be looking at plaintext from an attribute we handled in the preprocessor.
            // We need to adjust the location to the original source location.
            // We can do this by looking at the source map and finding the offset for the location.
            // We can then adjust the start and end columns by that offset.

            let attribute_total_offset: i32 = processed
                .source_map
                .offsets
                .iter()
                .filter(|(absolute_start, _offset, kind)| {
                    *kind == ProcessedKind::Attribute && *absolute_start < location.absolute_start
                })
                .map(|(_absolute_start, offset, _kind)| offset)
                .sum();
            if let Some((_, offset, kind)) = processed
                .source_map
                .offsets
                .iter()
                .find(|(absolute_start, _, _)| *absolute_start == location.absolute_start)
            {
                let start_location = processed.source_map.map_position(location.absolute_start);
                let end_location = processed.source_map.map_position(location.absolute_end);
                let end_column =
                    i32::try_from(processed.source_map.map_position(location.end.column)).unwrap()
                        - offset;
                if *kind == ProcessedKind::Attribute {
                    let location = Location {
                        start: Position {
                            line: location.start.line,
                            column: location.start.column,
                        },
                        end: Position {
                            line: location.end.line,
                            column: usize::try_from(end_column).unwrap(),
                        },
                        absolute_start: start_location,
                        absolute_end: end_location,
                    };

                    return Some((None, location));
                }
            } else if attribute_total_offset > 0 && last_index_seen.is_none() {
                let location = Location {
                    start: Position {
                        line: location.start.line,
                        column: usize::try_from(
                            i32::try_from(location.start.column)
                                .expect("location start column should be castable to i32")
                                + attribute_total_offset,
                        )
                        .expect("location start column minus offset should be castable to usize"),
                    },
                    end: Position {
                        line: location.end.line,
                        column: usize::try_from(
                            i32::try_from(location.end.column)
                                .expect("location end column should be castable to i32")
                                + attribute_total_offset,
                        )
                        .expect("location end column minus offset should be castable to usize"),
                    },
                    absolute_start: location.absolute_start,
                    absolute_end: location.absolute_end,
                };
                return Some((None, location));
            }
            // using last_index_seen to get the last passthrough index - we can take its
            // end location as the start location for this one. And the passthrough after
            // this one can be used to get the end location.
            if let Some(last_index_seen) = last_index_seen {
                let item_before = processed
                    .passthroughs
                    .get(last_index_seen)
                    .expect("passthrough index out of bounds");
                let item_after = processed.passthroughs.get(last_index_seen + 1);
                let absolute_start = item_before.location.absolute_end;
                let start = item_before.location.end.column;
                let absolute_end = if let Some(pass) = item_after {
                    pass.location.absolute_start
                } else {
                    location.absolute_end
                };
                let end = if let Some(pass) = item_after {
                    pass.location.start.column
                } else {
                    location.end.column
                };
                let location = Location {
                    start: Position {
                        line: location.start.line,
                        column: start,
                    },
                    end: Position {
                        line: location.end.line,
                        column: end,
                    },
                    absolute_start,
                    absolute_end,
                };
                Some((None, location))
            } else {
                None
            }
        }
        _ => None,
    }
}
