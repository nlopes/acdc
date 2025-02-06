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
            let mapped_location = map_inline_location(&location, processed)
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
                        content: mapped_location.0.unwrap_or_default(),
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

fn map_inline_location(
    location: &Location,
    processed: Option<&ProcessedContent>,
) -> Option<(Option<String>, Location)> {
    let processed = processed?;
    let mut adjustment = 0;
    let mut passthrough_index = 0;
    let location_absolute_start = i32::try_from(location.absolute_start).unwrap();

    let mut previously_seen_rep_processed_end = 0;

    for rep in &processed.source_map.replacements {
        let rep_absolute_start = i32::try_from(rep.absolute_start).unwrap();
        let rep_processed_end = i32::try_from(rep.processed_end).unwrap();

        let adjusted_absolute_start = location_absolute_start + adjustment;

        // if this adjusted absolute start is negative, we are no longer in a valid
        // situation, we can stop going through the replacements.
        if adjusted_absolute_start < 0 {
            break;
        }

        // if the adjusted absolute start is greater than the processed end of the last
        // replacement, we can stop and calculate the new location for this location.
        if previously_seen_rep_processed_end >= adjusted_absolute_start {
            break;
        }

        if rep.kind == ProcessedKind::Passthrough {
            // if we find a passthrough in this index, we need to adjust the location
            let Some(passthrough) = processed.passthroughs.get(passthrough_index) else {
                continue;
            };
            passthrough_index += 1;

            // We do this because pest parses \u{FFFD} as a 3-byte character and we
            // need to adjust the location accordingly. We use 3*\u{FFFD} on each side
            // of a passthrough, therefore (3-1)*3*2 = 18.
            if passthrough.kind != PassthroughKind::Macro {
                adjustment -= 12;
            }

            let adjusted_absolute_start_mapped = processed
                .source_map
                .map_position(usize::try_from(adjusted_absolute_start).unwrap());

            // If this is true, we went too far and need to adjust the adjustment back.
            if adjusted_absolute_start_mapped < rep.absolute_start {
                adjustment += 12;
            }

            // This means we are inside a passthrough, so we need to adjust the location
            if adjusted_absolute_start_mapped >= rep.absolute_start
                && adjusted_absolute_start_mapped < rep.absolute_end
            {
                let new_location = Location {
                    start: Position {
                        line: location.start.line,
                        column: processed.source_map.map_position(location.start.column),
                    },
                    end: Position {
                        line: location.end.line,
                        column: processed.source_map.map_position(
                            location.start.column + rep.absolute_end - rep.absolute_start,
                        ),
                    },
                    absolute_start: rep.absolute_start,
                    absolute_end: rep.absolute_end,
                };
                return Some((passthrough.text.clone(), new_location));
            }
        } else if adjusted_absolute_start >= rep_absolute_start
            && adjusted_absolute_start < rep_processed_end
        {
            let new_location = Location {
                start: Position {
                    line: location.start.line,
                    column: processed.source_map.map_position(location.start.column),
                },
                end: Position {
                    line: location.end.line,
                    column: processed.source_map.map_position(
                        location.start.column + rep.absolute_end - rep.absolute_start,
                    ),
                },
                absolute_start: rep.absolute_start,
                absolute_end: rep.absolute_end,
            };
            return Some((None, new_location));
        }

        // We need to keep track of the processed end of the last replacement we saw
        // to know when to stop going through the replacements.
        previously_seen_rep_processed_end = rep_processed_end;
    }

    // Otherwise, if this location is not inside an attribute replacement,
    // adjust its absolute_start and absolute_end using map_position.
    let adjusted_absolute_start = location_absolute_start + adjustment;
    let adjusted_absolute_start_mapped = processed
        .source_map
        .map_position(usize::try_from(adjusted_absolute_start).unwrap());
    let location_start_column_mapped = processed.source_map.map_position(location.start.column);
    let new_location = Location {
        start: Position {
            line: location.start.line,
            column: location_start_column_mapped,
        },
        end: Position {
            line: location.end.line,
            column: location_start_column_mapped + (location.end.column - location.start.column),
        },
        absolute_start: adjusted_absolute_start_mapped,
        absolute_end: adjusted_absolute_start_mapped
            + (location.absolute_end - location.absolute_start),
    };
    Some((None, new_location))
}
