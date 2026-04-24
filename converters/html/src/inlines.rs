//! Inline content rendering for HTML conversion.
//!
//! # Security Model and HTML Escaping
//!
//! `AsciiDoc` follows a **trusted document author** model, similar to Markdown. The document
//! author is trusted to create safe content, and certain features intentionally allow raw HTML
//! output for flexibility.
//!
//! ## Escaping Behavior by Content Type
//!
//! ### `PlainText`
//! - **Always escaped**: HTML special characters (`<`, `>`, `&`, `"`, `'`) are converted to entities
//! - **Typography applied**: Em-dashes, ellipses, and smart quotes (unless in basic/verbatim mode)
//! - Example: `<script>` → `&lt;script&gt;`
//!
//! ### `RawText` (Passthrough Content)
//! - **Never escaped by default**: Intentionally outputs raw HTML
//! - Used for passthrough blocks (`++++...++++`) and inline passthroughs (`pass:[]`, `+++...+++`)
//! - **Escaping when verbatim**: Only escaped when in verbatim context (listing/literal blocks)
//! - Example: `pass:[<strong>test</strong>]` → `<strong>test</strong>` (raw HTML)
//!
//! ### `VerbatimText` (Code Blocks)
//! - **Always escaped**: Used in listing and literal blocks to display code
//! - **Callout processing**: Handles `<1>`, `<2>` markers specially
//! - Example in listing block: `<h1>` → `&lt;h1&gt;`
//!
//! ### Passthrough with `SpecialChars` Substitution
//! - `pass:c[...]` or `pass:specialchars[...]` → HTML is escaped
//! - Example: `pass:c[<strong>test</strong>]` → `&lt;strong&gt;test&lt;/strong&gt;`
//!
//! ## Why This Design?
//!
//! This matches asciidoctor's security model where:
//! 1. Document authors are trusted (like Markdown, not like user-generated HTML)
//! 2. Passthrough is a feature for intentionally embedding raw HTML
//! 3. Regular content is always escaped for safety
//! 4. Code blocks escape HTML to display it correctly
//!
//! ## ID Attributes
//!
//! Currently, acdc auto-generates section IDs from titles rather than accepting custom IDs.
//! This is inherently safe. When custom ID support is added, asciidoctor does NOT escape
//! IDs, trusting the document author to provide safe values.

use std::io::{self, Write};

use acdc_converters_core::{
    substitutions::{restore_escaped_patterns, strip_backslash_escapes},
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{
    Form, IndexTermKind, InlineMacro, InlineNode, StemNotation, Substitution, inlines_to_string,
    parse_text_for_quotes, strip_quotes, substitute,
};

/// Leak a `&str` into a `&'static str` so index term kinds can be cached
/// beyond the parser arena's lifetime.
///
/// Leaks are bounded by the number of index entries encountered during a
/// single conversion run — acceptable for short-lived converter processes.
fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// Convert a borrowed `IndexTermKind` to one with `'static` lifetime.
fn index_term_kind_to_static(kind: &IndexTermKind<'_>) -> IndexTermKind<'static> {
    match kind {
        IndexTermKind::Flow(t) => IndexTermKind::Flow(leak_str(t)),
        IndexTermKind::Concealed {
            term,
            secondary,
            tertiary,
        } => IndexTermKind::Concealed {
            term: leak_str(term),
            secondary: secondary.map(leak_str),
            tertiary: tertiary.map(leak_str),
        },
        // IndexTermKind is non_exhaustive
        _ => IndexTermKind::Flow(""),
    }
}

use crate::{
    Error, HtmlVisitor, RenderOptions,
    constants::{encode_html_entities, escape_ampersands},
    icon::write_icon,
    image_helpers::{alt_text_from_filename, write_dimension_attributes},
};

/// Escape `&` to `&amp;` in URL strings for use in `href` attributes.
pub(crate) fn escape_href(url: &str) -> String {
    url.replace('&', "&amp;")
}

/// Strip the URI scheme (e.g., `https://`, `http://`, `ftp://`) from a URL string.
///
/// Used when the `hide-uri-scheme` document attribute is set to display cleaner link text
/// while preserving the full URL in the `href` attribute.
fn strip_uri_scheme(url: &str) -> &str {
    url.find("://")
        .map_or(url, |pos| url.get(pos + 3..).unwrap_or(url))
}

/// Extract the `role` attribute as a non-empty, unquoted string.
///
/// Returns `None` when the attribute is absent, non-string, or empty after stripping quotes.
fn role_from_attrs(attributes: &acdc_parser::ElementAttributes) -> Option<String> {
    attributes.get("role").and_then(|v| match v {
        acdc_parser::AttributeValue::String(s) => {
            let role = strip_quotes(s);
            if role.is_empty() {
                None
            } else {
                Some(role.to_string())
            }
        }
        acdc_parser::AttributeValue::Bool(_) | acdc_parser::AttributeValue::None | _ => None,
    })
}

/// Build the ` class="..."` attribute (with leading space) for link-like anchors.
///
/// - `bare` adds the `bare` class when `true` (asciidoctor's convention for empty link text
///   on URL/link macros; `Mailto` passes `false` because it does not use `bare`).
/// - `role` contributes additional class tokens.
/// - Returns an empty string when neither applies.
fn link_class_attr(role: Option<String>, bare: bool) -> String {
    match (bare, role) {
        (true, Some(role)) => format!(" class=\"bare {role}\""),
        (true, None) => " class=\"bare\"".to_string(),
        (false, Some(role)) => format!(" class=\"{role}\""),
        (false, None) => String::new(),
    }
}

/// Compute the visible fallback text for a link target when no display text was given.
///
/// Strips the `mailto:` prefix, or — when `hide_uri_scheme` is set — strips schemes like
/// `https://`, `http://`, `ftp://`. Otherwise returns the target as-is.
fn link_display_fallback(target: &str, hide_uri_scheme: bool) -> &str {
    if let Some(email) = target.strip_prefix("mailto:") {
        email
    } else if hide_uri_scheme {
        strip_uri_scheme(target)
    } else {
        target
    }
}

/// Extract `target` and `rel` attributes from `window` or `target` attribute values.
///
/// Maps the `AsciiDoc` `window` (preferred) or `target` attribute to HTML:
/// - `window=_blank` / `target=_blank` → `target="_blank" rel="noopener"`
/// - `window=<other>` / `target=<other>` → `target="<other>"`
fn window_attrs(attributes: &acdc_parser::ElementAttributes) -> String {
    let get_str = |key: &str| {
        attributes.get(key).and_then(|v| match v {
            acdc_parser::AttributeValue::String(s) => {
                let s = strip_quotes(s);
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            }
            acdc_parser::AttributeValue::Bool(_) | acdc_parser::AttributeValue::None | _ => None,
        })
    };
    let window = get_str("window").or_else(|| get_str("target"));
    match window {
        Some(w) if w == "_blank" => " target=\"_blank\" rel=\"noopener\"".to_string(),
        Some(w) => format!(" target=\"{w}\""),
        None => String::new(),
    }
}

/// Helper to write an HTML opening tag with optional id and role attributes.
///
/// Handles the common pattern of:
/// - Both id and role: `<tag id="..." class="...">`
/// - Only id: `<tag id="...">`
/// - Only role: `<tag class="...">`
/// - Neither: `<tag>`
fn write_tag_with_attrs<W: Write + ?Sized>(
    writer: &mut W,
    tag: &str,
    id: Option<&str>,
    role: Option<&str>,
) -> io::Result<()> {
    match (id, role) {
        (Some(id), Some(role)) => write!(writer, "<{tag} id=\"{id}\" class=\"{role}\">"),
        (Some(id), None) => write!(writer, "<{tag} id=\"{id}\">"),
        (None, Some(role)) => write!(writer, "<{tag} class=\"{role}\">"),
        (None, None) => write!(writer, "<{tag}>"),
    }
}

/// Tracks what was written by `write_quote_open` so `write_quote_close` can match.
#[derive(Clone, Copy)]
enum QuoteRenderState {
    /// Wrote HTML tag - close with `</tag>`
    Html,
    /// Basic mode - no tags written, no closing needed
    Basic,
    /// Quotes disabled - wrote literal delimiter, close with same
    Literal,
}

/// Write opening markup for inline formatting (bold, italic, etc.).
///
/// Returns state indicating what was written, for use with `write_quote_close`.
fn write_quote_open<W: Write + ?Sized>(
    w: &mut W,
    tag: &str,
    delim: &str,
    id: Option<&str>,
    role: Option<&str>,
    subs: &[Substitution],
    basic: bool,
) -> io::Result<QuoteRenderState> {
    if subs.contains(&Substitution::Quotes) {
        if basic {
            Ok(QuoteRenderState::Basic)
        } else {
            write_tag_with_attrs(w, tag, id, role)?;
            Ok(QuoteRenderState::Html)
        }
    } else {
        write!(w, "{delim}")?;
        Ok(QuoteRenderState::Literal)
    }
}

/// Write closing markup for inline formatting.
fn write_quote_close<W: Write + ?Sized>(
    w: &mut W,
    tag: &str,
    delim: &str,
    state: QuoteRenderState,
) -> io::Result<()> {
    match state {
        QuoteRenderState::Html => write!(w, "</{tag}>"),
        QuoteRenderState::Basic => Ok(()),
        QuoteRenderState::Literal => write!(w, "{delim}"),
    }
}

impl<W: Write> HtmlVisitor<'_, W> {
    /// Internal implementation for visiting inline nodes
    #[allow(clippy::too_many_lines)]
    pub(crate) fn render_inline_node(
        &mut self,
        node: &InlineNode,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let w = self.writer_mut();
        match node {
            InlineNode::PlainText(p) => {
                // Attribute substitution already applied by inline preprocessor during parsing
                let content = &p.content;

                // If escaped (from `\^2^` etc.), skip quote re-parsing; otherwise use block subs.
                let effective_subs: &[Substitution] = if p.escaped { &[] } else { subs };

                if effective_subs.contains(&Substitution::Quotes) {
                    // If quotes substitution is enabled, parse for inline formatting
                    let parsed = parse_text_for_quotes(content);
                    // Render parsed nodes without quotes to avoid infinite recursion
                    let no_quotes_subs: Vec<_> = effective_subs
                        .iter()
                        .filter(|s| **s != Substitution::Quotes)
                        .cloned()
                        .collect();
                    for node in parsed.inlines() {
                        self.render_inline_node(node, options, &no_quotes_subs)?;
                    }
                } else {
                    // No quotes substitution - output with escaping and typography only
                    let text = substitution_text(content, effective_subs, options);
                    if options.hardbreaks {
                        write!(w, "{}", text.replace('\n', "<br>"))?;
                    } else {
                        write!(w, "{text}")?;
                    }
                }
            }
            InlineNode::RawText(r) => {
                // RawText comes from passthroughs - attribute expansion was already
                // handled (or explicitly skipped) by the preprocessor based on the
                // passthrough's own substitution settings. Do NOT apply block subs.
                let content = &r.content;
                let text = if options.inlines_verbatim {
                    substitution_text(content, subs, options)
                } else if r.subs.is_empty() {
                    content.to_string()
                } else {
                    substitution_text(content, &r.subs, options)
                };
                write!(w, "{text}")?;
            }
            InlineNode::VerbatimText(v) => {
                // VerbatimText is now just text (callouts are separate CalloutRef nodes)
                // Apply attribute substitution first, then escaping
                let content = if subs.contains(&Substitution::Attributes) {
                    substitute(
                        v.content,
                        &[Substitution::Attributes],
                        processor.document_attributes(),
                    )
                } else {
                    std::borrow::Cow::Borrowed(v.content)
                };
                let verbatim_options = RenderOptions {
                    inlines_verbatim: true,
                    ..options.clone()
                };

                // If quotes substitution is enabled, parse for inline formatting
                if subs.contains(&Substitution::Quotes) {
                    let parsed = parse_text_for_quotes(&content);
                    // Render parsed nodes with verbatim settings
                    // Keep Quotes in subs so BoldText/ItalicText render as HTML
                    for node in parsed.inlines() {
                        self.render_inline_node(node, &verbatim_options, subs)?;
                    }
                } else {
                    let text = substitution_text(&content, subs, &verbatim_options);
                    write!(w, "{text}")?;
                }
            }
            InlineNode::CalloutRef(callout) => {
                if processor.is_font_icons_mode() {
                    write!(
                        w,
                        "<i class=\"conum\" data-value=\"{0}\"></i><b>({0})</b>",
                        callout.number
                    )?;
                } else {
                    write!(w, "<b class=\"conum\">({})</b>", callout.number)?;
                }
            }
            InlineNode::BoldText(b) => {
                let delim = match b.form {
                    Form::Constrained => "*",
                    Form::Unconstrained => "**",
                };
                let state = write_quote_open(
                    w,
                    "strong",
                    delim,
                    b.id,
                    b.role,
                    subs,
                    options.inlines_basic,
                )?;
                self.visit_inline_nodes(&b.content)?;
                write_quote_close(self.writer_mut(), "strong", delim, state)?;
            }
            InlineNode::ItalicText(i) => {
                let delim = match i.form {
                    Form::Constrained => "_",
                    Form::Unconstrained => "__",
                };
                let state =
                    write_quote_open(w, "em", delim, i.id, i.role, subs, options.inlines_basic)?;
                self.visit_inline_nodes(&i.content)?;
                write_quote_close(self.writer_mut(), "em", delim, state)?;
            }
            InlineNode::HighlightText(h) => {
                // Warn about deprecated built-in roles
                if let Some(role) = h.role {
                    for r in role.split_whitespace() {
                        match r {
                            "big" => tracing::warn!(
                                role = %r,
                                "Role is deprecated. Consider using `+++<big>+++text+++</big>+++` or CSS font-size instead."
                            ),
                            "small" => tracing::warn!(
                                role = %r,
                                "Role is deprecated. Consider using `+++<small>+++text+++</small>+++` or CSS font-size instead."
                            ),
                            _ => {}
                        }
                    }
                }
                // Check if quotes substitution is enabled
                if subs.contains(&Substitution::Quotes) {
                    if !options.inlines_basic {
                        if processor.variant() == crate::HtmlVariant::Semantic
                            && h.role == Some("line-through")
                        {
                            write_tag_with_attrs(w, "s", h.id, None)?;
                        } else {
                            // asciidoctor behavior: use <span> when role is present, <mark> otherwise
                            let tag = if h.role.is_some() { "span" } else { "mark" };
                            write_tag_with_attrs(w, tag, h.id, h.role)?;
                        }
                    }
                    self.visit_inline_nodes(&h.content)?;
                    if !options.inlines_basic {
                        let w = self.writer_mut();
                        if processor.variant() == crate::HtmlVariant::Semantic
                            && h.role == Some("line-through")
                        {
                            write!(w, "</s>")?;
                        } else {
                            let tag = if h.role.is_some() { "span" } else { "mark" };
                            write!(w, "</{tag}>")?;
                        }
                    }
                } else {
                    // No quotes substitution - output raw markup
                    let delim = match h.form {
                        Form::Constrained => "#",
                        Form::Unconstrained => "##",
                    };
                    write!(w, "{delim}")?;
                    self.visit_inline_nodes(&h.content)?;
                    let w = self.writer_mut();
                    write!(w, "{delim}")?;
                }
            }
            InlineNode::MonospaceText(m) => {
                let delim = match m.form {
                    Form::Constrained => "`",
                    Form::Unconstrained => "``",
                };
                let state =
                    write_quote_open(w, "code", delim, m.id, m.role, subs, options.inlines_basic)?;
                self.visit_inline_nodes(&m.content)?;
                write_quote_close(self.writer_mut(), "code", delim, state)?;
            }
            InlineNode::CurvedQuotationText(c) => {
                // Check if quotes substitution is enabled
                if subs.contains(&Substitution::Quotes) {
                    if c.id.is_some() || c.role.is_some() {
                        write_tag_with_attrs(w, "span", c.id, c.role)?;
                        write!(w, "&ldquo;")?;
                    } else {
                        write!(w, "&ldquo;")?;
                    }
                    self.visit_inline_nodes(&c.content)?;
                    let w = self.writer_mut();
                    if c.id.is_some() || c.role.is_some() {
                        write!(w, "&rdquo;</span>")?;
                    } else {
                        write!(w, "&rdquo;")?;
                    }
                } else {
                    // No quotes substitution - output literal quotes
                    write!(w, "\"")?;
                    self.visit_inline_nodes(&c.content)?;
                    let w = self.writer_mut();
                    write!(w, "\"")?;
                }
            }
            InlineNode::CurvedApostropheText(c) => {
                // Check if quotes substitution is enabled
                if subs.contains(&Substitution::Quotes) {
                    if c.id.is_some() || c.role.is_some() {
                        write_tag_with_attrs(w, "span", c.id, c.role)?;
                        write!(w, "&lsquo;")?;
                    } else {
                        write!(w, "&lsquo;")?;
                    }
                    self.visit_inline_nodes(&c.content)?;
                    let w = self.writer_mut();
                    if c.id.is_some() || c.role.is_some() {
                        write!(w, "&rsquo;</span>")?;
                    } else {
                        write!(w, "&rsquo;")?;
                    }
                } else {
                    // No quotes substitution - output literal apostrophes
                    write!(w, "'")?;
                    self.visit_inline_nodes(&c.content)?;
                    let w = self.writer_mut();
                    write!(w, "'")?;
                }
            }
            InlineNode::StandaloneCurvedApostrophe(_) => {
                // Check if quotes substitution is enabled
                if subs.contains(&Substitution::Quotes) {
                    write!(w, "&rsquo;")?;
                } else {
                    write!(w, "'")?;
                }
            }
            InlineNode::SuperscriptText(s) => {
                // Note: superscript doesn't check inlines_basic (pass false to preserve behavior)
                let state = write_quote_open(w, "sup", "^", s.id, s.role, subs, false)?;
                self.visit_inline_nodes(&s.content)?;
                write_quote_close(self.writer_mut(), "sup", "^", state)?;
            }
            InlineNode::SubscriptText(s) => {
                // Note: subscript doesn't check inlines_basic (pass false to preserve behavior)
                let state = write_quote_open(w, "sub", "~", s.id, s.role, subs, false)?;
                self.visit_inline_nodes(&s.content)?;
                write_quote_close(self.writer_mut(), "sub", "~", state)?;
            }
            InlineNode::Macro(m) => self.render_inline_macro(m, options, subs)?,
            InlineNode::LineBreak(_) => {
                writeln!(w, "<br>")?;
            }
            InlineNode::InlineAnchor(anchor) if !options.toc_mode => {
                write!(w, "<a id=\"{}\"></a>", anchor.id)?;
            }
            // Explicit InlineAnchor arm for TOC mode (no nested anchors) plus
            // a catch-all for future `#[non_exhaustive]` variants — both
            // render nothing, but enumerating the anchor arm keeps
            // `wildcard_enum_match_arm` satisfied.
            InlineNode::InlineAnchor(_) | _ => {}
        }
        Ok(())
    }

    /// Render an inline macro by dispatching to the per-variant renderer.
    fn render_inline_macro(
        &mut self,
        m: &InlineMacro,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        match m {
            InlineMacro::Autolink(al) => self.render_autolink(al, options),
            InlineMacro::Link(l) => self.render_link(l, options, subs),
            InlineMacro::Image(i) => self.render_inline_image(i),
            InlineMacro::Pass(p) => self.render_pass(p, options, subs),
            InlineMacro::Url(u) => self.render_url(u, options, subs),
            InlineMacro::Mailto(m) => self.render_mailto(m, options, subs),
            InlineMacro::Footnote(f) => self.render_footnote(f, options),
            InlineMacro::Button(b) => self.render_button(b),
            InlineMacro::CrossReference(xref) => self.render_xref(xref, options, subs),
            InlineMacro::Stem(s) => self.render_stem(s),
            InlineMacro::Icon(i) => self.render_icon(i),
            InlineMacro::Keyboard(k) => self.render_keyboard(k),
            InlineMacro::Menu(menu) => self.render_menu(menu),
            InlineMacro::IndexTerm(it) => self.render_indexterm(it, options, subs),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("Unsupported inline macro: {m:?}"),
            )
            .into()),
        }
    }

    fn render_autolink(
        &mut self,
        al: &acdc_parser::Autolink<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let w = self.writer_mut();
        let hide_uri_scheme = processor
            .document_attributes()
            .get("hide-uri-scheme")
            .is_some();
        let href_str = al.url.to_string();
        let inner = if al.bracketed {
            href_str
                .strip_prefix('<')
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(&href_str)
        } else {
            &href_str
        };
        let display_text = link_display_fallback(inner, hide_uri_scheme).to_string();

        if options.inlines_basic || options.toc_mode {
            write!(w, "{display_text}")?;
        } else if al.bracketed {
            // Preserve angle brackets for bracketed autolinks (e.g., <user@example.com>)
            write!(
                w,
                "&lt;<a href=\"{}\">{display_text}</a>&gt;",
                escape_href(&href_str)
            )?;
        } else {
            write!(
                w,
                "<a href=\"{}\" class=\"bare\">{display_text}</a>",
                escape_href(&href_str)
            )?;
        }
        Ok(())
    }

    fn render_link(
        &mut self,
        l: &acdc_parser::Link<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let hide_uri_scheme = processor
            .document_attributes()
            .get("hide-uri-scheme")
            .is_some();
        let target_str = l.target.to_string();
        let fallback = link_display_fallback(&target_str, hide_uri_scheme);

        if options.inlines_basic || options.toc_mode {
            if l.text.is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in &l.text {
                    self.render_inline_node(inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let class_attr = link_class_attr(role_from_attrs(&l.attributes), l.text.is_empty());
        let target_attr = window_attrs(&l.attributes);
        write!(
            self.writer_mut(),
            "<a href=\"{}\"{class_attr}{target_attr}>",
            escape_href(&target_str)
        )?;
        if l.text.is_empty() {
            write!(self.writer_mut(), "{fallback}")?;
        } else {
            for inline in &l.text {
                self.render_inline_node(inline, options, subs)?;
            }
        }
        write!(self.writer_mut(), "</a>")?;
        Ok(())
    }

    fn render_inline_image(&mut self, i: &acdc_parser::Image<'_>) -> Result<(), Error> {
        let is_semantic = self.processor.variant() == crate::HtmlVariant::Semantic;
        let w = self.writer_mut();
        // Inline images use a span wrapper (not in semantic mode)
        if !is_semantic {
            write!(w, "<span class=\"image\">")?;
        }

        // alt text: title (first positional) > `alt` attr > filename
        let alt_text = if i.title.is_empty() {
            i.metadata.attributes.get("alt").map_or_else(
                || alt_text_from_filename(&i.source),
                std::string::ToString::to_string,
            )
        } else {
            inlines_to_string(&i.title)
        };

        let link = i.metadata.attributes.get("link");
        if let Some(link) = link {
            write!(
                w,
                "<a class=\"image\" href=\"{}\">",
                escape_href(&link.to_string())
            )?;
        }

        write!(w, "<img src=\"{}\" alt=\"{alt_text}\"", i.source)?;
        write_dimension_attributes(w, &i.metadata)?;
        if let Some(title) = i.metadata.attributes.get("title") {
            write!(w, " title=\"{title}\"")?;
        }
        write!(w, ">")?;

        if link.is_some() {
            write!(w, "</a>")?;
        }
        if !is_semantic {
            write!(w, "</span>")?;
        }
        Ok(())
    }

    fn render_pass(
        &mut self,
        p: &acdc_parser::Pass<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let Some(text) = p.text else { return Ok(()) };
        let w = self.writer_mut();
        // Only escape when SpecialChars substitution is enabled (pass:c[]).
        // Otherwise output raw HTML (pass:[], +++...+++).
        if p.substitutions.contains(&Substitution::SpecialChars) {
            let text = substitution_text(text, subs, options);
            write!(w, "{text}")?;
        } else {
            write!(w, "{text}")?;
        }
        Ok(())
    }

    fn render_url(
        &mut self,
        u: &acdc_parser::Url<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let hide_uri_scheme = processor
            .document_attributes()
            .get("hide-uri-scheme")
            .is_some();
        let target_str = u.target.to_string();
        let fallback = link_display_fallback(&target_str, hide_uri_scheme);

        if options.toc_mode {
            if u.text.is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in &u.text {
                    self.render_inline_node(inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let class_attr = link_class_attr(role_from_attrs(&u.attributes), u.text.is_empty());
        let target_attr = window_attrs(&u.attributes);
        write!(
            self.writer_mut(),
            "<a href=\"{}\"{class_attr}{target_attr}>",
            escape_href(&target_str)
        )?;
        if u.text.is_empty() {
            write!(self.writer_mut(), "{fallback}")?;
        } else {
            for inline in &u.text {
                self.render_inline_node(inline, options, subs)?;
            }
        }
        write!(self.writer_mut(), "</a>")?;
        Ok(())
    }

    fn render_mailto(
        &mut self,
        m: &acdc_parser::Mailto<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target_str = m.target.to_string();
        // `mailto:` never uses `hide-uri-scheme` (the prefix strip handles it),
        // and never emits `class="bare"` (asciidoctor's convention).
        let fallback = link_display_fallback(&target_str, false);

        if options.toc_mode {
            if m.text.is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in &m.text {
                    self.render_inline_node(inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let class_attr = link_class_attr(role_from_attrs(&m.attributes), false);
        let target_attr = window_attrs(&m.attributes);
        write!(
            self.writer_mut(),
            "<a href=\"{}\"{class_attr}{target_attr}>",
            escape_href(&target_str)
        )?;
        if m.text.is_empty() {
            write!(self.writer_mut(), "{fallback}")?;
        } else {
            for inline in &m.text {
                self.render_inline_node(inline, options, subs)?;
            }
        }
        write!(self.writer_mut(), "</a>")?;
        Ok(())
    }

    fn render_footnote(
        &mut self,
        f: &acdc_parser::Footnote<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        // A named footnote reference (footnote:name[] with empty content)
        // uses class="footnoteref" and no IDs, matching asciidoctor.
        let is_ref = f.id.is_some() && f.content.is_empty();
        let is_semantic = self.processor.variant() == crate::HtmlVariant::Semantic;
        let w = self.writer_mut();
        let number = f.number;

        if options.inlines_basic {
            write!(w, "[{number}]")?;
            return Ok(());
        }

        if is_semantic {
            if options.toc_mode {
                write!(w, "<sup class=\"footnote-ref\">[{number}]</sup>")?;
            } else if is_ref {
                write!(
                    w,
                    "<sup class=\"footnote-ref\">[<a href=\"#_footnote_{number}\" title=\"View footnote {number}\" role=\"doc-noteref\">{number}</a>]</sup>"
                )?;
            } else {
                write!(
                    w,
                    "<sup class=\"footnote-ref\" id=\"_footnoteref_{number}\">[<a href=\"#_footnote_{number}\" title=\"View footnote {number}\" role=\"doc-noteref\">{number}</a>]</sup>"
                )?;
            }
            return Ok(());
        }

        let sup_class = if is_ref { "footnoteref" } else { "footnote" };
        if options.toc_mode {
            // In TOC mode, render footnote without anchor link or id
            // (id stays on the heading's footnote to avoid duplicate IDs)
            write!(w, "<sup class=\"{sup_class}\">[{number}]</sup>")?;
            return Ok(());
        }

        write!(w, "<sup class=\"{sup_class}\"")?;
        if !is_ref && let Some(id) = &f.id {
            write!(w, " id=\"_footnote_{id}\"")?;
        }
        if is_ref {
            write!(
                w,
                ">[<a class=\"footnote\" href=\"#_footnotedef_{number}\" title=\"View footnote.\">{number}</a>]</sup>"
            )?;
        } else {
            write!(
                w,
                ">[<a id=\"_footnoteref_{number}\" class=\"footnote\" href=\"#_footnotedef_{number}\" title=\"View footnote.\">{number}</a>]</sup>"
            )?;
        }
        Ok(())
    }

    fn render_button(&mut self, b: &acdc_parser::Button<'_>) -> Result<(), Error> {
        let processor = self.processor.clone();
        let w = self.writer_mut();
        if processor.document_attributes.get("experimental").is_none() {
            write!(w, "btn:[{}]", b.label)?;
            return Ok(());
        }
        if processor.variant() == crate::HtmlVariant::Semantic {
            write!(w, "<kbd class=\"button\"><samp>{}</samp></kbd>", b.label)?;
        } else {
            write!(w, "<b class=\"button\">{}</b>", b.label)?;
        }
        Ok(())
    }

    fn render_xref(
        &mut self,
        xref: &acdc_parser::CrossReference<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let processor = self.processor.clone();

        if xref.text.is_empty() {
            // Priority: xreflabel (from [[id,Custom Text]]) > section title > fallback
            let display_text = processor
                .toc_entries()
                .iter()
                .find(|entry| entry.id == xref.target)
                .map_or_else(
                    || format!("[{}]", xref.target),
                    |entry| {
                        entry.xreflabel.as_ref().map_or_else(
                            || inlines_to_string(&entry.title),
                            std::string::ToString::to_string,
                        )
                    },
                );

            let w = self.writer_mut();
            if options.inlines_basic || options.toc_mode {
                write!(w, "{display_text}")?;
            } else {
                write!(w, "<a href=\"#{}\">{display_text}</a>", xref.target)?;
            }
            return Ok(());
        }

        if options.inlines_basic || options.toc_mode {
            for inline in &xref.text {
                self.render_inline_node(inline, options, subs)?;
            }
            return Ok(());
        }

        write!(self.writer_mut(), "<a href=\"#{}\">", xref.target)?;
        for inline in &xref.text {
            self.render_inline_node(inline, options, subs)?;
        }
        write!(self.writer_mut(), "</a>")?;
        Ok(())
    }

    fn render_stem(&mut self, s: &acdc_parser::Stem<'_>) -> Result<(), Error> {
        let forced = if self.processor.variant() == crate::HtmlVariant::Semantic {
            self.processor
                .document_attributes()
                .get("html5s-force-stem-type")
                .and_then(|v| v.to_string().parse::<StemNotation>().ok())
        } else {
            None
        };
        let notation = forced.as_ref().unwrap_or(&s.notation);
        let w = self.writer_mut();
        match notation {
            StemNotation::Latexmath => writeln!(w, "\\({}\\)", s.content)?,
            StemNotation::Asciimath => writeln!(w, "\\${}\\$", s.content)?,
        }
        Ok(())
    }

    fn render_icon(&mut self, i: &acdc_parser::Icon<'_>) -> Result<(), Error> {
        let processor = self.processor.clone();
        write_icon(self.writer_mut(), &processor, i)?;
        Ok(())
    }

    fn render_keyboard(&mut self, k: &acdc_parser::Keyboard<'_>) -> Result<(), Error> {
        let is_semantic = self.processor.variant() == crate::HtmlVariant::Semantic;
        let key_class = if is_semantic { " class=\"key\"" } else { "" };
        let w = self.writer_mut();

        if k.keys.len() == 1
            && let Some(key) = k.keys.first()
        {
            write!(w, "<kbd{key_class}>{key}</kbd>")?;
            return Ok(());
        }

        if is_semantic {
            write!(w, "<kbd class=\"keyseq\">")?;
        } else {
            write!(w, "<span class=\"keyseq\">")?;
        }
        for (i, key) in k.keys.iter().enumerate() {
            if i > 0 {
                write!(w, "+")?;
            }
            write!(w, "<kbd{key_class}>{key}</kbd>")?;
        }
        if is_semantic {
            write!(w, "</kbd>")?;
        } else {
            write!(w, "</span>")?;
        }
        Ok(())
    }

    fn render_menu(&mut self, menu: &acdc_parser::Menu<'_>) -> Result<(), Error> {
        let is_semantic = self.processor.variant() == crate::HtmlVariant::Semantic;
        let w = self.writer_mut();

        if menu.items.is_empty() {
            if is_semantic {
                write!(w, "<kbd class=\"menu\"><samp>{}</samp></kbd>", menu.target)?;
            } else {
                write!(w, "<b class=\"menuref\">{}</b>", menu.target)?;
            }
            return Ok(());
        }

        if is_semantic {
            write!(w, "<kbd class=\"menuseq\">")?;
            write!(w, "<kbd class=\"menu\"><samp>{}</samp></kbd>", menu.target)?;
            for item in &menu.items {
                write!(w, "&#160;<span class=\"caret\">&#8250;</span>&#32;")?;
                write!(w, "<kbd class=\"menu\"><samp>{item}</samp></kbd>")?;
            }
            write!(w, "</kbd>")?;
            return Ok(());
        }

        write!(w, "<span class=\"menuseq\">")?;
        write!(w, "<b class=\"menu\">{}</b>", menu.target)?;
        for (i, item) in menu.items.iter().enumerate() {
            write!(w, "&#160;<i class=\"fa fa-angle-right caret\"></i> ")?;
            if i == menu.items.len() - 1 {
                write!(w, "<b class=\"menuitem\">{item}</b>")?;
            } else {
                write!(w, "<b class=\"submenu\">{item}</b>")?;
            }
        }
        write!(w, "</span>")?;
        Ok(())
    }

    fn render_indexterm(
        &mut self,
        it: &acdc_parser::IndexTerm<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        if options.toc_mode {
            // In TOC mode, skip anchor but still output visible term text
            if it.is_visible() {
                let text = substitution_text(it.term(), subs, options);
                write!(self.writer_mut(), "{text}")?;
            }
            return Ok(());
        }

        let anchor_id = self
            .processor
            .clone()
            .add_index_entry(index_term_kind_to_static(&it.kind));
        write!(self.writer_mut(), "<a id=\"{anchor_id}\"></a>")?;

        // Flow terms (visible): also output the term text.
        // Concealed terms: anchor only, no visible text.
        if it.is_visible() {
            let text = substitution_text(it.term(), subs, options);
            write!(self.writer_mut(), "{text}")?;
        }
        Ok(())
    }
}

fn substitution_text(text: &str, subs: &[Substitution], options: &RenderOptions) -> String {
    debug_assert!(
        !text.is_empty(),
        "substitution_text called with empty text - caller should filter empty content"
    );
    if text.is_empty() {
        tracing::warn!(
            "substitution_text called with empty text - caller should filter empty content"
        );
        return String::new();
    }

    // When escape_html is false (subs=none), return text as-is
    if !subs.contains(&Substitution::SpecialChars) {
        return text.to_string();
    }

    // Determine if we should apply typography replacements
    // Based on substitutions list, skip for basic mode (passthrough)
    let should_apply_replacements =
        subs.contains(&Substitution::Replacements) && !options.inlines_basic;

    // Strip backslash escapes first (before any other processing)
    // Only needed when replacements are applied (escape sequences only matter for replacements)
    let text = if should_apply_replacements {
        strip_backslash_escapes(text)
    } else {
        text.to_string()
    };

    // Escape & first (before arrow replacements that produce & entities)
    let text = escape_ampersands(&text);

    // Apply all typography replacements (em-dashes, arrows, symbols, ellipsis, apostrophes)
    // This must happen after & escaping (replacements produce & entities) and before <> escaping
    let text = if should_apply_replacements {
        acdc_converters_core::substitutions::Replacements::html()
            .apply(&text, !options.in_inline_span)
    } else {
        text
    };

    // Restore escaped patterns (convert placeholders back to literal forms)
    // This must happen after typography substitutions but BEFORE escaping < and >
    // so that restored patterns like => and <- don't get their angle brackets escaped
    let text = if should_apply_replacements {
        restore_escaped_patterns(&text)
    } else {
        text
    };

    // Escape < and > after restore so that restored patterns (e.g., \=> → =>) keep literal chars
    let text = text.replace('>', "&gt;").replace('<', "&lt;");

    // Encode non-ASCII Unicode characters as HTML numeric entities
    // to match asciidoctor's output format
    encode_html_entities(&text)
}

#[cfg(test)]
mod tests {
    use acdc_converters_core::substitutions::replace_apostrophes;

    #[test]
    fn apostrophe_in_contraction() {
        assert_eq!(replace_apostrophes("it's", "&#8217;"), "it&#8217;s");
    }

    #[test]
    fn apostrophe_digit_after_not_converted() {
        assert_eq!(replace_apostrophes("3'4\"", "&#8217;"), "3'4\"");
    }

    #[test]
    fn apostrophe_quotes_around_word() {
        assert_eq!(replace_apostrophes("'word'", "&#8217;"), "'word'");
    }

    #[test]
    fn apostrophe_escaped_stripped() {
        assert_eq!(replace_apostrophes("Olaf\\'s", "&#8217;"), "Olaf's");
    }

    #[test]
    fn apostrophe_escaped_contraction() {
        assert_eq!(replace_apostrophes("don\\'t", "&#8217;"), "don't");
    }

    #[test]
    fn apostrophe_escape_outside_word_context_preserved() {
        assert_eq!(replace_apostrophes("test \\'s", "&#8217;"), "test \\'s");
    }

    #[test]
    fn apostrophe_decade() {
        assert_eq!(replace_apostrophes("1990's", "&#8217;"), "1990&#8217;s");
    }
}
