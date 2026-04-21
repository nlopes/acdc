//! Inline node rendering for manpages.
//!
//! Handles bold, italic, monospace, links, and other inline formatting.

use std::io::Write;

use acdc_converters_core::{
    decode_numeric_char_refs,
    substitutions::Replacements,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{Autolink, InlineMacro, InlineNode, Link, Mailto};

use crate::{
    Error, ManpageVisitor,
    escape::{EscapeMode, manify},
};

impl<W: Write> ManpageVisitor<'_, W> {
    /// Visit an inline node.
    pub(crate) fn render_inline_node(&mut self, node: &InlineNode) -> Result<(), Error> {
        match node {
            InlineNode::PlainText(text) => {
                let content = if self.strip_next_leading_space {
                    self.strip_next_leading_space = false;
                    text.content.trim_start()
                } else {
                    text.content
                };
                let content = Replacements::unicode().transform(content, !self.in_inline_span);
                let escaped = manify(&content, EscapeMode::Normalize);
                let w = self.writer_mut();
                write!(w, "{escaped}")?;
            }

            InlineNode::RawText(text) => {
                // Raw text - decode numeric char refs for non-HTML output, then escape
                let decoded = decode_numeric_char_refs(text.content);
                let content = if self.strip_next_leading_space {
                    self.strip_next_leading_space = false;
                    decoded.trim_start()
                } else {
                    &decoded
                };
                let escaped = manify(content, EscapeMode::Normalize);
                let w = self.writer_mut();
                write!(w, "{escaped}")?;
            }

            InlineNode::VerbatimText(text) => {
                // Verbatim text - render as-is, preserve whitespace
                let escaped = manify(text.content, EscapeMode::Preserve);
                let w = self.writer_mut();
                write!(w, "{escaped}")?;
            }

            InlineNode::BoldText(bold) => {
                write!(self.writer_mut(), "\\fB")?;
                self.visit_inline_nodes(&bold.content)?;
                write!(self.writer_mut(), "\\fP")?;
            }

            InlineNode::ItalicText(italic) => {
                write!(self.writer_mut(), "\\fI")?;
                self.visit_inline_nodes(&italic.content)?;
                write!(self.writer_mut(), "\\fP")?;
            }

            InlineNode::MonospaceText(mono) => {
                // Monospace uses Courier font (matching asciidoctor's \f(CR)
                write!(self.writer_mut(), "\\f(CR")?;
                self.visit_inline_nodes(&mono.content)?;
                write!(self.writer_mut(), "\\fP")?;
            }

            InlineNode::HighlightText(highlight) => {
                // Highlight - render as bold (no highlighting in roff)
                write!(self.writer_mut(), "\\fB")?;
                self.visit_inline_nodes(&highlight.content)?;
                write!(self.writer_mut(), "\\fP")?;
            }

            InlineNode::SubscriptText(sub) => {
                // No subscript in roff - render in parentheses
                write!(self.writer_mut(), "_(")?;
                self.visit_inline_nodes(&sub.content)?;
                write!(self.writer_mut(), ")")?;
            }

            InlineNode::SuperscriptText(sup) => {
                // No superscript in roff - render in parentheses
                write!(self.writer_mut(), "^(")?;
                self.visit_inline_nodes(&sup.content)?;
                write!(self.writer_mut(), ")")?;
            }

            InlineNode::CurvedQuotationText(quoted) => {
                write!(self.writer_mut(), "\\(lq")?;
                self.visit_inline_nodes(&quoted.content)?;
                write!(self.writer_mut(), "\\(rq")?;
            }

            InlineNode::CurvedApostropheText(quoted) => {
                write!(self.writer_mut(), "\\(oq")?;
                self.visit_inline_nodes(&quoted.content)?;
                write!(self.writer_mut(), "\\(cq")?;
            }

            InlineNode::StandaloneCurvedApostrophe(_) => {
                let w = self.writer_mut();
                write!(w, "\\(cq")?;
            }

            InlineNode::LineBreak(_) => {
                let w = self.writer_mut();
                writeln!(w)?;
                writeln!(w, ".br")?;
            }

            InlineNode::InlineAnchor(anchor) => {
                // Anchors have no visible representation in man pages
                // But we can add a comment for reference
                let w = self.writer_mut();
                writeln!(w, r#".\" anchor: {}"#, anchor.id)?;
            }

            InlineNode::Macro(inline_macro) => {
                self.render_inline_macro(inline_macro)?;
            }

            InlineNode::CalloutRef(callout) => {
                // Render callout reference in manpage format: <N>
                let w = self.writer_mut();
                write!(w, "\\fB({})\\fP", callout.number)?;
            }

            // Handle any future variants - skip unknown nodes
            _ => {}
        }

        Ok(())
    }

    fn render_link(&mut self, link: &Link) -> Result<(), Error> {
        // Use .URL macro for links (matching asciidoctor)
        // The macro must be on its own line; continuation text goes on the next line
        let target_str = link.target.to_string();
        let escaped_target = manify(&target_str, EscapeMode::Normalize);
        let display_text = if link.text.is_empty() {
            String::new()
        } else {
            let mut buf = Vec::new();
            let processor = self.processor.clone();
            let mut text_visitor = ManpageVisitor::new(&mut buf, processor);
            text_visitor.visit_inline_nodes(&link.text)?;
            String::from_utf8_lossy(&buf).trim().to_string()
        };
        let w = self.writer_mut();
        writeln!(w, "\\c\n.URL \"{escaped_target}\" \"{display_text}\" \"\"")?;
        self.strip_next_leading_space = true;

        Ok(())
    }

    fn render_mailto(&mut self, mailto: &Mailto) -> Result<(), Error> {
        self.write_mailto_with_trailing(mailto, "")
    }

    /// Write a mailto macro with explicit trailing punctuation.
    ///
    /// This is called from the manpage visitor's `visit_inline_nodes` when it detects
    /// an explicit mailto macro followed by non-whitespace punctuation. The trailing
    /// punctuation is passed to the `.MTO` macro's third argument.
    pub(crate) fn write_mailto_with_trailing(
        &mut self,
        mailto: &Mailto,
        trailing: &str,
    ) -> Result<(), Error> {
        let target_str = mailto.target.to_string();
        let email = target_str
            .strip_prefix("mailto:")
            .unwrap_or(&target_str)
            .replace('@', "\\(at");

        let display_text = if mailto.text.is_empty() {
            String::new()
        } else {
            let mut buf = Vec::new();
            let processor = self.processor.clone();
            let mut text_visitor = ManpageVisitor::new(&mut buf, processor);
            text_visitor.visit_inline_nodes(&mailto.text)?;
            String::from_utf8_lossy(&buf).trim().to_string()
        };

        let w = self.writer_mut();
        writeln!(w, "\\c\n.MTO \"{email}\" \"{display_text}\" \"{trailing}\"")?;
        self.strip_next_leading_space = true;
        Ok(())
    }

    fn render_autolink(&mut self, autolink: &Autolink) -> Result<(), Error> {
        self.write_autolink_with_trailing(autolink, "")
    }

    /// Write an autolink with explicit trailing punctuation.
    ///
    /// This is called from the manpage visitor's `visit_inline_nodes` when it detects
    /// a mailto autolink followed by single-character punctuation. The trailing
    /// punctuation is passed to the `.MTO` macro's third argument.
    pub(crate) fn write_autolink_with_trailing(
        &mut self,
        autolink: &Autolink,
        trailing: &str,
    ) -> Result<(), Error> {
        let url_str = autolink.url.to_string();
        // Use .MTO macro for mailto autolinks
        // The macro must end with newline; continuation text goes on the next line
        if let Some(email) = url_str.strip_prefix("mailto:") {
            let escaped_email = email.replace('@', "\\(at");
            let w = self.writer_mut();
            writeln!(w, "\\c\n.MTO \"{escaped_email}\" \"\" \"{trailing}\"")?;
        } else {
            // Use .URL macro for HTTP(S) links
            let w = self.writer_mut();
            writeln!(
                w,
                "\\c\n.URL \"{}\" \"\" \"{trailing}\"",
                manify(&url_str, EscapeMode::Normalize)
            )?;
        }
        self.strip_next_leading_space = true;
        Ok(())
    }

    /// Visit an inline macro.
    fn render_inline_macro(&mut self, macro_node: &InlineMacro) -> Result<(), Error> {
        match macro_node {
            InlineMacro::Url(_)
            | InlineMacro::Mailto(_)
            | InlineMacro::Link(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::CrossReference(_) => {
                self.render_url_inline_macro(macro_node)?;
            }

            InlineMacro::Footnote(footnote) => {
                let w = self.writer_mut();
                write!(w, "[{}]", footnote.number)?;
            }

            InlineMacro::Image(_)
            | InlineMacro::Icon(_)
            | InlineMacro::Keyboard(_)
            | InlineMacro::Button(_)
            | InlineMacro::Menu(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_)
            | InlineMacro::IndexTerm(_) => {
                self.render_ui_inline_macro(macro_node)?;
            }

            // Handle any future variants - skip unknown macros
            _ => {}
        }

        Ok(())
    }

    /// Render URL-like inline macros: url, mailto, link, autolink, cross-reference.
    fn render_url_inline_macro(&mut self, macro_node: &InlineMacro) -> Result<(), Error> {
        match macro_node {
            InlineMacro::Url(url) => {
                // URL - use .URL macro for proper rendering (matching asciidoctor)
                // The macro must end with newline; continuation text goes on the next line
                let target_str = url.target.to_string();
                let escaped_target = manify(&target_str, EscapeMode::Normalize);
                if url.text.is_empty() {
                    let w = self.writer_mut();
                    writeln!(w, "\\c\n.URL \"{escaped_target}\" \"\" \"\"")?;
                } else {
                    // Render text to a buffer for the .URL macro
                    let mut buf = Vec::new();
                    let processor = self.processor.clone();
                    let mut text_visitor = ManpageVisitor::new(&mut buf, processor);
                    text_visitor.visit_inline_nodes(&url.text)?;
                    let display_text = String::from_utf8_lossy(&buf).trim().to_string();
                    let w = self.writer_mut();
                    writeln!(w, "\\c\n.URL \"{escaped_target}\" \"{display_text}\" \"\"")?;
                }
                self.strip_next_leading_space = true;
            }

            InlineMacro::Mailto(mailto) => {
                self.render_mailto(mailto)?;
            }

            InlineMacro::Link(link) => {
                self.render_link(link)?;
            }

            InlineMacro::Autolink(autolink) => {
                self.render_autolink(autolink)?;
            }

            InlineMacro::CrossReference(xref) => {
                // Cross-reference - try to render as man page reference if it looks like one
                // e.g., git(1) -> \fBgit\fP(1)
                if xref.text.is_empty() {
                    // Try to format as man page reference
                    let w = self.writer_mut();
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
                } else {
                    // Render inline nodes recursively
                    self.visit_inline_nodes(&xref.text)?;
                }
            }

            InlineMacro::Footnote(_)
            | InlineMacro::Icon(_)
            | InlineMacro::Image(_)
            | InlineMacro::Keyboard(_)
            | InlineMacro::Button(_)
            | InlineMacro::Menu(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_)
            | InlineMacro::IndexTerm(_)
            | _ => {}
        }
        Ok(())
    }

    /// Render UI-element inline macros: image, icon, keyboard, button, menu, pass, stem, index-term.
    fn render_ui_inline_macro(&mut self, macro_node: &InlineMacro) -> Result<(), Error> {
        match macro_node {
            InlineMacro::Image(img) => {
                // Inline image - show title as alt text
                if img.title.is_empty() {
                    write!(self.writer_mut(), "[IMAGE]")?;
                } else {
                    write!(self.writer_mut(), "[")?;
                    self.visit_inline_nodes(&img.title)?;
                    write!(self.writer_mut(), "]")?;
                }
            }

            InlineMacro::Icon(icon) => {
                // Icon - show target name in brackets
                let w = self.writer_mut();
                write!(w, "[{}]", icon.target)?;
            }

            InlineMacro::Keyboard(kbd) => {
                // Keyboard shortcut - render as bold
                let w = self.writer_mut();
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
                let w = self.writer_mut();
                write!(w, "[\\fB{}\\fP]", btn.label)?;
            }

            InlineMacro::Menu(menu) => {
                // Menu - render target and items with arrows between them
                let w = self.writer_mut();
                write!(w, "\\fB{}\\fP", menu.target)?;
                for item in &menu.items {
                    write!(w, " \\(ra \\fB{item}\\fP")?;
                }
            }

            InlineMacro::Pass(pass) => {
                // Passthrough - write text directly (already processed)
                if let Some(text) = &pass.text {
                    let w = self.writer_mut();
                    write!(w, "{text}")?;
                }
            }

            InlineMacro::Stem(stem) => {
                // Math/stem - render as-is (no LaTeX support in roff)
                let w = self.writer_mut();
                write!(w, "{}", stem.content)?;
            }

            InlineMacro::IndexTerm(it) => {
                // Flow terms (visible): output the term text
                // Concealed terms (hidden): output nothing
                if it.is_visible() {
                    let w = self.writer_mut();
                    write!(w, "{}", manify(it.term(), EscapeMode::Normalize))?;
                }
                // Concealed terms produce no output - they're only for index generation
            }

            InlineMacro::Footnote(_)
            | InlineMacro::Url(_)
            | InlineMacro::Link(_)
            | InlineMacro::Mailto(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::CrossReference(_)
            | _ => {}
        }
        Ok(())
    }
}
