use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{InlineMacro, InlineNode};

use crate::{Error, ManpageHtmlVisitor, escape::escape_html};

pub(crate) fn visit_inline_node<W: Write>(
    node: &InlineNode,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    match node {
        InlineNode::PlainText(text) => {
            write!(visitor.writer_mut(), "{}", escape_html(&text.content))?;
        }

        InlineNode::RawText(text) => {
            write!(visitor.writer_mut(), "{}", escape_html(&text.content))?;
        }

        InlineNode::VerbatimText(text) => {
            write!(visitor.writer_mut(), "{}", escape_html(&text.content))?;
        }

        InlineNode::BoldText(bold) => {
            write!(visitor.writer_mut(), "<b>")?;
            visitor.visit_inline_nodes(&bold.content)?;
            write!(visitor.writer_mut(), "</b>")?;
        }

        InlineNode::ItalicText(italic) => {
            write!(visitor.writer_mut(), "<i>")?;
            visitor.visit_inline_nodes(&italic.content)?;
            write!(visitor.writer_mut(), "</i>")?;
        }

        InlineNode::MonospaceText(mono) => {
            write!(visitor.writer_mut(), "<code class=\"Li\">")?;
            visitor.visit_inline_nodes(&mono.content)?;
            write!(visitor.writer_mut(), "</code>")?;
        }

        InlineNode::HighlightText(highlight) => {
            write!(visitor.writer_mut(), "<mark>")?;
            visitor.visit_inline_nodes(&highlight.content)?;
            write!(visitor.writer_mut(), "</mark>")?;
        }

        InlineNode::SubscriptText(sub) => {
            write!(visitor.writer_mut(), "<sub>")?;
            visitor.visit_inline_nodes(&sub.content)?;
            write!(visitor.writer_mut(), "</sub>")?;
        }

        InlineNode::SuperscriptText(sup) => {
            write!(visitor.writer_mut(), "<sup>")?;
            visitor.visit_inline_nodes(&sup.content)?;
            write!(visitor.writer_mut(), "</sup>")?;
        }

        InlineNode::CurvedQuotationText(quoted) => {
            write!(visitor.writer_mut(), "\u{201c}")?;
            visitor.visit_inline_nodes(&quoted.content)?;
            write!(visitor.writer_mut(), "\u{201d}")?;
        }

        InlineNode::CurvedApostropheText(quoted) => {
            write!(visitor.writer_mut(), "\u{2018}")?;
            visitor.visit_inline_nodes(&quoted.content)?;
            write!(visitor.writer_mut(), "\u{2019}")?;
        }

        InlineNode::StandaloneCurvedApostrophe(_) => {
            write!(visitor.writer_mut(), "\u{2019}")?;
        }

        InlineNode::LineBreak(_) => {
            write!(visitor.writer_mut(), "<br>")?;
        }

        InlineNode::InlineAnchor(anchor) => {
            write!(
                visitor.writer_mut(),
                "<a id=\"{}\"></a>",
                escape_html(&anchor.id)
            )?;
        }

        InlineNode::Macro(inline_macro) => {
            visit_inline_macro(inline_macro, visitor)?;
        }

        InlineNode::CalloutRef(callout) => {
            write!(
                visitor.writer_mut(),
                "<b class=\"callout\">({})</b>",
                callout.number
            )?;
        }

        _ => {}
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn visit_inline_macro<W: Write>(
    macro_node: &InlineMacro,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    match macro_node {
        InlineMacro::Url(url) => {
            let target = escape_html(&url.target.to_string());
            if url.text.is_empty() {
                write!(visitor.writer_mut(), "<a href=\"{target}\">{target}</a>")?;
            } else {
                write!(visitor.writer_mut(), "<a href=\"{target}\">")?;
                visitor.visit_inline_nodes(&url.text)?;
                write!(visitor.writer_mut(), "</a>")?;
            }
        }

        InlineMacro::Mailto(mailto) => {
            let target = escape_html(&mailto.target.to_string());
            let href = if target.starts_with("mailto:") {
                target.clone()
            } else {
                format!("mailto:{target}")
            };
            if mailto.text.is_empty() {
                let display = target.strip_prefix("mailto:").unwrap_or(&target);
                write!(visitor.writer_mut(), "<a href=\"{href}\">{display}</a>")?;
            } else {
                write!(visitor.writer_mut(), "<a href=\"{href}\">")?;
                visitor.visit_inline_nodes(&mailto.text)?;
                write!(visitor.writer_mut(), "</a>")?;
            }
        }

        InlineMacro::Link(link) => {
            let target = escape_html(&link.target.to_string());
            if let Some(text) = &link.text {
                let text = escape_html(text);
                write!(visitor.writer_mut(), "<a href=\"{target}\">{text}</a>")?;
            } else {
                write!(visitor.writer_mut(), "<a href=\"{target}\">{target}</a>")?;
            }
        }

        InlineMacro::Autolink(autolink) => {
            let url = escape_html(&autolink.url.to_string());
            if let Some(email) = url.strip_prefix("mailto:") {
                write!(visitor.writer_mut(), "<a href=\"{url}\">{email}</a>")?;
            } else {
                write!(visitor.writer_mut(), "<a href=\"{url}\">{url}</a>")?;
            }
        }

        InlineMacro::CrossReference(xref) => {
            if xref.text.is_empty() {
                let target = escape_html(&xref.target);
                // Try to format as manpage reference: name(N) -> bold name with section
                if let Some((name, vol)) = target.rsplit_once('(') {
                    if vol.ends_with(')') && vol.len() <= 3 {
                        write!(visitor.writer_mut(), "<b>{name}</b>({vol}")?;
                    } else {
                        write!(visitor.writer_mut(), "{target}")?;
                    }
                } else {
                    write!(visitor.writer_mut(), "{target}")?;
                }
            } else {
                visitor.visit_inline_nodes(&xref.text)?;
            }
        }

        InlineMacro::Footnote(footnote) => {
            write!(
                visitor.writer_mut(),
                "<sup class=\"footnote\">[{}]</sup>",
                footnote.number
            )?;
        }

        InlineMacro::Image(img) => {
            if img.title.is_empty() {
                write!(visitor.writer_mut(), "[IMAGE]")?;
            } else {
                write!(visitor.writer_mut(), "[")?;
                visitor.visit_inline_nodes(&img.title)?;
                write!(visitor.writer_mut(), "]")?;
            }
        }

        InlineMacro::Icon(icon) => {
            write!(
                visitor.writer_mut(),
                "[{}]",
                escape_html(&icon.target.to_string())
            )?;
        }

        InlineMacro::Keyboard(kbd) => {
            write!(visitor.writer_mut(), "<kbd>")?;
            for (i, key) in kbd.keys.iter().enumerate() {
                if i > 0 {
                    write!(visitor.writer_mut(), "+")?;
                }
                write!(visitor.writer_mut(), "{}", escape_html(key))?;
            }
            write!(visitor.writer_mut(), "</kbd>")?;
        }

        InlineMacro::Button(btn) => {
            write!(
                visitor.writer_mut(),
                "<b class=\"Sy\">[{}]</b>",
                escape_html(&btn.label)
            )?;
        }

        InlineMacro::Menu(menu) => {
            write!(visitor.writer_mut(), "<b>{}</b>", escape_html(&menu.target))?;
            for item in &menu.items {
                write!(
                    visitor.writer_mut(),
                    " \u{2192} <b>{}</b>",
                    escape_html(item)
                )?;
            }
        }

        InlineMacro::Pass(pass) => {
            if let Some(text) = &pass.text {
                write!(visitor.writer_mut(), "{text}")?;
            }
        }

        InlineMacro::Stem(stem) => {
            write!(
                visitor.writer_mut(),
                "<code class=\"stem\">{}</code>",
                escape_html(&stem.content)
            )?;
        }

        InlineMacro::IndexTerm(it) => {
            if it.is_visible() {
                write!(visitor.writer_mut(), "{}", escape_html(it.term()))?;
            }
        }

        _ => {}
    }

    Ok(())
}
