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
    error::Error, inline_preprocessor::ProcessedKind, AttributeValue, Autolink, Bold, Button,
    DocumentAttributes, ElementAttributes, Highlight, Icon, Image, InlineMacro, InlineNode, Italic,
    Keyboard, LineBreak, Link, Location, Menu, Monospace, Pass, Plain, Position, ProcessedContent,
    Raw, Rule, Subscript, Superscript, Url,
};

impl InlineNode {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        processed: Option<&ProcessedContent>,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<InlineNode, Error> {
        let mut role = None;

        for pair in pairs {
            let mut location = Location::from_pair(&pair);
            location.shift_inline(parent_location);

            match pair.as_rule() {
                Rule::plain_text | Rule::one_line_plain_text => {
                    let content = pair.as_str();
                    let content = content
                        .strip_suffix("\r\n")
                        .or(content.strip_suffix("\n"))
                        .unwrap_or(content)
                        .to_string();
                    match map_inline_location(&location, processed) {
                        Some((Some(content), location)) => {
                            return Ok(InlineNode::PlainText(Plain { content, location }));
                        }
                        Some((None, location)) => {
                            return Ok(InlineNode::PlainText(Plain { content, location }));
                        }
                        None => {
                            return Ok(InlineNode::PlainText(Plain { content, location }));
                        }
                    }
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
                        Some(&location),
                        parent_attributes,
                    )?;
                    return Ok(InlineNode::BoldText(Bold {
                        role,
                        content,
                        location,
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
                    if let Some((Some(content), location)) =
                        map_inline_location(&location, processed)
                    {
                        return Ok(InlineNode::RawText(Raw { content, location }));
                    }
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
    for pair in pairs {
        match pair.as_rule() {
            Rule::non_plain_text => {
                let entry = InlineNode::parse(
                    pair.into_inner(),
                    processed,
                    parent_location,
                    parent_attributes,
                )?;
                content.push(entry);
            }
            Rule::plain_text | Rule::one_line_plain_text => {
                let entry = InlineNode::parse(
                    Pairs::single(pair),
                    processed,
                    parent_location,
                    parent_attributes,
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
        location.shift_line_column(1, token_length + 1);
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

#[instrument(level = "trace")]
fn map_inline_location(
    location: &Location,
    processed: Option<&ProcessedContent>,
) -> Option<(Option<String>, Location)> {
    // If there is processed content, which is the case for a placeholder,
    // like a pass macro or a passthrough we need to find the processed
    // content that matches the location of the placeholder and return the
    // content of the passthrough, with updated location information
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
                {
                    (Some(pass), passthroughs_before_count)
                } else {
                    (matching_pass, passthroughs_before_count)
                }
            },
        );
        dbg!((effective_start, matching_pass));
        if let Some(pass) = matching_pass {
            let mapped_start = processed
                .source_map
                .map_position(effective_start - passthroughs_before_count)
                + 1
                - passthroughs_before_count;
            let mapped_end = processed
                .source_map
                .map_position(location.absolute_end - passthroughs_before_count - 1)
                - 1
                - passthroughs_before_count;
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
            return Some((pass.text.clone(), location));
        }

        // If we're here, it means that the placeholder is not a passthrough
        //
        // We also have to calculate the delta in the column position and for that we
        // have to do some trickery.  We must map_position of the effective_start
        // (that's the mapped position of the absolute start) which might feel like a
        // repeat but it's not. the effective start is the mapped position of the
        // absolute start. What we want is to understand how much the effective start
        // is shifted from the absolute start. We can then use this delta to adjust
        // the column position of the location.
        let mapping_to_calculate_delta = processed.source_map.map_position(effective_start);

        let delta = if mapping_to_calculate_delta > effective_start {
            mapping_to_calculate_delta - effective_start
        } else {
            effective_start - mapping_to_calculate_delta
        };
        let location = Location {
            start: Position {
                line: location.start.line,
                column: location.start.column + delta,
            },
            end: Position {
                line: location.end.line,
                column: location.end.column + delta,
            },
            absolute_start: processed.source_map.map_position(effective_start),
            absolute_end: processed.source_map.map_position(location.absolute_end),
        };
        return Some((None, location));
    }
    tracing::debug!(
        location = %location,
        "no processed content found for placeholder at location"
    );
    None
}
