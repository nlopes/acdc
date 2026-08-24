//! Inline node rendering for manpages.
//!
//! Handles bold, italic, monospace, links, and other inline formatting.

use std::{borrow::Cow, io::Write, rc::Rc};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::apply_replacements;
use acdc_converters_core::{
    decode_numeric_char_refs,
    substitutions::{Replacements, TextBoundaries},
    visitor::{Visitor, WritableVisitor},
    xref::{XrefDisplay, resolve_xref},
};
use acdc_parser::{Autolink, CrossReference, InlineMacro, InlineNode, Link, Mailto};

use crate::{
    Error, ManpageVisitor,
    escape::{EscapeMode, manify, uppercase_title},
    manpage_visitor::TextCase,
};

fn replacements() -> Replacements<'static> {
    let mut replacements = Replacements::unicode();
    replacements.em_dash_spaced = " \u{2014} ";
    replacements.em_dash_word_bounded = "\u{2014}";
    replacements
}

/// Apply manpage typography replacements to a `PlainText` leaf.
///
/// When `pre-spec-subs` is enabled, defers to
/// [`apply_replacements`](acdc_converters_core::substitutions::apply_replacements)
/// so that `[subs="-replacements"]` can suppress the transform. Otherwise,
/// always applies — matching the asciidoctor default.
#[cfg(feature = "pre-spec-subs")]
fn transform_plain<'a>(
    text: &'a str,
    visitor: &ManpageVisitor<'_, '_, impl Write>,
    text_boundaries: TextBoundaries,
) -> Cow<'a, str> {
    apply_replacements(
        text,
        visitor.processor.current_subs.get(),
        &replacements(),
        text_boundaries,
    )
}

#[cfg(not(feature = "pre-spec-subs"))]
fn transform_plain<'a>(
    text: &'a str,
    _visitor: &ManpageVisitor<'_, '_, impl Write>,
    text_boundaries: TextBoundaries,
) -> Cow<'a, str> {
    Cow::Owned(replacements().transform(text, text_boundaries))
}

/// Apply the casing the surrounding context asks for, keeping inline markup.
///
/// The uppercase rule is the one in [`uppercase_title`], which `.SH` lines use:
/// a reference to a level-1 section reads as that section's heading.
fn apply_text_case(content: Cow<'_, str>, text_case: TextCase) -> Cow<'_, str> {
    match text_case {
        TextCase::Preserve => content,
        TextCase::Uppercase => Cow::Owned(uppercase_title(&content)),
    }
}

fn restore_em_dash_line_prefixes(content: &str, escaped: &str) -> Option<String> {
    // `manify` removes indentation after a newline. Preserve the source space
    // that separates a line-leading em dash from the following word in roff.
    if !content
        .split('\n')
        .any(|line| line.starts_with(" \u{2014}"))
    {
        return None;
    }

    let mut restored = String::with_capacity(escaped.len() + 1);
    for (index, (content_line, escaped_line)) in
        content.split('\n').zip(escaped.split('\n')).enumerate()
    {
        if index > 0 {
            restored.push('\n');
        }
        if content_line.starts_with(" \u{2014}") && !escaped_line.starts_with(' ') {
            restored.push(' ');
        }
        restored.push_str(escaped_line);
    }
    Some(restored)
}

impl<W: Write> ManpageVisitor<'_, '_, W> {
    fn render_plain_text(&mut self, text: &str) -> Result<(), Error> {
        let content = if self.strip_next_leading_space {
            self.strip_next_leading_space = false;
            text.trim_start()
        } else {
            text
        };
        let mut content = transform_plain(content, self, self.text_boundaries);
        if self.text_boundaries.at_paragraph_end() && text.ends_with("--") && content.ends_with(' ')
        {
            content.to_mut().pop();
        }
        let content = apply_text_case(content, self.text_case);
        let escaped = manify(&content, EscapeMode::Normalize);
        let w = self.writer_mut();
        if let Some(restored) = restore_em_dash_line_prefixes(&content, &escaped) {
            write!(w, "{restored}")?;
        } else {
            write!(w, "{escaped}")?;
        }
        Ok(())
    }

    /// Visit an inline node.
    pub(crate) fn render_inline_node(&mut self, node: &InlineNode) -> Result<(), Error> {
        match node {
            InlineNode::PlainText(text) => self.render_plain_text(text.content)?,

            InlineNode::RawText(text) => {
                // Raw text - decode numeric char refs for non-HTML output, then escape
                let decoded = decode_numeric_char_refs(text.content);
                let content = if self.strip_next_leading_space {
                    self.strip_next_leading_space = false;
                    decoded.trim_start()
                } else {
                    &decoded
                };
                let content = apply_text_case(Cow::Borrowed(content), self.text_case);
                let escaped = manify(&content, EscapeMode::Normalize);
                let w = self.writer_mut();
                write!(w, "{escaped}")?;
            }

            InlineNode::VerbatimText(text) => {
                // Verbatim text - render as-is, preserve whitespace
                let content = apply_text_case(Cow::Borrowed(text.content), self.text_case);
                let escaped = manify(&content, EscapeMode::Preserve);
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
            let mut text_visitor = self.nested_visitor(&mut buf);
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
            let mut text_visitor = self.nested_visitor(&mut buf);
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
                    let mut text_visitor = self.nested_visitor(&mut buf);
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
                self.render_cross_reference(xref)?;
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

    fn render_cross_reference(&mut self, xref: &CrossReference<'_>) -> Result<(), Error> {
        if !xref.text.is_empty() {
            return self.visit_inline_nodes(&xref.text);
        }

        // Clone the handles so the borrowed reference text and the resolution
        // guard both outlive the `&mut self` render calls.
        let references = Rc::clone(&self.processor.references);
        let guard = self.processor.xref_guard.clone();
        match resolve_xref(references.get(xref.target), xref, &guard) {
            // A reference to a level-1 section reads as that section's `.SH`
            // heading, which manpages upper-case. An explicit label reads as
            // written.
            XrefDisplay::Title(inlines, _scope) => {
                let text_case = if self.processor.top_level_section_ids.contains(xref.target) {
                    TextCase::Uppercase
                } else {
                    TextCase::Preserve
                };
                self.with_text_case(text_case, |visitor| visitor.visit_inline_nodes(inlines))
            }
            XrefDisplay::Label(inlines, _scope) => self.visit_inline_nodes(inlines),
            XrefDisplay::ShortCaption(prefix) => {
                let text = manify(&prefix, EscapeMode::Normalize);
                write!(self.writer_mut(), "{text}")?;
                Ok(())
            }
            XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                let prefix = manify(&prefix, EscapeMode::Normalize);
                let separator = manify(", “", EscapeMode::Normalize);
                write!(self.writer_mut(), "{prefix}{separator}")?;
                self.visit_inline_nodes(inlines)?;
                let closing_quote = manify("”", EscapeMode::Normalize);
                write!(self.writer_mut(), "{closing_quote}")?;
                Ok(())
            }
            XrefDisplay::Fallback(text)
            | XrefDisplay::Unresolved(text)
            | XrefDisplay::Nested(text) => {
                let text = manify(&text, EscapeMode::Normalize);
                write!(self.writer_mut(), "{text}")?;
                Ok(())
            }
            XrefDisplay::External(target) => {
                let fallback = format!("[{target}]");
                let text = manify(&fallback, EscapeMode::Normalize);
                write!(self.writer_mut(), "{text}")?;
                Ok(())
            }
        }
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
                let alt = acdc_converters_core::icon::alt(&icon.target, &icon.attributes);
                write!(self.writer_mut(), "[{alt}]")?;
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
                if it.is_visible() {
                    self.visit_inline_nodes(it.term())?;
                }
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
