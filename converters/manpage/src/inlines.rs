//! Inline node rendering for manpages.
//!
//! Handles bold, italic, monospace, links, and other inline formatting.

use std::{borrow::Cow, io::Write, rc::Rc};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::apply_replacements;
use acdc_converters_core::{
    decode_numeric_char_refs,
    link::{link_fallback, mailto_fallback},
    substitutions::{Replacements, TextBoundaries},
    visitor::{Visitor, WritableVisitor},
    xref::{XrefDisplay, resolve_xref},
};
use acdc_parser::{
    Autolink, CrossReference, ElementAttributes, InlineMacro, InlineNode, Link, Mailto,
};

use crate::{
    Error, ManpageVisitor,
    escape::{
        EscapeMode, escape_rendered_roff_macro_argument, escape_roff_macro_argument, manify,
        uppercase_title,
    },
    manpage_visitor::TextCase,
};

fn replacements() -> Replacements<'static> {
    let mut replacements = Replacements::unicode();
    replacements.em_dash_spaced = " \u{2014} ";
    replacements.em_dash_word_bounded = "\u{2014}";
    replacements
}

#[derive(Clone, Copy)]
enum RoleDefault {
    Plain,
    Highlight,
}

fn role_affixes(role: Option<&str>, default: RoleDefault) -> (String, String) {
    let mut prefix = String::new();
    let mut closings = Vec::new();

    for role in role.into_iter().flat_map(str::split_whitespace) {
        let (opening, closing) = match role {
            "underline" | "subtitle" => ("\\fI", "\\fP"),
            "line-through" => ("[deleted: ", "]"),
            "overline" => ("[overlined: ", "]"),
            "big" => ("\\s+1", "\\s-1"),
            "small" => ("\\s-1", "\\s+1"),
            "highlight" => ("\\fB", "\\fP"),
            _ => continue,
        };
        prefix.push_str(opening);
        closings.push(closing);
    }

    if prefix.is_empty() && role.is_none() && matches!(default, RoleDefault::Highlight) {
        prefix.push_str("\\fB");
        closings.push("\\fP");
    }

    let suffix = closings.into_iter().rev().collect();
    (prefix, suffix)
}

fn role_from_attributes<'a>(attributes: &ElementAttributes<'a>) -> Option<Cow<'a, str>> {
    attributes.get_string("role")
}

fn has_degraded_role(role: Option<&str>) -> bool {
    role.into_iter()
        .flat_map(str::split_whitespace)
        .any(|role| {
            !matches!(
                role,
                "underline" | "subtitle" | "big" | "small" | "highlight"
            )
        })
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
    fn render_with_role(
        &mut self,
        role: Option<&str>,
        default: RoleDefault,
        content: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if has_degraded_role(role) && !self.processor.inline_role_warning.replace(true) {
            self.diagnostics.warn_with_advice(
                "some inline roles have no exact portable roff styling; rendering textual or plain fallbacks",
                "Use an HTML-capable backend when exact role styling is required.",
            );
        }
        let (prefix, suffix) = role_affixes(role, default);
        write!(self.writer_mut(), "{prefix}")?;
        content(self)?;
        write!(self.writer_mut(), "{suffix}")?;
        Ok(())
    }

    fn render_link_display(
        &mut self,
        text: &[InlineNode<'_>],
        fallback: &str,
        role: Option<&str>,
    ) -> Result<String, Error> {
        let mut output = Vec::new();
        {
            let mut visitor = self.nested_visitor(&mut output);
            visitor.render_with_role(role, RoleDefault::Plain, |visitor| {
                if text.is_empty() {
                    write!(
                        visitor.writer_mut(),
                        "{}",
                        manify(fallback, EscapeMode::Normalize)
                    )?;
                } else {
                    visitor.visit_inline_nodes(text)?;
                }
                Ok(())
            })?;
        }
        let rendered = String::from_utf8_lossy(&output).trim().to_string();
        Ok(escape_rendered_roff_macro_argument(&rendered).into_owned())
    }

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
                let default = if highlight.id.is_some() {
                    RoleDefault::Plain
                } else {
                    RoleDefault::Highlight
                };
                self.render_with_role(highlight.role, default, |visitor| {
                    visitor.visit_inline_nodes(&highlight.content)
                })?;
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
        let escaped_target = escape_roff_macro_argument(&target_str);
        let role = role_from_attributes(&link.attributes);
        let styled_fallback = !role_affixes(role.as_deref(), RoleDefault::Plain)
            .0
            .is_empty();
        let display_text = if link.text.is_empty() && !link.hides_uri_scheme() && !styled_fallback {
            String::new()
        } else {
            self.render_link_display(
                &link.text,
                link_fallback(&target_str, link.hides_uri_scheme()),
                role.as_deref(),
            )?
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
        let email =
            escape_roff_macro_argument(target_str.strip_prefix("mailto:").unwrap_or(&target_str))
                .replace('@', "\\(at");

        let role = role_from_attributes(&mailto.attributes);
        let styled_fallback = !role_affixes(role.as_deref(), RoleDefault::Plain)
            .0
            .is_empty();
        let display_text = if mailto.text.is_empty() && !styled_fallback {
            String::new()
        } else {
            self.render_link_display(&mailto.text, mailto_fallback(&target_str), role.as_deref())?
        };

        let trailing = escape_rendered_roff_macro_argument(trailing);
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
            let escaped_email = escape_roff_macro_argument(email).replace('@', "\\(at");
            let trailing = escape_rendered_roff_macro_argument(trailing);
            let w = self.writer_mut();
            writeln!(w, "\\c\n.MTO \"{escaped_email}\" \"\" \"{trailing}\"")?;
        } else {
            // Use .URL macro for HTTP(S) links
            let display_text = if autolink.hides_uri_scheme() {
                escape_roff_macro_argument(link_fallback(&url_str, autolink.hides_uri_scheme()))
            } else {
                String::new()
            };
            let target = escape_roff_macro_argument(&url_str);
            let trailing = escape_rendered_roff_macro_argument(trailing);
            let w = self.writer_mut();
            writeln!(
                w,
                "\\c\n.URL \"{target}\" \"{display_text}\" \"{trailing}\"",
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
                let escaped_target = escape_roff_macro_argument(&target_str);
                let role = role_from_attributes(&url.attributes);
                let styled_fallback = !role_affixes(role.as_deref(), RoleDefault::Plain)
                    .0
                    .is_empty();
                if url.text.is_empty() && !url.hides_uri_scheme() && !styled_fallback {
                    let w = self.writer_mut();
                    writeln!(w, "\\c\n.URL \"{escaped_target}\" \"\" \"\"")?;
                } else {
                    let display_text = self.render_link_display(
                        &url.text,
                        link_fallback(&target_str, url.hides_uri_scheme()),
                        role.as_deref(),
                    )?;
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
        let target = xref.target;
        match resolve_xref(references.get(target), xref, &guard) {
            // A reference to a level-1 section reads as that section's `.SH`
            // heading, which manpages upper-case. An explicit label reads as
            // written.
            XrefDisplay::Title(inlines, _scope) => {
                let text_case = if self.processor.top_level_section_ids.contains(target) {
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
                self.render_inline_image(img)?;
            }

            InlineMacro::Icon(icon) => {
                let alt = acdc_converters_core::icon::alt(&icon.target, &icon.attributes);
                let alt = manify(&alt, EscapeMode::Collapse);
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
                    let key = manify(key, EscapeMode::Collapse);
                    write!(w, "{key}")?;
                }
                write!(w, "\\fP")?;
            }

            InlineMacro::Button(btn) => {
                // Button - render in brackets
                let label = manify(btn.label, EscapeMode::Collapse);
                let w = self.writer_mut();
                write!(w, "[\\fB{label}\\fP]")?;
            }

            InlineMacro::Menu(menu) => {
                // Menu - render target and items with arrows between them
                let target = manify(menu.target, EscapeMode::Collapse);
                let w = self.writer_mut();
                write!(w, "\\fB{target}\\fP")?;
                for item in &menu.items {
                    let item = manify(item, EscapeMode::Collapse);
                    write!(w, " \\(ra \\fB{item}\\fP")?;
                }
            }

            InlineMacro::Pass(pass) => {
                // Passthrough content is backend-native and intentionally bypasses escaping.
                if let Some(text) = &pass.text {
                    let w = self.writer_mut();
                    write!(w, "{text}")?;
                }
            }

            InlineMacro::Stem(stem) => {
                let content = manify(stem.content, EscapeMode::Collapse);
                let w = self.writer_mut();
                write!(w, "{content}")?;
            }

            InlineMacro::IndexTerm(it) => {
                self.render_index_term(it)?;
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
