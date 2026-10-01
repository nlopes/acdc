//! Inline node rendering for manpages.
//!
//! Handles bold, italic, monospace, links, and other inline formatting.

use std::{borrow::Cow, io::Write, rc::Rc};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::apply_replacements;
use acdc_converters_core::{
    TraversalContext, decode_numeric_char_refs,
    link::{link_fallback, mailto_fallback, mailto_target},
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

pub(crate) struct LinkLabel {
    command: &'static str,
    target: String,
    pub(crate) content: Vec<u8>,
    split: bool,
}

fn is_empty_formatting(node: &InlineNode<'_>) -> bool {
    let content = match node {
        InlineNode::BoldText(n) => &n.content,
        InlineNode::ItalicText(n) => &n.content,
        InlineNode::MonospaceText(n) => &n.content,
        InlineNode::HighlightText(n) => &n.content,
        InlineNode::SubscriptText(n) => &n.content,
        InlineNode::SuperscriptText(n) => &n.content,
        // Curved quotes and other inline nodes can contribute visible text
        // or converter effects even when they have no child content.
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        | _ => return false,
    };
    content.iter().all(is_empty_formatting)
}

pub(crate) fn contains_link(node: &InlineNode<'_>) -> bool {
    let children = match node {
        InlineNode::Macro(
            InlineMacro::Link(_)
            | InlineMacro::Url(_)
            | InlineMacro::Mailto(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::CrossReference(_),
        ) => return true,
        InlineNode::Macro(InlineMacro::Image(image)) => {
            return image.metadata.attributes.contains_key("link");
        }
        InlineNode::BoldText(n) => &n.content,
        InlineNode::ItalicText(n) => &n.content,
        InlineNode::MonospaceText(n) => &n.content,
        InlineNode::HighlightText(n) => &n.content,
        InlineNode::SubscriptText(n) => &n.content,
        InlineNode::SuperscriptText(n) => &n.content,
        InlineNode::CurvedQuotationText(n) => &n.content,
        InlineNode::CurvedApostropheText(n) => &n.content,
        InlineNode::Macro(InlineMacro::IndexTerm(n)) if n.is_visible() => n.term(),
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        | _ => return false,
    };
    children.iter().any(contains_link)
}

impl<'a, W: Write> ManpageVisitor<'a, '_, W> {
    fn flush_link_label(&mut self, trailing: &str, finish: bool) -> Result<(), Error> {
        if let Some(label) = self.link_label.as_mut() {
            let text = String::from_utf8_lossy(&label.content);
            let text = escape_rendered_roff_macro_argument(text.trim());
            let trailing = escape_rendered_roff_macro_argument(trailing);
            // A child command must never become part of its parent's quoted argument.
            if !text.is_empty() || (finish && !label.split) {
                writeln!(
                    self.writer,
                    "\\c\n.{} \"{}\" \"{text}\" \"{trailing}\"",
                    label.command, label.target
                )?;
            } else if finish {
                write!(self.writer, "{trailing}")?;
            }
            label.content.clear();
            label.split = true;
        }
        Ok(())
    }

    fn with_link_label(
        &mut self,
        target: String,
        mailto: bool,
        trailing: &str,
        render: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.flush_link_label("", false)?;
        let previous = self.link_label.replace(LinkLabel {
            command: if mailto { "MTO" } else { "URL" },
            target,
            content: Vec::new(),
            split: false,
        });
        let boundaries = self.text_boundaries;
        let in_span = self.in_inline_span;
        self.strip_next_leading_space = false;
        self.text_boundaries = TextBoundaries::BOTH;
        self.in_inline_span = false;
        let result = render(self).and_then(|()| self.flush_link_label(trailing, true));
        self.link_label = previous;
        self.text_boundaries = boundaries;
        self.in_inline_span = in_span;
        self.strip_next_leading_space = true;
        result
    }

    pub(crate) fn write_link_command(
        &mut self,
        command: &str,
        target: &str,
        label: &str,
        trailing: &str,
    ) -> Result<(), Error> {
        self.flush_link_label("", false)?;
        writeln!(
            self.writer,
            "\\c\n.{command} \"{target}\" \"{label}\" \"{trailing}\""
        )?;
        self.strip_next_leading_space = true;
        Ok(())
    }

    fn render_with_role(
        &mut self,
        role: Option<&str>,
        default: RoleDefault,
        split: bool,
        content: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if has_degraded_role(role) && !self.processor.inline_role_warning.replace(true) {
            self.diagnostics.warn_with_advice(
                "some inline roles have no exact portable roff styling; rendering textual or plain fallbacks",
                "Use an HTML-capable backend when exact role styling is required.",
            );
        }
        let (prefix, suffix) = role_affixes(role, default);
        self.render_affixed(&prefix, &suffix, split, content)
    }

    fn render_affixed(
        &mut self,
        prefix: &str,
        suffix: &str,
        split: bool,
        content: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        // Formatting around several links belongs outside their command arguments.
        if split {
            self.flush_link_label("", false)?;
            write!(self.writer, "{prefix}")?;
        } else {
            write!(self.writer_mut(), "{prefix}")?;
        }
        content(self)?;
        if split {
            self.flush_link_label("", false)?;
            write!(self.writer, "{suffix}")?;
        } else {
            write!(self.writer_mut(), "{suffix}")?;
        }
        Ok(())
    }

    fn render_formatted_inlines(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        text: &[InlineNode<'_>],
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Error> {
        let split = self.link_label.is_some() && text.iter().any(contains_link);
        self.render_affixed(prefix, suffix, split, |visitor| {
            visitor.visit_inline_nodes(traversal, text)
        })
    }

    fn render_link_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        text: &[InlineNode<'_>],
        fallback: &str,
        role: Option<&str>,
    ) -> Result<(), Error> {
        let split = text.iter().any(contains_link);
        self.render_with_role(role, RoleDefault::Plain, split, |visitor| {
            if text.is_empty() {
                write!(
                    visitor.writer_mut(),
                    "{}",
                    manify(fallback, EscapeMode::Normalize)
                )?;
            } else {
                visitor.visit_inline_nodes(traversal, text)?;
            }
            Ok(())
        })
    }

    fn render_plain_text(&mut self, text: &str) -> Result<(), Error> {
        let content = if self.strip_next_leading_space {
            self.strip_next_leading_space = false;
            text.trim_start_matches(|character: char| character.is_ascii_whitespace())
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
    pub(crate) fn render_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: &InlineNode,
    ) -> Result<(), Error> {
        match node {
            InlineNode::PlainText(text) => self.render_plain_text(text.content)?,

            InlineNode::RawText(text) => {
                // Raw text - decode numeric char refs for non-HTML output, then escape
                let decoded = decode_numeric_char_refs(text.content);
                let content = if self.strip_next_leading_space {
                    self.strip_next_leading_space = false;
                    decoded.trim_start_matches(|character: char| character.is_ascii_whitespace())
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
                self.render_formatted_inlines(traversal, &bold.content, "\\fB", "\\fP")?;
            }

            InlineNode::ItalicText(italic) => {
                self.render_formatted_inlines(traversal, &italic.content, "\\fI", "\\fP")?;
            }

            InlineNode::MonospaceText(mono) => {
                self.render_formatted_inlines(traversal, &mono.content, "\\f(CR", "\\fP")?;
            }

            InlineNode::HighlightText(highlight) => {
                let default = if highlight.id.is_some() {
                    RoleDefault::Plain
                } else {
                    RoleDefault::Highlight
                };
                let split =
                    self.link_label.is_some() && highlight.content.iter().any(contains_link);
                self.render_with_role(highlight.role, default, split, |visitor| {
                    visitor.visit_inline_nodes(traversal, &highlight.content)
                })?;
            }

            InlineNode::SubscriptText(sub) => {
                if !sub.content.iter().all(is_empty_formatting) {
                    self.render_formatted_inlines(traversal, &sub.content, "_(", ")")?;
                }
            }

            InlineNode::SuperscriptText(sup) => {
                if !sup.content.iter().all(is_empty_formatting) {
                    self.render_formatted_inlines(traversal, &sup.content, "^(", ")")?;
                }
            }

            InlineNode::CurvedQuotationText(quoted) => {
                self.render_formatted_inlines(traversal, &quoted.content, "\\(lq", "\\(rq")?;
            }

            InlineNode::CurvedApostropheText(quoted) => {
                self.render_formatted_inlines(traversal, &quoted.content, "\\(oq", "\\(cq")?;
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

            // A roff comment here can start mid-line and print its leading dot.
            // References use the parser's catalog; the target needs no output.
            InlineNode::InlineAnchor(_) => {}

            InlineNode::Macro(inline_macro) => {
                self.render_inline_macro(traversal, inline_macro)?;
            }

            InlineNode::CalloutRef(callout) => {
                // Render callout reference in manpage format: <N>
                let w = self.writer_mut();
                write!(w, "\\fB({})\\fP", callout.number)?;
            }

            _ => self.warn_unsupported_parser_variant("inline node"),
        }

        Ok(())
    }

    fn render_link(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        link: &Link,
    ) -> Result<(), Error> {
        let target = link.target.to_string();
        let role = role_from_attributes(&link.attributes);
        self.with_link_label(escape_roff_macro_argument(&target), false, "", |visitor| {
            let styled = !role_affixes(role.as_deref(), RoleDefault::Plain)
                .0
                .is_empty();
            if !link.text.is_empty() || link.hides_uri_scheme() || styled {
                visitor.render_link_content(
                    traversal,
                    &link.text,
                    link_fallback(&target, link.hides_uri_scheme()),
                    role.as_deref(),
                )?;
            }
            Ok(())
        })
    }

    fn render_mailto(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        mailto: &Mailto,
    ) -> Result<(), Error> {
        self.write_mailto_with_trailing(traversal, mailto, "")
    }

    /// Write a mailto macro with explicit trailing punctuation.
    ///
    /// This is called from the manpage visitor's `visit_inline_nodes` when it detects
    /// an explicit mailto macro followed by non-whitespace punctuation. The trailing
    /// punctuation is passed to the `.MTO` macro's third argument.
    pub(crate) fn write_mailto_with_trailing(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        mailto: &Mailto,
        trailing: &str,
    ) -> Result<(), Error> {
        let target = mailto.target.to_string();
        let destination = mailto_target(mailto);
        let email = escape_roff_macro_argument(mailto_fallback(&destination)).replace('@', "\\(at");
        let role = role_from_attributes(&mailto.attributes);
        self.with_link_label(email, true, trailing, |visitor| {
            let styled = !role_affixes(role.as_deref(), RoleDefault::Plain)
                .0
                .is_empty();
            if !mailto.text.is_empty() || styled || destination != target {
                visitor.render_link_content(
                    traversal,
                    &mailto.text,
                    mailto_fallback(&target),
                    role.as_deref(),
                )?;
            }
            Ok(())
        })
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
        let target = autolink.url.to_string();
        let trailing = escape_rendered_roff_macro_argument(trailing);
        if let Some(email) = target.strip_prefix("mailto:") {
            let email = escape_roff_macro_argument(email).replace('@', "\\(at");
            self.write_link_command("MTO", &email, "", &trailing)
        } else {
            let label = if autolink.hides_uri_scheme() {
                escape_roff_macro_argument(link_fallback(&target, true))
            } else {
                String::new()
            };
            let target = escape_roff_macro_argument(&target);
            self.write_link_command("URL", &target, &label, &trailing)
        }
    }

    /// Visit an inline macro.
    fn render_inline_macro(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        macro_node: &InlineMacro,
    ) -> Result<(), Error> {
        match macro_node {
            InlineMacro::Url(_)
            | InlineMacro::Mailto(_)
            | InlineMacro::Link(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::CrossReference(_) => {
                self.render_url_inline_macro(traversal, macro_node)?;
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
                self.render_ui_inline_macro(traversal, macro_node)?;
            }

            _ => self.warn_unsupported_parser_variant("inline macro"),
        }

        Ok(())
    }

    /// Render URL-like inline macros: url, mailto, link, autolink, cross-reference.
    fn render_url_inline_macro(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        macro_node: &InlineMacro,
    ) -> Result<(), Error> {
        match macro_node {
            InlineMacro::Url(url) => {
                let target = url.target.to_string();
                let role = role_from_attributes(&url.attributes);
                self.with_link_label(escape_roff_macro_argument(&target), false, "", |visitor| {
                    let styled = !role_affixes(role.as_deref(), RoleDefault::Plain)
                        .0
                        .is_empty();
                    if !url.text.is_empty() || url.hides_uri_scheme() || styled {
                        visitor.render_link_content(
                            traversal,
                            &url.text,
                            link_fallback(&target, url.hides_uri_scheme()),
                            role.as_deref(),
                        )?;
                    }
                    Ok(())
                })?;
            }

            InlineMacro::Mailto(mailto) => {
                self.render_mailto(traversal, mailto)?;
            }

            InlineMacro::Link(link) => {
                self.render_link(traversal, link)?;
            }

            InlineMacro::Autolink(autolink) => {
                self.render_autolink(autolink)?;
            }

            InlineMacro::CrossReference(xref) => {
                self.render_cross_reference(traversal, xref)?;
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

    fn render_cross_reference(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        xref: &CrossReference<'_>,
    ) -> Result<(), Error> {
        if !xref.text.is_empty() {
            return self.visit_inline_nodes(traversal, &xref.text);
        }

        // Clone the handles so the borrowed reference text and the resolution
        // guard both outlive the `&mut self` render calls.
        let references = Rc::clone(&self.processor.references);
        let guard = self.processor.xref_guard.clone();
        let target = xref.target;
        // Manpages print reference text without a link. Asciidoctor uses a
        // matching local label even when the syntax selects another document.
        let mut display_xref = xref.clone();
        display_xref.target_is_local = true;
        match resolve_xref(references.get(target), &display_xref, &guard) {
            // A reference to a level-1 section reads as that section's `.SH`
            // heading, which manpages upper-case. An explicit label reads as
            // written.
            XrefDisplay::Title(inlines, _scope) => {
                let text_case = if self.processor.top_level_section_ids.contains(target) {
                    TextCase::Uppercase
                } else {
                    TextCase::Preserve
                };
                self.with_text_case(text_case, |visitor| {
                    visitor.visit_inline_nodes(traversal, inlines)
                })
            }
            XrefDisplay::Label(inlines, _scope) => self.visit_inline_nodes(traversal, inlines),
            XrefDisplay::ShortCaption(prefix) => {
                let text = manify(&prefix, EscapeMode::Normalize);
                write!(self.writer_mut(), "{text}")?;
                Ok(())
            }
            XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                let prefix = manify(&prefix, EscapeMode::Normalize);
                let separator = manify(", “", EscapeMode::Normalize);
                write!(self.writer_mut(), "{prefix}{separator}")?;
                self.visit_inline_nodes(traversal, inlines)?;
                let closing_quote = manify("”", EscapeMode::Normalize);
                write!(self.writer_mut(), "{closing_quote}")?;
                Ok(())
            }
            XrefDisplay::Emphasized(prefix, inlines, _scope) => {
                if let Some(prefix) = prefix {
                    let prefix = manify(&prefix, EscapeMode::Normalize);
                    let separator = manify(", ", EscapeMode::Normalize);
                    write!(self.writer_mut(), "{prefix}{separator}")?;
                }
                write!(self.writer_mut(), "\\fI")?;
                self.visit_inline_nodes(traversal, inlines)?;
                write!(self.writer_mut(), "\\fP")?;
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
    fn render_ui_inline_macro(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        macro_node: &InlineMacro,
    ) -> Result<(), Error> {
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
                self.render_index_term(traversal, it)?;
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
