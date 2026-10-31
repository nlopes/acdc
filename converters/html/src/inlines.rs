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

use std::{
    io::{self, Write},
    rc::Rc,
};

use acdc_converters_core::{
    InlineTextTransform, TraversalContext,
    link::{autolink_fallback, link_fallback, mailto_fallback, mailto_target},
    substitutions::{
        Replacements, TextBoundaries, apply_passthrough_replacements_iter,
        protect_replacement_escapes, restore_escaped_patterns, strip_backslash_escapes,
    },
    visitor::{Visitor, WritableVisitor},
    xref::{XrefDisplay, interdocument_xref, resolve_xref},
};
use acdc_parser::{
    Anchor, Bold, Button, CrossReference, CurvedApostrophe, CurvedQuotation, Highlight, Icon,
    IndexTerm, IndexTermRelationship, InlineImage, InlineMacro, InlineNode, Italic, Keyboard, Link,
    Mailto, Menu, Monospace, StemNotation, Subscript, Substitution, Superscript, Url,
    ast::{
        AttributeValueRef, AttributesRef, AutolinkRef, CalloutRef, ContentRef, ElementIdRef,
        FootnoteRef, InlineImageRef, InlineRef, LinkRef, MailtoRef, PassRef, PlainRef, RawRef,
        StemRef, TokensRef, UrlRef, VerbatimRef,
    },
    strip_quotes,
};

use crate::{
    Error, HtmlVariant, HtmlVisitor, IndexCatalogRelationship, IndexTermLabel, RenderOptions,
    constants::{encode_html_entities, escape_ampersands},
    image::image_link_control_attributes,
};

#[cfg(test)]
mod attribute_reads_tests;
#[cfg(test)]
mod text_reads_tests;

fn replacements() -> Replacements<'static> {
    let mut replacements = Replacements::unicode();
    replacements.em_dash_spaced = "&thinsp;&mdash;&thinsp;";
    replacements.em_dash_word_bounded = "&#8212;&#8203;";
    replacements.double_arrow_right = "&#8658;";
    replacements.double_arrow_left = "&#8656;";
    replacements.arrow_right = "&#8594;";
    replacements.arrow_left = "&#8592;";
    replacements.copyright = "&#169;";
    replacements.registered = "&#174;";
    replacements.trademark = "&#8482;";
    replacements.ellipsis = "&#8230;&#8203;";
    replacements.apostrophe = "&#8217;";
    replacements
}

fn passthrough_substitution_text(raw: RawRef<'_>, text_boundaries: TextBoundaries) -> String {
    let mut text = raw.text().to_owned();
    let mut applied_special_chars = false;

    for substitution in raw.substitutions() {
        match substitution {
            Substitution::SpecialChars => {
                // Keep source arrow characters until replacements have run.
                // Ampersands still follow the requested stage order so authored
                // references and generated typography escape independently.
                text = text.replace('&', "&amp;");
                applied_special_chars = true;
            }
            Substitution::Replacements => {
                text = apply_passthrough_replacements_iter(
                    &text,
                    raw.substitutions(),
                    &replacements(),
                    text_boundaries,
                )
                .into_owned();
            }
            Substitution::Attributes
            | Substitution::Macros
            | Substitution::PostReplacements
            | Substitution::Normal
            | Substitution::Verbatim
            | Substitution::Quotes
            | Substitution::Callouts
            | _ => {}
        }
    }

    if applied_special_chars {
        encode_html_entities(&text.replace('>', "&gt;").replace('<', "&lt;"))
    } else {
        text
    }
}

/// Escape `&` to `&amp;` in URL strings for use in HTML.
pub(crate) fn escape_href(url: &str) -> String {
    url.replace('&', "&amp;")
}

/// Escape literal text for HTML character data.
///
/// Only text that bypasses the inline pipeline needs this — anything the
/// pipeline renders is escaped there. Quotes are left alone, matching
/// asciidoctor's PCDATA encoding.
pub(crate) fn escape_pcdata(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape literal text for a double-quoted HTML attribute value.
pub(crate) fn escape_attribute(text: &str) -> String {
    escape_pcdata(text).replace('"', "&quot;")
}

fn raw_fragment_placeholder(index: usize) -> String {
    format!("\0{index}\0")
}

/// Build the ` class="..."` attribute (with leading space) for link-like anchors.
///
/// - `bare` adds the `bare` class when `true` (asciidoctor's convention for empty link text
///   on URL/link macros; `Mailto` passes `false` because it does not use `bare`).
/// - `roles` contributes additional class tokens.
/// - Returns an empty string when neither applies.
fn link_class_attr(roles: TokensRef<'_>, bare: bool) -> String {
    let mut role = String::new();
    for (index, token) in roles.iter().enumerate() {
        if index > 0 {
            role.push(' ');
        }
        role.push_str(token);
    }
    let role = strip_quotes(&role);
    let role = (!role.is_empty()).then_some(role);
    match (bare, role) {
        (true, Some(role)) => format!(" class=\"bare {role}\""),
        (true, None) => " class=\"bare\"".to_string(),
        (false, Some(role)) => format!(" class=\"{role}\""),
        (false, None) => String::new(),
    }
}

fn link_id_attr(id: Option<ElementIdRef<'_>>) -> String {
    id.map_or_else(String::new, |id| {
        format!(" id=\"{}\"", strip_quotes(id.as_str()))
    })
}

/// Extract `target` and `rel` attributes from `window` or `target` attribute values.
///
/// Maps the `AsciiDoc` `window` (preferred) or `target` attribute to HTML:
/// - `window=_blank` / `target=_blank` → `target="_blank" rel="noopener"`
/// - `window=<other>` / `target=<other>` → `target="<other>"`
fn window_attrs(attributes: AttributesRef<'_>) -> String {
    let get_str = |key: &str| {
        attributes.get(key).and_then(|value| {
            if let AttributeValueRef::Text(s) = value {
                let s = strip_quotes(s);
                (!s.is_empty()).then_some(s)
            } else {
                None
            }
        })
    };
    let window = get_str("window").or_else(|| get_str("target"));
    match window {
        Some("_blank") => " target=\"_blank\" rel=\"noopener\"".to_string(),
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
    // Roles retain source entity references; escape the quoted attribute without
    // turning an authored &amp; into a literal entity name.
    let role = role.map(|role| {
        escape_ampersands(role)
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    });
    match (id.map(escape_attribute), role) {
        (Some(id), Some(role)) => write!(writer, "<{tag} id=\"{id}\" class=\"{role}\">"),
        (Some(id), None) => write!(writer, "<{tag} id=\"{id}\">"),
        (None, Some(role)) => write!(writer, "<{tag} class=\"{role}\">"),
        (None, None) => write!(writer, "<{tag}>"),
    }
}

pub(crate) struct InlineLink {
    href: String,
    id: String,
    attributes: String,
    open: bool,
}

impl InlineLink {
    fn new(href: String, id: String, attributes: String) -> Self {
        Self {
            href,
            id,
            attributes,
            open: false,
        }
    }

    fn open(&mut self, writer: &mut dyn Write) -> io::Result<()> {
        if !self.open {
            write!(
                writer,
                "<a href=\"{}\"{}{}>",
                self.href, self.id, self.attributes
            )?;
            self.id.clear();
            self.open = true;
        }
        Ok(())
    }

    fn close(&mut self, writer: &mut dyn Write) -> io::Result<()> {
        if self.open {
            write!(writer, "</a>")?;
            self.open = false;
        } else if !self.id.is_empty() {
            // Keep the explicit target even when the first child needs its own anchor.
            write!(writer, "<span{}></span>", self.id)?;
            self.id.clear();
        }
        Ok(())
    }
}

fn link_label_needs_split(node: &InlineNode<'_>, in_reference: bool, index: bool) -> bool {
    let children = match node {
        InlineNode::Macro(
            InlineMacro::Footnote(_)
            | InlineMacro::Link(_)
            | InlineMacro::Url(_)
            | InlineMacro::Mailto(_)
            | InlineMacro::Autolink(_),
        )
        | InlineNode::InlineAnchor(_) => return true,
        // Automatic references already being resolved render as text to break cycles.
        InlineNode::Macro(InlineMacro::CrossReference(n))
            if !n.text.is_empty() || !in_reference =>
        {
            return true;
        }
        InlineNode::Macro(InlineMacro::Image(n)) => {
            return n.attributes().contains_key("link");
        }
        InlineNode::Macro(InlineMacro::CrossReference(n)) => &n.text,
        InlineNode::BoldText(n) => &n.content,
        InlineNode::ItalicText(n) => &n.content,
        InlineNode::MonospaceText(n) => &n.content,
        InlineNode::HighlightText(n) => &n.content,
        InlineNode::SubscriptText(n) => &n.content,
        InlineNode::SuperscriptText(n) => &n.content,
        InlineNode::CurvedQuotationText(n) => &n.content,
        InlineNode::CurvedApostropheText(n) => &n.content,
        InlineNode::Macro(InlineMacro::IndexTerm(_)) if index => return true,
        InlineNode::Macro(InlineMacro::IndexTerm(n)) if n.is_visible() => n.term(),
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        | _ => return false,
    };
    children
        .iter()
        .any(|node| link_label_needs_split(node, in_reference, index))
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    /// Render inline content without nesting generated anchors inside label links.
    pub(crate) fn render_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: &InlineNode,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        if let Some(mut link) = self.inline_link.take() {
            if link_label_needs_split(
                node,
                self.processor.xref_guard.is_resolving(),
                self.processor.generate_index(),
            ) {
                link.close(self.writer_mut())?;
                self.inline_link = Some(link);
                self.render_inline_node_content(traversal, node, options, subs)?;
                return self.close_inline_link();
            }
            link.open(self.writer_mut())?;
            let result = self.render_inline_node_content(traversal, node, options, subs);
            self.inline_link = Some(link);
            return result;
        }
        self.render_inline_node_content(traversal, node, options, subs)
    }

    pub(crate) fn close_inline_link(&mut self) -> Result<(), Error> {
        if let Some(mut link) = self.inline_link.take() {
            let result = link.close(self.writer_mut());
            self.inline_link = Some(link);
            result?;
        }
        Ok(())
    }

    fn open_inline_link(&mut self) -> Result<(), Error> {
        if let Some(mut link) = self.inline_link.take() {
            let result = link.open(self.writer_mut());
            self.inline_link = Some(link);
            result?;
        }
        Ok(())
    }

    fn with_inline_link(
        &mut self,
        link: InlineLink,
        render: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let previous = self.inline_link.replace(link);
        let result = render(self).and_then(|()| self.close_inline_link());
        self.inline_link = previous;
        result
    }

    fn render_inline_node_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: &InlineNode,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        // The parser selects formatting, including explicit attribute profiles
        // in code. Block substitutions still govern text escaping and typography.
        match node {
            InlineNode::PlainText(p) => self.render_plain(p.into(), options, subs),
            InlineNode::RawText(r) => self.render_raw(r.into()),
            InlineNode::VerbatimText(v) => self.render_verbatim(traversal, v.into(), options, subs),
            InlineNode::CalloutRef(c) => self.render_callout_ref(traversal, c.into()),
            InlineNode::BoldText(b) => self.render_bold(traversal, b, options),
            InlineNode::ItalicText(i) => self.render_italic(traversal, i, options),
            InlineNode::HighlightText(h) => self.render_highlight(traversal, h, options),
            InlineNode::MonospaceText(m) => self.render_monospace(traversal, m, options),
            InlineNode::CurvedQuotationText(c) => self.render_curved_quotation(traversal, c),
            InlineNode::CurvedApostropheText(c) => self.render_curved_apostrophe(traversal, c),
            InlineNode::StandaloneCurvedApostrophe(_) => self.render_standalone_apostrophe(),
            InlineNode::SuperscriptText(s) => self.render_superscript(traversal, s),
            InlineNode::SubscriptText(s) => self.render_subscript(traversal, s),
            InlineNode::Macro(m) => self.render_inline_macro(traversal, m, options, subs),
            InlineNode::LineBreak(_) => {
                writeln!(self.writer_mut(), "<br>")?;
                Ok(())
            }
            InlineNode::InlineAnchor(anchor) if !options.toc_mode => {
                self.render_inline_anchor(traversal, anchor, options, subs)
            }
            // TOC links cannot contain nested anchors.
            InlineNode::InlineAnchor(_) => Ok(()),
            _unsupported => {
                self.diagnostics.warn_with_advice(
                    "an unsupported parser inline variant was omitted from HTML output",
                    "Use another backend for this document and report the unsupported construct.",
                );
                Ok(())
            }
        }
    }

    fn render_inline_anchor(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        anchor: &Anchor<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        write!(self.writer_mut(), "<a id=\"{}\"></a>", anchor.id)?;
        if !anchor.is_bibliography() {
            return Ok(());
        }

        if let Some(label) = anchor.bibliography_label() {
            let html = self.render_bibliography_inline_fragment(traversal, label, options, subs)?;
            write!(self.writer_mut(), "[{html}]")?;
            return Ok(());
        }

        write!(self.writer_mut(), "[{}]", escape_pcdata(anchor.id))?;
        Ok(())
    }

    fn render_bibliography_inline_fragment(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode<'_>],
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<String, Error> {
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            Rc::clone(&self.processor),
            options.clone(),
            self.diagnostics.reborrow(),
        );
        visitor.current_subs = subs.to_vec();
        visitor.captured_raw_fragments = Some(Vec::new());
        visitor.visit_inline_nodes(traversal, inlines)?;
        let captured = visitor.captured_raw_fragments.take().unwrap_or_default();
        let html = String::from_utf8(visitor.into_writer()).map_err(io::Error::other)?;
        let mut html = html.replace('<', "&lt;").replace('>', "&gt;");
        for (index, raw) in captured.iter().enumerate() {
            html = html.replace(&raw_fragment_placeholder(index), raw);
        }
        Ok(html)
    }

    /// Render one of the simple inline-formatting nodes (bold, italic, monospace, sup, sub).
    fn render_simple_quote(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        tag: &str,
        id: Option<&str>,
        role: Option<&str>,
        content: &[InlineNode<'_>],
        basic: bool,
    ) -> Result<(), Error> {
        if !basic {
            write_tag_with_attrs(self.writer_mut(), tag, id, role)?;
        }
        self.visit_inline_nodes(traversal, content)?;
        if !basic {
            write!(self.writer_mut(), "</{tag}>")?;
        }
        Ok(())
    }

    /// Render a curved-quote or curved-apostrophe node.
    fn render_curved(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        entities: (&str, &str),
        id: Option<&str>,
        role: Option<&str>,
        content: &[InlineNode<'_>],
    ) -> Result<(), Error> {
        let (open_entity, close_entity) = entities;
        let has_attrs = id.is_some() || role.is_some();
        if has_attrs {
            write_tag_with_attrs(self.writer_mut(), "span", id, role)?;
        }
        self.open_inline_link()?;
        write!(self.writer_mut(), "{open_entity}")?;
        self.visit_inline_nodes(traversal, content)?;
        self.open_inline_link()?;
        write!(self.writer_mut(), "{close_entity}")?;
        self.close_inline_link()?;
        if has_attrs {
            write!(self.writer_mut(), "</span>")?;
        }
        Ok(())
    }

    fn render_plain(
        &mut self,
        p: PlainRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        if p.text().is_empty() {
            return Ok(());
        }
        // Attribute substitution already applied by the inline preprocessor during parsing.
        let content = p.text();
        // Escaped text must also bypass typography replacements.
        let effective_subs: &[Substitution] = if p.is_escaped() { &[] } else { subs };

        // Formatting is already represented by inline nodes. Plain text can
        // contain literal delimiters introduced after quote substitution.
        let text = substitution_text(content, effective_subs, options, self.text_boundaries());
        let w = self.writer_mut();
        if options.hardbreaks {
            write!(w, "{}", text.replace('\n', "<br>"))?;
        } else {
            write!(w, "{text}")?;
        }
        Ok(())
    }

    fn render_raw(&mut self, r: RawRef<'_>) -> Result<(), Error> {
        // RawText comes from passthroughs — attribute expansion was already handled (or
        // explicitly skipped) by the preprocessor. Do NOT apply block subs.
        let text = passthrough_substitution_text(r, self.text_boundaries());
        if let Some(captured) = self.captured_raw_fragments.as_mut() {
            let index = captured.len();
            captured.push(text);
            write!(self.writer_mut(), "{}", raw_fragment_placeholder(index))?;
        } else {
            write!(self.writer_mut(), "{text}")?;
        }
        Ok(())
    }

    fn render_verbatim(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        v: VerbatimRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        // The parser has already applied quotes and attributes in the requested order.
        // Re-parsing this text would activate markup introduced by late attributes.
        let verbatim_options = RenderOptions {
            inlines_verbatim: true,
            ..options.clone()
        };

        let text = substitution_text(v.text(), subs, &verbatim_options, self.text_boundaries());
        write!(self.writer_mut(), "{text}")?;
        Ok(())
    }

    fn render_callout_ref(
        &mut self,
        traversal: &TraversalContext<'_>,
        callout: CalloutRef<'_>,
    ) -> Result<(), Error> {
        let html =
            crate::callouts::marker_html(callout.number(), crate::callouts::font_icons(traversal));
        self.writer_mut().write_all(html.as_bytes())?;
        Ok(())
    }

    fn render_bold(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        b: &Bold<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        self.render_simple_quote(
            traversal,
            "strong",
            b.id.as_ref().map(|id| id.id),
            b.role,
            &b.content,
            options.inlines_basic,
        )
    }

    fn render_italic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        i: &Italic<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        self.render_simple_quote(
            traversal,
            "em",
            i.id.as_ref().map(|id| id.id),
            i.role,
            &i.content,
            options.inlines_basic,
        )
    }

    fn render_monospace(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        m: &Monospace<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        self.render_simple_quote(
            traversal,
            "code",
            m.id.as_ref().map(|id| id.id),
            m.role,
            &m.content,
            options.inlines_basic,
        )
    }

    fn render_highlight(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        h: &Highlight<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        // Warn about deprecated built-in roles.
        if let Some(role) = h.role {
            for r in role
                .split_whitespace()
                .filter(|r| matches!(*r, "big" | "small"))
            {
                self.diagnostics.warn_with_advice(
                    format!("deprecated role `{r}`"),
                    "Replace the deprecated built-in role with explicit passthrough markup or CSS.",
                );
            }
        }

        let is_semantic = self.processor.variant() == HtmlVariant::Semantic;
        let use_s_tag = is_semantic && h.role == Some("line-through");
        // Asciidoctor treats an ID or role as an attributed span rather than a mark.
        let tag = if use_s_tag {
            "s"
        } else if h.id.is_some() || h.role.is_some() {
            "span"
        } else {
            "mark"
        };
        // <s> gets no role attribute (h.role is consumed by the tag choice itself).
        let tag_role = if use_s_tag { None } else { h.role };

        if !options.inlines_basic {
            write_tag_with_attrs(
                self.writer_mut(),
                tag,
                h.id.as_ref().map(|id| id.id),
                tag_role,
            )?;
        }
        self.visit_inline_nodes(traversal, &h.content)?;
        if !options.inlines_basic {
            write!(self.writer_mut(), "</{tag}>")?;
        }
        Ok(())
    }

    fn render_curved_quotation(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        c: &CurvedQuotation<'_>,
    ) -> Result<(), Error> {
        self.render_curved(
            traversal,
            ("&ldquo;", "&rdquo;"),
            c.id.as_ref().map(|id| id.id),
            c.role,
            &c.content,
        )
    }

    fn render_curved_apostrophe(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        c: &CurvedApostrophe<'_>,
    ) -> Result<(), Error> {
        self.render_curved(
            traversal,
            ("&lsquo;", "&rsquo;"),
            c.id.as_ref().map(|id| id.id),
            c.role,
            &c.content,
        )
    }

    fn render_standalone_apostrophe(&mut self) -> Result<(), Error> {
        write!(self.writer_mut(), "&rsquo;")?;
        Ok(())
    }

    fn render_superscript(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        s: &Superscript<'_>,
    ) -> Result<(), Error> {
        // Note: superscript doesn't check inlines_basic (pass false to preserve behavior).
        self.render_simple_quote(
            traversal,
            "sup",
            s.id.as_ref().map(|id| id.id),
            s.role,
            &s.content,
            false,
        )
    }

    fn render_subscript(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        s: &Subscript<'_>,
    ) -> Result<(), Error> {
        // Note: subscript doesn't check inlines_basic (pass false to preserve behavior).
        self.render_simple_quote(
            traversal,
            "sub",
            s.id.as_ref().map(|id| id.id),
            s.role,
            &s.content,
            false,
        )
    }

    /// Render an inline macro by dispatching to the per-variant renderer.
    fn render_inline_macro(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        m: &InlineMacro,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        if self.captured_raw_fragments.is_some() {
            let mut visitor = HtmlVisitor::new(
                Vec::new(),
                Rc::clone(&self.processor),
                options.clone(),
                self.diagnostics.reborrow(),
            );
            visitor.current_subs = subs.to_vec();
            visitor.render_inline_macro(traversal, m, options, subs)?;
            let html = String::from_utf8(visitor.into_writer()).map_err(io::Error::other)?;
            if let Some(captured) = self.captured_raw_fragments.as_mut() {
                let index = captured.len();
                captured.push(html);
                write!(self.writer_mut(), "{}", raw_fragment_placeholder(index))?;
            }
            return Ok(());
        }
        match m {
            InlineMacro::Autolink(al) => self.render_autolink(al.into(), options),
            InlineMacro::Link(l) => self.render_link(traversal, l, options, subs),
            InlineMacro::Image(i) => self.render_inline_image(traversal, i),
            InlineMacro::Pass(p) => self.render_pass(p.into(), options, subs),
            InlineMacro::Url(u) => self.render_url(traversal, u, options, subs),
            InlineMacro::Mailto(m) => self.render_mailto(traversal, m, options, subs),
            InlineMacro::Footnote(f) => self.render_footnote(f.into(), options),
            InlineMacro::Button(b) => self.render_button(traversal, b),
            InlineMacro::CrossReference(xref) => self.render_xref(traversal, xref, options, subs),
            InlineMacro::Stem(s) => self.render_stem(traversal, s.into()),
            InlineMacro::Icon(i) => self.render_icon(traversal, i),
            InlineMacro::Keyboard(k) => self.render_keyboard(traversal, k),
            InlineMacro::Menu(menu) => self.render_menu(traversal, menu),
            InlineMacro::IndexTerm(it) => self.render_indexterm(traversal, it, options, subs),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("Unsupported inline macro: {m:?}"),
            )
            .into()),
        }
    }

    fn render_autolink(
        &mut self,
        al: AutolinkRef<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        let w = self.writer_mut();
        let href_str = al.target().to_string();
        let (display_text, angle_brackets) =
            autolink_fallback(&href_str, al.is_bracketed(), al.hides_uri_scheme());
        let display_text = escape_href(display_text);

        if options.inlines_basic || options.toc_mode {
            if angle_brackets {
                write!(w, "&lt;")?;
            }
            write!(w, "{display_text}")?;
            if angle_brackets {
                write!(w, "&gt;")?;
            }
        } else if angle_brackets {
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
        traversal: &mut TraversalContext<'a>,
        l: &Link<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target_str = l.target.to_string();
        let fallback = escape_href(link_fallback(&target_str, l.hides_uri_scheme()));

        if options.inlines_basic || options.toc_mode {
            if l.text.is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in &l.text {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let link = LinkRef::from(l);
        let id_attr = link_id_attr(link.id());
        let class_attr = link_class_attr(link.roles(), l.text.is_empty());
        let target_attr = window_attrs(link.attributes());
        let tooltip = tooltip_attribute(link.tooltip());
        self.with_inline_link(
            InlineLink::new(
                escape_href(&target_str),
                id_attr,
                format!("{class_attr}{target_attr}{tooltip}"),
            ),
            |visitor| {
                if l.text.is_empty() {
                    visitor.open_inline_link()?;
                    write!(visitor.writer_mut(), "{fallback}")?;
                } else {
                    for inline in &l.text {
                        visitor.render_inline_node(traversal, inline, options, subs)?;
                    }
                }
                Ok(())
            },
        )
    }

    fn render_inline_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        i: &InlineImage<'_>,
    ) -> Result<(), Error> {
        let is_semantic = self.processor.variant() == HtmlVariant::Semantic;
        let markup = self.image_markup(
            traversal,
            &i.source,
            i.attributes(),
            i.options(),
            i.alt.as_ref(),
            i.tooltip.as_ref(),
        )?;
        let mut classes = vec!["image"];
        if let Some(acdc_parser::AttributeValue::String(float)) = i.attributes().get("float") {
            classes.push(float.as_ref());
        }
        classes.extend(&i.metadata.roles);
        let has_wrapper = !is_semantic || classes.len() > 1;
        let w = self.writer_mut();
        if has_wrapper {
            write!(
                w,
                "<span class=\"{}\">",
                escape_attribute(&classes.join(" "))
            )?;
        }

        let link = i.attributes().get("link");
        let image = InlineImageRef::from(i);
        let link_controls = image_link_control_attributes(image.attributes(), image.options());
        if let Some(link) = link {
            write!(
                w,
                "<a class=\"image\" href=\"{}\"{link_controls}>",
                escape_attribute(&link.to_string()),
            )?;
        }

        write!(w, "{markup}")?;

        if link.is_some() {
            write!(w, "</a>")?;
        }
        if has_wrapper {
            write!(w, "</span>")?;
        }
        Ok(())
    }

    fn render_pass(
        &mut self,
        p: PassRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let Some(text) = p.text() else { return Ok(()) };
        // Only escape when SpecialChars substitution is enabled (pass:c[]).
        // Otherwise output raw HTML (pass:[], +++...+++).
        if p.substitutions()
            .any(|sub| sub == Substitution::SpecialChars)
        {
            let text = substitution_text(text, subs, options, self.text_boundaries());
            write!(self.writer_mut(), "{text}")?;
        } else {
            write!(self.writer_mut(), "{text}")?;
        }
        Ok(())
    }

    fn render_url(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        u: &Url<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target_str = u.target.to_string();
        let fallback = escape_href(link_fallback(&target_str, u.hides_uri_scheme()));

        if options.toc_mode {
            if u.text.is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in &u.text {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let link = UrlRef::from(u);
        let id_attr = link_id_attr(link.id());
        let class_attr = link_class_attr(link.roles(), u.text.is_empty());
        let target_attr = window_attrs(link.attributes());
        let tooltip = tooltip_attribute(link.tooltip());
        self.with_inline_link(
            InlineLink::new(
                escape_href(&target_str),
                id_attr,
                format!("{class_attr}{target_attr}{tooltip}"),
            ),
            |visitor| {
                if u.text.is_empty() {
                    visitor.open_inline_link()?;
                    write!(visitor.writer_mut(), "{fallback}")?;
                } else {
                    for inline in &u.text {
                        visitor.render_inline_node(traversal, inline, options, subs)?;
                    }
                }
                Ok(())
            },
        )
    }

    fn render_mailto(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        m: &Mailto<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target_str = m.target.to_string();
        // `mailto:` never emits `class="bare"` (asciidoctor's convention).
        let fallback = escape_href(mailto_fallback(&target_str));

        if options.toc_mode {
            if m.text.is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in &m.text {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let link = MailtoRef::from(m);
        let id_attr = link_id_attr(link.id());
        let class_attr = link_class_attr(link.roles(), false);
        let target_attr = window_attrs(link.attributes());
        let tooltip = tooltip_attribute(link.tooltip());
        self.with_inline_link(
            InlineLink::new(
                escape_href(&mailto_target(link)),
                id_attr,
                format!("{class_attr}{target_attr}{tooltip}"),
            ),
            |visitor| {
                if m.text.is_empty() {
                    visitor.open_inline_link()?;
                    write!(visitor.writer_mut(), "{fallback}")?;
                } else {
                    for inline in &m.text {
                        visitor.render_inline_node(traversal, inline, options, subs)?;
                    }
                }
                Ok(())
            },
        )
    }

    fn render_footnote(
        &mut self,
        f: FootnoteRef<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        // Reused notes and copies in reference text must not repeat the original IDs.
        let is_ref = self.processor.xref_guard.is_resolving()
            || (f.id().is_some() && f.content().is_empty());
        let is_semantic = self.processor.variant() == HtmlVariant::Semantic;
        let w = self.writer_mut();
        let number = f.number();

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
        if !is_ref && let Some(id) = f.id() {
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

    fn render_button(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        b: &Button<'_>,
    ) -> Result<(), Error> {
        let content = self.content_html(traversal, &b.label)?;
        if self.processor.variant() == HtmlVariant::Semantic {
            write!(
                self.writer_mut(),
                "<kbd class=\"button\"><samp>{content}</samp></kbd>"
            )?;
        } else {
            write!(self.writer_mut(), "<b class=\"button\">{content}</b>")?;
        }
        Ok(())
    }

    fn render_xref(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        xref: &CrossReference<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target = xref.target;
        // An `xref:` macro's `role=` becomes the link's class, as in
        // asciidoctor, whatever the link text. An empty role adds no class,
        // matching how a `link:` macro's role is handled here.
        let class = if xref.metadata.roles.is_empty() {
            String::new()
        } else {
            format!(
                " class=\"{}\"",
                escape_attribute(&xref.metadata.roles.join(" "))
            )
        };
        if xref.text.is_empty() {
            // Resolve via the id -> reference map (sections + titled blocks):
            // xreflabel (from [[id,Custom Text]]) > caption style or target title >
            // fallback `[id]`.
            //
            // Reference text — a title or an xreflabel — goes through the inline
            // pipeline, so its formatting (`<code>`, bold, italic, ...) survives
            // inside the link and its characters are escaped, matching
            // asciidoctor. The `[id]` fallback is literal text, so it is escaped
            // here.
            //
            // The processor handle is cloned so the borrowed reference text and
            // the resolution guard both outlive the `&mut self` render calls.
            let processor = Rc::clone(&self.processor);
            let display = resolve_xref(
                processor.references.get(target),
                xref,
                &processor.xref_guard,
            );

            if let XrefDisplay::External(target) = &display {
                let Some((target, text)) = Self::interdocument_xref(traversal, target) else {
                    write!(self.writer_mut(), "{}", escape_pcdata(target))?;
                    return Ok(());
                };
                if options.inlines_basic || options.toc_mode {
                    write!(self.writer_mut(), "{}", escape_pcdata(&text))?;
                } else {
                    write!(
                        self.writer_mut(),
                        "<a href=\"{}\"{class}>{}</a>",
                        escape_href(&target),
                        escape_pcdata(&text)
                    )?;
                }
                return Ok(());
            }

            // A cross-reference inside another one's text cannot be a link: an
            // `<a>` does not nest. Everything else links, including an
            // unresolved target, matching asciidoctor.
            let linked = !(options.inlines_basic
                || options.toc_mode
                || matches!(display, XrefDisplay::Nested(_)));
            let render =
                |visitor: &mut Self| visitor.render_xref_display(traversal, display, options, subs);
            return if linked {
                self.with_inline_link(
                    InlineLink::new(format!("#{target}"), String::new(), class),
                    render,
                )
            } else {
                render(self)
            };
        }

        if options.inlines_basic || options.toc_mode {
            for inline in &xref.text {
                self.render_inline_node(traversal, inline, options, subs)?;
            }
            return Ok(());
        }

        let href = if !xref.target_is_local
            && let Some((external_target, _)) = Self::interdocument_xref(traversal, target)
        {
            escape_href(&external_target)
        } else {
            format!("#{target}")
        };
        self.with_inline_link(InlineLink::new(href, String::new(), class), |visitor| {
            for inline in &xref.text {
                visitor.render_inline_node(traversal, inline, options, subs)?;
            }
            Ok(())
        })
    }

    fn render_xref_display(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        display: XrefDisplay<'_, '_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        match display {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => {
                for inline in inlines {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
            }
            XrefDisplay::ShortCaption(prefix) => {
                self.open_inline_link()?;
                write!(self.writer_mut(), "{}", escape_pcdata(&prefix))?;
            }
            XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                self.open_inline_link()?;
                write!(self.writer_mut(), "{}, &#8220;", escape_pcdata(&prefix))?;
                for inline in inlines {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
                self.open_inline_link()?;
                write!(self.writer_mut(), "&#8221;")?;
            }
            XrefDisplay::Emphasized(prefix, inlines, _scope) => {
                if let Some(prefix) = prefix {
                    self.open_inline_link()?;
                    write!(self.writer_mut(), "{}, ", escape_pcdata(&prefix))?;
                }
                let split = inlines.iter().any(|node| {
                    link_label_needs_split(
                        node,
                        self.processor.xref_guard.is_resolving(),
                        self.processor.generate_index(),
                    )
                });
                if split {
                    self.close_inline_link()?;
                } else {
                    self.open_inline_link()?;
                }
                write!(self.writer_mut(), "<em>")?;
                for inline in inlines {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
                if split {
                    self.close_inline_link()?;
                }
                write!(self.writer_mut(), "</em>")?;
            }
            XrefDisplay::Fallback(text)
            | XrefDisplay::Unresolved(text)
            | XrefDisplay::Nested(text) => {
                self.open_inline_link()?;
                write!(self.writer_mut(), "{}", escape_pcdata(&text))?;
            }
            XrefDisplay::External(target) => {
                self.open_inline_link()?;
                write!(self.writer_mut(), "{}", escape_pcdata(&target))?;
            }
        }
        Ok(())
    }

    fn interdocument_xref(
        traversal: &TraversalContext<'a>,
        target: &str,
    ) -> Option<(String, String)> {
        let attributes = &traversal;
        let extension = attributes
            .get("relfilesuffix")
            .and_then(|value| value.text())
            .or_else(|| {
                attributes
                    .get("outfilesuffix")
                    .and_then(|value| value.text())
            })
            .unwrap_or("html");
        interdocument_xref(target, extension.strip_prefix('.').unwrap_or(extension))
    }

    fn render_stem(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        s: StemRef<'_>,
    ) -> Result<(), Error> {
        let forced = if self.processor.variant() == HtmlVariant::Semantic {
            traversal
                .get("html5s-force-stem-type")
                .and_then(|value| value.text())
                .and_then(|value| value.parse::<StemNotation>().ok())
        } else {
            None
        };
        let notation = forced.unwrap_or_else(|| s.notation());
        let w = self.writer_mut();
        let content = escape_pcdata(s.text());
        match notation {
            StemNotation::Latexmath => writeln!(w, "\\({content}\\)")?,
            StemNotation::Asciimath => writeln!(w, "\\${content}\\$")?,
        }
        Ok(())
    }

    fn render_icon(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        icon: &Icon<'_>,
    ) -> Result<(), Error> {
        let markup = self.icon_markup(traversal, icon)?;
        write!(self.writer_mut(), "{markup}")?;
        Ok(())
    }

    fn render_keyboard(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        keyboard: &Keyboard<'_>,
    ) -> Result<(), Error> {
        let semantic = self.processor.variant() == HtmlVariant::Semantic;
        let tag = if semantic { "kbd" } else { "span" };
        let class = if semantic { " class=\"key\"" } else { "" };
        if keyboard.keys.len() != 1 {
            write!(self.writer_mut(), "<{tag} class=\"keyseq\">")?;
        }
        for (index, key) in keyboard.keys.iter().enumerate() {
            let content = self.content_html(traversal, key)?;
            if index > 0 {
                write!(self.writer_mut(), "+")?;
            }
            write!(self.writer_mut(), "<kbd{class}>{content}</kbd>")?;
        }
        if keyboard.keys.len() != 1 {
            write!(self.writer_mut(), "</{tag}>")?;
        }
        Ok(())
    }

    fn render_menu(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        menu: &Menu<'_>,
    ) -> Result<(), Error> {
        let semantic = self.processor.variant() == HtmlVariant::Semantic;
        let target = self.content_html(traversal, &menu.target)?;
        if menu.items.is_empty() {
            if semantic {
                write!(
                    self.writer_mut(),
                    "<kbd class=\"menu\"><samp>{target}</samp></kbd>"
                )?;
            } else {
                write!(self.writer_mut(), "<b class=\"menuref\">{target}</b>")?;
            }
            return Ok(());
        }
        if semantic {
            write!(
                self.writer_mut(),
                "<kbd class=\"menuseq\"><kbd class=\"menu\"><samp>{target}</samp></kbd>"
            )?;
        } else {
            write!(
                self.writer_mut(),
                "<span class=\"menuseq\"><b class=\"menu\">{target}</b>"
            )?;
        }
        let font = acdc_converters_core::icon::IconMode::from_attributes(traversal)
            == acdc_converters_core::icon::IconMode::Font;
        for (index, item) in menu.items.iter().enumerate() {
            let content = self.content_html(traversal, item)?;
            if semantic {
                write!(
                    self.writer_mut(),
                    "&#160;<span class=\"caret\">&#8250;</span>&#32;<kbd class=\"menu\"><samp>{content}</samp></kbd>"
                )?;
            } else {
                if font {
                    write!(
                        self.writer_mut(),
                        "&#160;<i class=\"fa fa-angle-right caret\"></i> "
                    )?;
                } else {
                    write!(self.writer_mut(), "&#160;<b class=\"caret\">&#8250;</b> ")?;
                }
                let class = if index + 1 == menu.items.len() {
                    "menuitem"
                } else {
                    "submenu"
                };
                write!(self.writer_mut(), "<b class=\"{class}\">{content}</b>")?;
            }
        }
        write!(
            self.writer_mut(),
            "</{}>",
            if semantic { "kbd" } else { "span" }
        )?;
        Ok(())
    }

    pub(crate) fn content_html(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: &[InlineNode<'_>],
    ) -> Result<String, Error> {
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            Rc::clone(&self.processor),
            self.render_options.clone(),
            self.diagnostics.reborrow(),
        );
        visitor.current_subs.clone_from(&self.current_subs);
        visitor.visit_inline_nodes(traversal, content)?;
        Ok(String::from_utf8(visitor.into_writer()).map_err(io::Error::other)?)
    }

    pub(crate) fn register_indexterm(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        it: &IndexTerm<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<Option<String>, Error> {
        let it = it.catalog_entry();
        // Occurrence anchors serve the opted-in catalog; TOC copies are not
        // additional source occurrences.
        if !options.toc_mode && self.processor.generate_index() {
            let primary = self.render_index_term_label(traversal, it.term(), options, subs)?;
            let secondary = it
                .secondary()
                .map(|inlines| self.render_index_term_label(traversal, inlines, options, subs))
                .transpose()?;
            let tertiary = it
                .tertiary()
                .map(|inlines| self.render_index_term_label(traversal, inlines, options, subs))
                .transpose()?;
            let relationship = match it.relationship.as_ref() {
                Some(IndexTermRelationship::See { target }) => IndexCatalogRelationship::See(
                    self.render_index_term_label(traversal, target, options, subs)?,
                ),
                Some(IndexTermRelationship::SeeAlso { targets }) => {
                    IndexCatalogRelationship::SeeAlso(
                        targets
                            .iter()
                            .map(|target| {
                                self.render_index_term_label(traversal, target, options, subs)
                            })
                            .collect::<Result<_, _>>()?,
                    )
                }
                None | Some(_) => IndexCatalogRelationship::None,
            };
            let anchor_id = self.processor.clone().add_index_entry(
                primary,
                secondary,
                tertiary,
                relationship,
                self.current_section_title.clone(),
            );
            return Ok(Some(anchor_id));
        }

        Ok(None)
    }

    fn render_indexterm(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        it: &IndexTerm<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        if let Some(anchor_id) = self.register_indexterm(traversal, it, options, subs)? {
            write!(self.writer_mut(), "<a id=\"{anchor_id}\"></a>")?;
        }

        // Flow terms (visible): also output the term text.
        // Concealed terms: no visible text.
        if it.is_visible() {
            let boundaries = self.text_boundaries();
            let siblings = it.term().iter().map(InlineRef::from);
            let result = it
                .term()
                .iter()
                .enumerate()
                .try_for_each(|(index, inline)| {
                    self.text_boundaries = boundaries
                        .with_ordinary_replacements(subs.contains(&Substitution::Replacements))
                        .for_inline_refs(
                            Some(inline.into()),
                            siblings.clone().take(index).rev(),
                            siblings.clone().skip(index + 1),
                        );
                    self.render_inline_node(traversal, inline, options, subs)
                });
            self.text_boundaries = boundaries;
            result?;
        }
        Ok(())
    }

    fn render_index_term_label(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode<'_>],
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<IndexTermLabel, Error> {
        let mut catalog_options = options.clone();
        catalog_options.toc_mode = true;
        catalog_options.inlines_verbatim = false;
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            Rc::clone(&self.processor),
            catalog_options,
            self.diagnostics.reborrow(),
        );
        visitor.current_subs = subs.to_vec();
        visitor.visit_inline_nodes(traversal, inlines)?;
        let html = String::from_utf8(visitor.into_writer()).map_err(io::Error::other)?;
        Ok(IndexTermLabel {
            plain: InlineTextTransform::default().to_string(inlines),
            html,
        })
    }
}

fn substitution_text(
    text: &str,
    subs: &[Substitution],
    options: &RenderOptions,
    text_boundaries: TextBoundaries,
) -> String {
    debug_assert!(
        !text.is_empty(),
        "substitution_text called with empty text - caller should filter empty content"
    );
    if text.is_empty() {
        tracing::debug!(
            "substitution_text called with empty text - caller should filter empty content"
        );
        return String::new();
    }

    let should_escape = subs.contains(&Substitution::SpecialChars);

    // Determine if we should apply typography replacements
    // Based on substitutions list, skip for basic mode (passthrough)
    let should_apply_replacements =
        subs.contains(&Substitution::Replacements) && !options.inlines_basic;

    // Strip backslash escapes first (before any other processing)
    // Only needed when replacements are applied (escape sequences only matter for replacements)
    let text = if should_apply_replacements {
        if options.inlines_verbatim || !subs.contains(&Substitution::Quotes) {
            protect_replacement_escapes(text, true).into_owned()
        } else {
            strip_backslash_escapes(text)
        }
    } else {
        text.to_string()
    };

    // Escape source ampersands before replacements so entities produced by the
    // replacement table remain valid HTML.
    let text = if should_escape {
        escape_ampersands(&text)
    } else {
        text
    };

    // Apply all typography replacements (em-dashes, arrows, symbols, ellipsis, apostrophes)
    // This must happen after & escaping (replacements produce & entities) and before <> escaping
    let text = if should_apply_replacements {
        replacements().apply(&text, text_boundaries)
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

    if !should_escape {
        return text;
    }

    // Escape < and > after restore so that restored patterns (e.g., \=> → =>) keep literal chars
    let text = text.replace('>', "&gt;").replace('<', "&lt;");

    // Encode non-ASCII Unicode characters as HTML numeric entities
    // to match asciidoctor's output format
    encode_html_entities(&text)
}

pub(crate) fn tooltip_attribute(content: Option<ContentRef<'_>>) -> String {
    content.map_or_else(String::new, |content| {
        let text = InlineTextTransform::default()
            .rendered_replacements(true)
            .decode_char_refs(true)
            .content_to_string(content);
        format!(
            " title=\"{}\"",
            crate::image_helpers::escape_image_text(&text)
        )
    })
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
