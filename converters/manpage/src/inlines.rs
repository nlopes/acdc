//! Inline node rendering for manpages.
//!
//! Handles bold, italic, monospace, links, and other inline formatting.

use std::{borrow::Cow, io::Write, rc::Rc};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::apply_replacements;
use acdc_converters_core::{
    TraversalContext, decode_numeric_char_refs,
    link::{link_fallback, mailto_fallback, mailto_target},
    substitutions::{Replacements, TextBoundaries, apply_passthrough_replacements_iter},
    xref::{XrefDisplay, resolve_local_reference},
};
use acdc_parser::{
    Substitution, SymbolKind,
    ast::{
        AttributeValueRef, AutolinkRef, ContentRef, CrossReferenceRef, IconRef, InlineKind,
        InlineRef, LinkRef, MailtoRef, RawRef, ReferenceRef, TokensRef, UrlRef,
    },
};

use crate::{
    Error, ManpageVisitor,
    escape::{
        EscapeMode, escape_rendered_roff_macro_argument, escape_roff_macro_argument, manify,
        uppercase_title,
    },
    manpage_visitor::TextCase,
};

pub(crate) fn replacements() -> Replacements<'static> {
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

fn font_affixes(font: &'static str, current: &mut &'static str) -> (&'static str, &'static str) {
    let previous = std::mem::replace(current, font);
    (font, previous)
}

fn role_affixes(
    role: Option<&str>,
    default: RoleDefault,
    font: &mut &'static str,
) -> (String, String) {
    let mut prefix = String::new();
    let mut closings = Vec::new();

    for role in role.into_iter().flat_map(str::split_whitespace) {
        let (opening, closing) = match role {
            "underline" | "subtitle" => font_affixes("\\fI", font),
            "line-through" => ("[deleted: ", "]"),
            "overline" => ("[overlined: ", "]"),
            "big" => ("\\s+1", "\\s-1"),
            "small" => ("\\s-1", "\\s+1"),
            "highlight" => font_affixes("\\fB", font),
            _ => continue,
        };
        prefix.push_str(opening);
        closings.push(closing);
    }

    if prefix.is_empty() && role.is_none() && matches!(default, RoleDefault::Highlight) {
        let (opening, closing) = font_affixes("\\fB", font);
        prefix.push_str(opening);
        closings.push(closing);
    }

    let suffix = closings.into_iter().rev().collect();
    (prefix, suffix)
}

fn role_from_tokens(roles: TokensRef<'_>) -> Option<String> {
    if roles.is_empty() {
        return None;
    }
    let mut role = String::new();
    for (index, token) in roles.iter().enumerate() {
        if index > 0 {
            role.push(' ');
        }
        role.push_str(token);
    }
    Some(acdc_parser::strip_quotes(&role).to_owned())
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

fn is_empty_formatting(node: InlineRef<'_>) -> bool {
    let content = if let InlineKind::Bold(node) = node.kind() {
        node.content()
    } else if let InlineKind::Italic(node) = node.kind() {
        node.content()
    } else if let InlineKind::Monospace(node) = node.kind() {
        node.content()
    } else if let InlineKind::Highlight(node) = node.kind() {
        node.content()
    } else if let InlineKind::Subscript(node) = node.kind() {
        node.content()
    } else if let InlineKind::Superscript(node) = node.kind() {
        node.content()
    } else {
        return false;
    };
    content.inlines().all(is_empty_formatting)
}

pub(crate) fn contains_link(node: InlineRef<'_>) -> bool {
    let content = match node.kind() {
        InlineKind::Link(_)
        | InlineKind::Url(_)
        | InlineKind::Mailto(_)
        | InlineKind::Autolink(_)
        | InlineKind::CrossReference(_) => return true,
        InlineKind::Image(image) => return image.attributes().contains("link"),
        InlineKind::Bold(node) => node.content(),
        InlineKind::Italic(node) => node.content(),
        InlineKind::Monospace(node) => node.content(),
        InlineKind::Highlight(node) => node.content(),
        InlineKind::Subscript(node) => node.content(),
        InlineKind::Superscript(node) => node.content(),
        InlineKind::CurvedQuotation(node) => node.content(),
        InlineKind::CurvedApostrophe(node) => node.content(),
        InlineKind::IndexTerm(term) if term.is_visible() => term.term(),
        InlineKind::Plain(_)
        | InlineKind::Raw(_)
        | InlineKind::Verbatim(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::Symbol(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Anchor(_)
        | InlineKind::Callout(_)
        | InlineKind::Footnote(_)
        | InlineKind::Icon(_)
        | InlineKind::Keyboard(_)
        | InlineKind::Button(_)
        | InlineKind::Menu(_)
        | InlineKind::Pass(_)
        | InlineKind::Stem(_)
        | InlineKind::IndexTerm(_)
        | _ => return false,
    };
    content.inlines().any(contains_link)
}

impl<'a, W: Write> ManpageVisitor<'a, '_, W> {
    fn flush_link_label(&mut self, finish: bool) -> Result<(), Error> {
        if let Some(label) = self.link_label.as_mut() {
            let text = String::from_utf8_lossy(&label.content);
            let body = text.trim_start_matches(|character: char| character.is_ascii_whitespace());
            let leading = &text[..text.len() - body.len()];
            let trimmed = body.trim_end_matches(|character: char| character.is_ascii_whitespace());
            // A child command must never become part of its parent's quoted argument.
            if !trimmed.is_empty() || (finish && !label.split) {
                // Boundary spaces separate split commands, outside their visible labels.
                let leading = if label.split { leading } else { "" };
                let trailing = if finish { "" } else { &body[trimmed.len()..] };
                let font = self.current_font;
                // A split label must carry its active span into the link macro.
                let prefix = if font == "\\fR" { "" } else { font };
                let text = escape_rendered_roff_macro_argument(trimmed);
                let trailing = escape_rendered_roff_macro_argument(trailing);
                // Continue after the command; only authored spaces separate inline nodes.
                // The portable link macro also changes fonts, so restore our scope.
                writeln!(
                    self.writer,
                    "{leading}\\c\n.{} \"{}\" \"{prefix}{text}\" \"{font}{trailing}\\c\"",
                    label.command, label.target
                )?;
            } else if label.split && !finish {
                write!(self.writer, "{text}")?;
            }
            label.content.clear();
            label.split = true;
        }
        Ok(())
    }

    pub(crate) fn with_link_label(
        &mut self,
        target: String,
        mailto: bool,
        render: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.flush_link_label(false)?;
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
        let result = render(self).and_then(|()| self.flush_link_label(true));
        self.link_label = previous;
        self.text_boundaries = boundaries;
        self.in_inline_span = in_span;
        self.strip_next_leading_space = false;
        result
    }

    pub(crate) fn write_link_command(
        &mut self,
        command: &str,
        target: &str,
        label: &str,
    ) -> Result<(), Error> {
        self.flush_link_label(false)?;
        let font = self.current_font;
        let prefix = if font == "\\fR" { "" } else { font };
        writeln!(
            self.writer,
            "\\c\n.{command} \"{target}\" \"{prefix}{label}\" \"{font}\\c\""
        )?;
        self.strip_next_leading_space = false;
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
        let previous = self.current_font;
        let mut font = previous;
        let (prefix, suffix) = role_affixes(role, default, &mut font);
        let result = self.render_affixed(&prefix, &suffix, split, |visitor| {
            visitor.current_font = font;
            content(visitor)
        });
        self.current_font = previous;
        result
    }

    pub(crate) fn render_font(
        &mut self,
        font: &'static str,
        split: bool,
        content: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        // Nested changes overwrite roff's single previous-font slot. Restore
        // the enclosing font explicitly; mandoc cannot use groff's saved registers.
        let previous = self.current_font;
        let result = self.render_affixed(font, previous, split, |visitor| {
            visitor.current_font = font;
            content(visitor)
        });
        self.current_font = previous;
        result
    }

    pub(crate) fn render_affixed(
        &mut self,
        prefix: &str,
        suffix: &str,
        split: bool,
        content: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        // Formatting around several links belongs outside their command arguments.
        if split {
            self.flush_link_label(false)?;
            write!(self.writer, "{prefix}")?;
        } else {
            write!(self.writer_mut(), "{prefix}")?;
        }
        content(self)?;
        if split {
            self.flush_link_label(false)?;
            write!(self.writer, "{suffix}")?;
        } else {
            write!(self.writer_mut(), "{suffix}")?;
        }
        Ok(())
    }

    fn render_formatted_inlines(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        text: ContentRef<'_>,
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Error> {
        let split = self.link_label.is_some() && text.inlines().any(contains_link);
        self.render_affixed(prefix, suffix, split, |visitor| {
            visitor.write_content(traversal, text)
        })
    }

    fn render_font_inlines(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        text: ContentRef<'_>,
        font: &'static str,
    ) -> Result<(), Error> {
        let split = self.link_label.is_some() && text.inlines().any(contains_link);
        self.render_font(font, split, |visitor| {
            visitor.write_content(traversal, text)
        })
    }

    fn render_link_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        text: ContentRef<'_>,
        fallback: &str,
        role: Option<&str>,
    ) -> Result<(), Error> {
        let split = text.inlines().any(contains_link);
        self.render_with_role(role, RoleDefault::Plain, split, |visitor| {
            if text.is_empty() {
                write!(
                    visitor.writer_mut(),
                    "{}",
                    manify(fallback, EscapeMode::Normalize)
                )?;
            } else {
                visitor.write_content(traversal, text)?;
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
        let escaped = manify(&content, self.text_escape_mode);
        let w = self.writer_mut();
        if let Some(restored) = restore_em_dash_line_prefixes(&content, &escaped) {
            write!(w, "{restored}")?;
        } else {
            write!(w, "{escaped}")?;
        }
        Ok(())
    }

    fn render_raw_inline(&mut self, text: RawRef<'_>) -> Result<(), Error> {
        let mut replaced = apply_passthrough_replacements_iter(
            text.text(),
            text.substitutions(),
            &replacements(),
            self.text_boundaries,
        );
        if self.text_boundaries.at_paragraph_end()
            && text.text().ends_with("--")
            && replaced.ends_with(' ')
        {
            replaced.to_mut().pop();
        }
        let decoded = if text.substitutions().last() == Some(Substitution::SpecialChars) {
            Cow::Borrowed(replaced.as_ref())
        } else {
            decode_numeric_char_refs(&replaced)
        };
        let content = if self.strip_next_leading_space {
            self.strip_next_leading_space = false;
            decoded.trim_start_matches(|character: char| character.is_ascii_whitespace())
        } else {
            &decoded
        };
        let content = apply_text_case(Cow::Borrowed(content), self.text_case);
        let escaped = manify(&content, self.text_escape_mode);
        let w = self.writer_mut();
        write!(w, "{escaped}")?;
        Ok(())
    }

    fn render_formatting_span(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        if let InlineKind::Bold(bold) = node.kind() {
            self.render_font_inlines(traversal, bold.content(), "\\fB")?;
        } else if let InlineKind::Italic(italic) = node.kind() {
            self.render_font_inlines(traversal, italic.content(), "\\fI")?;
        } else if let InlineKind::Monospace(mono) = node.kind() {
            self.render_font_inlines(traversal, mono.content(), "\\f(CR")?;
        } else if let InlineKind::Highlight(highlight) = node.kind() {
            let default = if highlight.id().is_some() {
                RoleDefault::Plain
            } else {
                RoleDefault::Highlight
            };
            let split =
                self.link_label.is_some() && highlight.content().inlines().any(contains_link);
            self.render_with_role(highlight.role(), default, split, |visitor| {
                visitor.write_content(traversal, highlight.content())
            })?;
        } else if let InlineKind::Subscript(sub) = node.kind() {
            if !sub.content().inlines().all(is_empty_formatting) {
                self.render_formatted_inlines(traversal, sub.content(), "_(", ")")?;
            }
        } else if let InlineKind::Superscript(sup) = node.kind() {
            if !sup.content().inlines().all(is_empty_formatting) {
                self.render_formatted_inlines(traversal, sup.content(), "^(", ")")?;
            }
        } else if let InlineKind::CurvedQuotation(quoted) = node.kind() {
            self.render_formatted_inlines(traversal, quoted.content(), "\\(lq", "\\(rq")?;
        } else if let InlineKind::CurvedApostrophe(quoted) = node.kind() {
            self.render_formatted_inlines(traversal, quoted.content(), "\\(oq", "\\(cq")?;
        } else {
            self.warn_unsupported_parser_variant("formatting span");
        }
        Ok(())
    }

    pub(crate) fn render_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        match node.kind() {
            InlineKind::Plain(text) => self.render_plain_text(text.text())?,
            InlineKind::Symbol(symbol) => {
                self.render_plain_text(replacements().symbol(symbol.kind()))?;
                if symbol.kind() == SymbolKind::SpacedEmDash
                    && self.text_boundaries.at_paragraph_end()
                {
                    write!(self.writer_mut(), "\\&")?;
                }
            }

            InlineKind::Raw(text) => self.render_raw_inline(text)?,

            InlineKind::Verbatim(text) => {
                let content = apply_text_case(Cow::Borrowed(text.text()), self.text_case);
                let escaped = manify(&content, EscapeMode::Preserve);
                let w = self.writer_mut();
                write!(w, "{escaped}")?;
            }

            InlineKind::StandaloneCurvedApostrophe(_) => {
                let w = self.writer_mut();
                write!(w, "\\(cq")?;
            }

            InlineKind::LineBreak(_) => {
                let w = self.writer_mut();
                writeln!(w)?;
                writeln!(w, ".br")?;
            }

            // A roff comment here can start mid-line and print its leading dot.
            // References use the parser's catalog; the target needs no output.
            InlineKind::Anchor(_) => {}

            InlineKind::Callout(callout) => {
                let w = self.writer_mut();
                write!(w, "\\fB({})\\fP", callout.number())?;
            }

            InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_) => self.render_formatting_span(traversal, node)?,
            InlineKind::Url(url) => self.render_url(traversal, url)?,
            InlineKind::Link(link) => self.render_link(traversal, link)?,
            InlineKind::Mailto(mailto) => self.render_mailto(traversal, mailto)?,
            InlineKind::Autolink(autolink) => self.render_autolink(autolink)?,
            InlineKind::CrossReference(xref) => self.render_cross_reference(traversal, xref)?,
            InlineKind::Footnote(footnote) => write!(self.writer_mut(), "[{}]", footnote.number())?,
            InlineKind::Icon(icon) => self.render_icon(traversal, icon)?,
            InlineKind::Image(img) => {
                self.render_inline_image(traversal, img)?;
            }

            InlineKind::Keyboard(kbd) => {
                for (index, key) in kbd.keys().enumerate() {
                    if index > 0 {
                        write!(self.writer_mut(), "+")?;
                    }
                    self.render_font_inlines(traversal, key, "\\fB")?;
                }
            }
            InlineKind::Button(btn) => {
                write!(self.writer_mut(), "[")?;
                self.render_font_inlines(traversal, btn.label(), "\\fB")?;
                write!(self.writer_mut(), "]")?;
            }
            InlineKind::Menu(menu) => {
                self.render_font_inlines(traversal, menu.target(), "\\fB")?;
                for item in menu.items() {
                    write!(self.writer_mut(), " \\(ra ")?;
                    self.render_font_inlines(traversal, item, "\\fB")?;
                }
            }

            InlineKind::Pass(pass) => {
                // Passthrough content is backend-native and intentionally bypasses escaping.
                if let Some(text) = pass.text() {
                    let w = self.writer_mut();
                    write!(w, "{text}")?;
                }
            }

            InlineKind::Stem(stem) => {
                let content = manify(stem.text(), EscapeMode::Collapse);
                let w = self.writer_mut();
                write!(w, "{content}")?;
            }

            InlineKind::IndexTerm(it) => {
                self.render_index_term(traversal, it)?;
            }

            _ => self.warn_unsupported_parser_variant("inline node"),
        }
        Ok(())
    }

    fn render_url(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        url: UrlRef<'_>,
    ) -> Result<(), Error> {
        let target = url.target().to_string();
        let role = role_from_tokens(url.roles());
        self.with_link_label(escape_roff_macro_argument(&target), false, |visitor| {
            let styled = !role_affixes(role.as_deref(), RoleDefault::Plain, &mut "\\fR")
                .0
                .is_empty();
            if !url.text().is_empty() || url.hides_uri_scheme() || styled {
                visitor.render_link_content(
                    traversal,
                    url.text(),
                    link_fallback(&target, url.hides_uri_scheme()),
                    role.as_deref(),
                )?;
            }
            Ok(())
        })?;
        Ok(())
    }

    fn render_icon(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        icon: IconRef<'_>,
    ) -> Result<(), Error> {
        let fallback = acdc_converters_core::icon::alt_from_content(icon.target(), None);
        self.render_media_label(
            traversal,
            icon.alt(),
            &fallback,
            match icon.attributes().get("link") {
                Some(AttributeValueRef::Text(link)) => Some(acdc_parser::strip_quotes(link)),
                _ => None,
            },
            true,
        )?;
        Ok(())
    }

    fn render_link(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        link: LinkRef<'_>,
    ) -> Result<(), Error> {
        let target = link.target().to_string();
        let role = role_from_tokens(link.roles());
        self.with_link_label(escape_roff_macro_argument(&target), false, |visitor| {
            let styled = !role_affixes(role.as_deref(), RoleDefault::Plain, &mut "\\fR")
                .0
                .is_empty();
            if !link.text().is_empty() || link.hides_uri_scheme() || styled {
                visitor.render_link_content(
                    traversal,
                    link.text(),
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
        mailto: MailtoRef<'_>,
    ) -> Result<(), Error> {
        let target = mailto.target().to_string();
        let destination = mailto_target(mailto);
        let email = escape_roff_macro_argument(mailto_fallback(&destination)).replace('@', "\\(at");
        let role = role_from_tokens(mailto.roles());
        self.with_link_label(email, true, |visitor| {
            let styled = !role_affixes(role.as_deref(), RoleDefault::Plain, &mut "\\fR")
                .0
                .is_empty();
            if !mailto.text().is_empty() || styled || destination != target {
                visitor.render_link_content(
                    traversal,
                    mailto.text(),
                    mailto_fallback(&target),
                    role.as_deref(),
                )?;
            }
            Ok(())
        })
    }

    fn render_autolink(&mut self, autolink: AutolinkRef<'_>) -> Result<(), Error> {
        let target = autolink.target().to_string();
        if let Some(email) = target.strip_prefix("mailto:") {
            let email = escape_roff_macro_argument(email).replace('@', "\\(at");
            self.write_link_command("MTO", &email, "")
        } else {
            let label = if autolink.hides_uri_scheme() {
                escape_roff_macro_argument(link_fallback(&target, true))
            } else {
                String::new()
            };
            let target = escape_roff_macro_argument(&target);
            self.write_link_command("URL", &target, &label)
        }
    }

    fn render_cross_reference(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        xref: CrossReferenceRef<'_>,
    ) -> Result<(), Error> {
        if !xref.text().is_empty() {
            return self.write_content(traversal, xref.text());
        }

        // Clone the handles so the borrowed reference text and the resolution
        // guard both outlive the `&mut self` render calls.
        let references = Rc::clone(&self.processor.references);
        let guard = self.processor.xref_guard.clone();
        let target = xref.target();
        // Manpages print reference text without a link. Asciidoctor uses a
        // matching local label even when the syntax selects another document.
        match resolve_local_reference(references.get(target).map(ReferenceRef::from), xref, &guard)
        {
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
                    visitor.write_content(traversal, inlines)
                })
            }
            XrefDisplay::Label(inlines, _scope) => self.write_content(traversal, inlines),
            XrefDisplay::ShortCaption(prefix) => {
                let text = manify(&prefix, EscapeMode::Normalize);
                write!(self.writer_mut(), "{text}")?;
                Ok(())
            }
            XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                let prefix = manify(&prefix, EscapeMode::Normalize);
                let separator = manify(", “", EscapeMode::Normalize);
                write!(self.writer_mut(), "{prefix}{separator}")?;
                self.write_content(traversal, inlines)?;
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
                self.render_font_inlines(traversal, inlines, "\\fI")
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
}
