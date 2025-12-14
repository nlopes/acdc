//! Inline node rendering for manpages.
//!
//! Handles bold, italic, monospace, links, and other inline formatting.

use std::io::Write;

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use acdc_parser::{InlineMacro, InlineNode, Link, Mailto};

use crate::{
    Error, ManpageVisitor,
    escape::{EscapeMode, manify},
};

/// Visit an inline node.
pub fn visit_inline_node<W: Write>(
    node: &InlineNode,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    match node {
        InlineNode::PlainText(text) => {
            let escaped = manify(&text.content, EscapeMode::Normalize);
            let w = visitor.writer_mut();
            write!(w, "{escaped}")?;
        }

        InlineNode::RawText(text) => {
            // Raw text - pass through with minimal escaping
            let escaped = manify(&text.content, EscapeMode::Normalize);
            let w = visitor.writer_mut();
            write!(w, "{escaped}")?;
        }

        InlineNode::VerbatimText(text) => {
            // Verbatim text - render as-is, preserve whitespace
            let escaped = manify(&text.content, EscapeMode::Preserve);
            let w = visitor.writer_mut();
            write!(w, "{escaped}")?;
        }

        InlineNode::BoldText(bold) => {
            write!(visitor.writer_mut(), "\\fB")?;
            visitor.visit_inline_nodes(&bold.content)?;
            write!(visitor.writer_mut(), "\\fP")?;
        }

        InlineNode::ItalicText(italic) => {
            write!(visitor.writer_mut(), "\\fI")?;
            visitor.visit_inline_nodes(&italic.content)?;
            write!(visitor.writer_mut(), "\\fP")?;
        }

        InlineNode::MonospaceText(mono) => {
            // Monospace is typically rendered as bold in man pages (for commands)
            write!(visitor.writer_mut(), "\\fB")?;
            visitor.visit_inline_nodes(&mono.content)?;
            write!(visitor.writer_mut(), "\\fP")?;
        }

        InlineNode::HighlightText(highlight) => {
            // Highlight - render as bold (no highlighting in roff)
            write!(visitor.writer_mut(), "\\fB")?;
            visitor.visit_inline_nodes(&highlight.content)?;
            write!(visitor.writer_mut(), "\\fP")?;
        }

        InlineNode::SubscriptText(sub) => {
            // No subscript in roff - render in parentheses
            write!(visitor.writer_mut(), "_(")?;
            visitor.visit_inline_nodes(&sub.content)?;
            write!(visitor.writer_mut(), ")")?;
        }

        InlineNode::SuperscriptText(sup) => {
            // No superscript in roff - render in parentheses
            write!(visitor.writer_mut(), "^(")?;
            visitor.visit_inline_nodes(&sup.content)?;
            write!(visitor.writer_mut(), ")")?;
        }

        InlineNode::CurvedQuotationText(quoted) => {
            write!(visitor.writer_mut(), "\\(lq")?;
            visitor.visit_inline_nodes(&quoted.content)?;
            write!(visitor.writer_mut(), "\\(rq")?;
        }

        InlineNode::CurvedApostropheText(quoted) => {
            write!(visitor.writer_mut(), "\\(oq")?;
            visitor.visit_inline_nodes(&quoted.content)?;
            write!(visitor.writer_mut(), "\\(cq")?;
        }

        InlineNode::StandaloneCurvedApostrophe(_) => {
            let w = visitor.writer_mut();
            write!(w, "\\(cq")?;
        }

        InlineNode::LineBreak(_) => {
            let w = visitor.writer_mut();
            writeln!(w)?;
            writeln!(w, ".br")?;
        }

        InlineNode::InlineAnchor(anchor) => {
            // Anchors have no visible representation in man pages
            // But we can add a comment for reference
            let w = visitor.writer_mut();
            writeln!(w, r#".\" anchor: {}"#, anchor.id)?;
        }

        InlineNode::Macro(inline_macro) => {
            visit_inline_macro(inline_macro, visitor)?;
        }

        // Handle any future variants - skip unknown nodes
        _ => {}
    }

    Ok(())
}

fn write_link<W: Write>(visitor: &mut ManpageVisitor<W>, link: &Link) -> Result<(), Error> {
    // Link has Option<String> for text, not Vec<InlineNode>
    let target_str = link.target.to_string();
    let w = visitor.writer_mut();
    if let Some(text) = &link.text {
        write!(w, "{}", manify(text, EscapeMode::Normalize))?;
        write!(
            w,
            " \\(la{}\\(ra",
            manify(&target_str, EscapeMode::Normalize)
        )?;
    } else {
        write!(
            w,
            "\\(la{}\\(ra",
            manify(&target_str, EscapeMode::Normalize)
        )?;
    }

    Ok(())
}

fn write_mailto<W: Write>(visitor: &mut ManpageVisitor<W>, mailto: &Mailto) -> Result<(), Error> {
    let target_str = mailto.target.to_string();
    let escaped_target = manify(&target_str, EscapeMode::Normalize);
    if mailto.text.is_empty() {
        let w = visitor.writer_mut();
        write!(w, "\\(la{escaped_target}\\(ra")?;
    } else {
        visitor.visit_inline_nodes(&mailto.text)?;
        let w = visitor.writer_mut();
        write!(w, " \\(la{escaped_target}\\(ra")?;
    }
    Ok(())
}

/// Visit an inline macro.
fn visit_inline_macro<W: Write>(
    macro_node: &InlineMacro,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    match macro_node {
        InlineMacro::Url(url) => {
            // URL - show as "text <url>" or just url if no text
            let target_str = url.target.to_string();
            let escaped_target = manify(&target_str, EscapeMode::Normalize);
            if url.text.is_empty() {
                let w = visitor.writer_mut();
                write!(w, "\\(la{escaped_target}\\(ra")?;
            } else {
                visitor.visit_inline_nodes(&url.text)?;
                let w = visitor.writer_mut();
                write!(w, " \\(la{escaped_target}\\(ra")?;
            }
        }

        InlineMacro::Mailto(mailto) => {
            write_mailto(visitor, mailto)?;
        }

        InlineMacro::Link(link) => {
            write_link(visitor, link)?;
        }

        InlineMacro::Autolink(autolink) => {
            let w = visitor.writer_mut();
            let url_str = autolink.url.to_string();
            write!(w, "\\(la{}\\(ra", manify(&url_str, EscapeMode::Normalize))?;
        }

        InlineMacro::CrossReference(xref) => {
            // Cross-reference - try to render as man page reference if it looks like one
            // e.g., git(1) -> \fBgit\fP(1)
            let w = visitor.writer_mut();
            if let Some(text) = &xref.text {
                // text is String, so we just write it escaped
                write!(w, "{}", manify(text, EscapeMode::Normalize))?;
            } else {
                // Try to format as man page reference
                let target = &xref.target;
                if let Some((name, vol)) = target.rsplit_once('(') {
                    if vol.ends_with(')') && vol.len() <= 3 {
                        write!(w, "\\fB{name}\\fP({vol}")?;
                    } else {
                        write!(w, "{target}")?;
                    }
                } else {
                    write!(w, "{target}")?;
                }
            }
        }

        InlineMacro::Footnote(footnote) => {
            // Footnotes - inline the content with a marker
            let w = visitor.writer_mut();
            write!(w, "[{}]", footnote.number)?;
        }

        InlineMacro::Image(img) => {
            // Inline image - show title as alt text
            if img.title.is_empty() {
                write!(visitor.writer_mut(), "[IMAGE]")?;
            } else {
                write!(visitor.writer_mut(), "[")?;
                visitor.visit_inline_nodes(&img.title)?;
                write!(visitor.writer_mut(), "]")?;
            }
        }

        InlineMacro::Icon(icon) => {
            // Icon - show target name in brackets
            let w = visitor.writer_mut();
            write!(w, "[{}]", icon.target)?;
        }

        InlineMacro::Keyboard(kbd) => {
            // Keyboard shortcut - render as bold
            let w = visitor.writer_mut();
            write!(w, "\\fB")?;
            for (i, key) in kbd.keys.iter().enumerate() {
                if i > 0 {
                    write!(w, "+")?;
                }
                write!(w, "{key}")?;
            }
            write!(w, "\\fP")?;
        }

        InlineMacro::Button(btn) => {
            // Button - render in brackets
            let w = visitor.writer_mut();
            write!(w, "[\\fB{}\\fP]", btn.label)?;
        }

        InlineMacro::Menu(menu) => {
            // Menu - render with arrows between items
            let w = visitor.writer_mut();
            for (i, item) in menu.items.iter().enumerate() {
                if i > 0 {
                    write!(w, " > ")?;
                }
                write!(w, "\\fB{item}\\fP")?;
            }
        }

        InlineMacro::Pass(pass) => {
            // Passthrough - write text directly (already processed)
            if let Some(text) = &pass.text {
                let w = visitor.writer_mut();
                write!(w, "{text}")?;
            }
        }

        InlineMacro::Stem(stem) => {
            // Math/stem - render as-is (no LaTeX support in roff)
            let w = visitor.writer_mut();
            write!(w, "{}", stem.content)?;
        }

        // Handle any future variants - skip unknown macros
        _ => {}
    }

    Ok(())
}
