mod autolink;
mod button;
mod icon;
mod image;
mod keyboard;
mod link;
mod menu;
mod pass;
mod url;

use std::collections::HashMap;

use acdc_core::{AttributeName, DocumentAttributes, Location, Position};
use pest::{
    iterators::{Pair, Pairs},
    Parser as _,
};
use tracing::instrument;

use crate::{
    model::{
        Autolink, BoldText, Button, HighlightText, Icon, Image, InlineMacro, InlineNode,
        ItalicText, Keyboard, Link, Menu, MonospaceText, Pass, PlainText, SubscriptText,
        SuperscriptText, Url,
    },
    Error, Rule,
};

impl InlineNode {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<InlineNode, Error> {
        let mut role = None;
        for pair in pairs {
            let start = pair.as_span().start_pos();
            let end = pair.as_span().end_pos();

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

            match pair.as_rule() {
                Rule::plain_text | Rule::one_line_plain_text => {
                    let content = pair.as_str().trim();
                    let content = content
                        .strip_suffix("\r\n")
                        .or(content.strip_suffix("\n"))
                        .unwrap_or(content)
                        .to_string();

                    return Ok(InlineNode::PlainText(PlainText { content, location }));
                }
                Rule::highlight_text | Rule::highlight_text_unconstrained => {
                    let unconstrained = pair.as_rule() == Rule::highlight_text_unconstrained;
                    let content =
                        get_content("highlight", unconstrained, &pair, parent_attributes)?;
                    return Ok(InlineNode::HighlightText(HighlightText {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::italic_text | Rule::italic_text_unconstrained => {
                    let unconstrained = pair.as_rule() == Rule::italic_text_unconstrained;
                    let content = get_content("italic", unconstrained, &pair, parent_attributes)?;
                    return Ok(InlineNode::ItalicText(ItalicText {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::bold_text | Rule::bold_text_unconstrained => {
                    let unconstrained = pair.as_rule() == Rule::bold_text_unconstrained;
                    let content = get_content("bold", unconstrained, &pair, parent_attributes)?;
                    return Ok(InlineNode::BoldText(BoldText {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::monospace_text | Rule::monospace_text_unconstrained => {
                    let unconstrained = pair.as_rule() == Rule::monospace_text_unconstrained;
                    let content =
                        get_content("monospace", unconstrained, &pair, parent_attributes)?;
                    return Ok(InlineNode::MonospaceText(MonospaceText {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::subscript_text => {
                    let content = get_content("subscript", false, &pair, parent_attributes)?;
                    return Ok(InlineNode::SubscriptText(SubscriptText {
                        role,
                        content,
                        location,
                    }));
                }
                Rule::superscript_text => {
                    let content = get_content("superscript", false, &pair, parent_attributes)?;
                    return Ok(InlineNode::SuperscriptText(SuperscriptText {
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
                | Rule::pass_inline
                | Rule::single_double_passthrough
                | Rule::triple_passthrough => return Self::parse_macro(pair),
                Rule::role => role = Some(pair.as_str().to_string()),
                Rule::inline_line_break | Rule::hard_wrap => {
                    return Ok(InlineNode::InlineLineBreak(location));
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        // TODO: this should be unreachable instead!
        Err(Error::Parse("no valid inline text found".to_string()))
    }

    #[instrument(level = "trace")]
    fn parse_macro(pair: Pair<Rule>) -> Result<InlineNode, Error> {
        let start = pair.as_span().start_pos();
        let end = pair.as_span().end_pos();
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
            Rule::single_double_passthrough | Rule::triple_passthrough => {
                Ok(InlineNode::Macro(InlineMacro::Pass(
                    Pass::parse_inline_single_double_or_triple(Pairs::single(pair), location),
                )))
            }
            unknown => unreachable!("{unknown:?}"),
        }
    }
}

fn parse_named_attribute(
    pairs: Pairs<Rule>,
    attributes: &mut HashMap<AttributeName, Option<String>>,
) {
    let mut name = String::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::id => {
                attributes.insert("id".to_string(), Some(pair.as_str().to_string()));
            }
            Rule::role => {
                attributes.insert("role".to_string(), Some(pair.as_str().to_string()));
            }
            Rule::option => {
                attributes.insert("option".to_string(), Some(pair.as_str().to_string()));
            }
            Rule::attribute_name => name = pair.as_str().to_string(),
            Rule::named_attribute_value => {
                attributes.insert(name.clone(), Some(pair.as_str().to_string()));
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
}

#[instrument(level = "trace")]
pub(crate) fn parse_inlines(
    pair: Pair<Rule>,
    parent_attributes: &mut DocumentAttributes,
) -> Result<Vec<InlineNode>, Error> {
    let pairs = pair.into_inner();
    let mut content = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::non_plain_text => {
                content.push(InlineNode::parse(pair.into_inner(), parent_attributes)?);
            }
            Rule::plain_text | Rule::one_line_plain_text => {
                content.push(InlineNode::parse(Pairs::single(pair), parent_attributes)?);
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
    parent_attributes: &mut DocumentAttributes,
) -> Result<Vec<InlineNode>, Error> {
    let mut content = Vec::new();
    let len = pair.as_str().len();
    let token_length = if unconstrained { 2 } else { 1 };
    match crate::InnerPestParser::parse(
        Rule::inlines,
        &pair.as_str()[token_length..len - token_length],
    ) {
        Ok(pairs) => {
            for pair in pairs {
                content.extend(parse_inlines(pair, parent_attributes)?);
            }
        }
        Err(e) => {
            tracing::error!(text_style, "error parsing text: {e}");
            return Err(Error::Parse(e.to_string()));
        }
    }
    Ok(content)
}
