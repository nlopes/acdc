//! Render resolved inline content as HTML.
//!
//! Plain and verbatim text retain the parser's formatting and typography decisions.
//! Block substitutions control output escaping. Raw text uses its own profile,
//! including intentional HTML. IDs and attribute values are escaped when written.

use std::{
    io::{self, Write},
    rc::Rc,
};

use acdc_converters_core::{
    InlineTextTransform, TraversalContext,
    link::{autolink_fallback, link_fallback, mailto_fallback, mailto_target},
    substitutions::{Replacements, TextBoundaries, apply_passthrough_replacements_iter},
    xref::{XrefDisplay, interdocument_xref, resolve_xref},
};
use acdc_parser::{
    StemNotation, Substitution,
    ast::{
        AnchorRef, AttributeValueRef, AttributesRef, AutolinkRef, BoldRef, ButtonRef, CalloutRef,
        ContentRef, CrossReferenceRef, CurvedApostropheRef, CurvedQuotationRef, ElementIdRef,
        FootnoteRef, HighlightRef, IconRef, IndexTermRef, IndexTermRelationshipRef, InlineImageRef,
        InlineKind, InlineRef, ItalicRef, KeyboardRef, LinkRef, MailtoRef, MenuRef, MonospaceRef,
        PassRef, PlainRef, RawRef, StemRef, SubscriptRef, SuperscriptRef, TokensRef, UrlRef,
        VerbatimRef,
    },
};

use crate::{
    Error, HtmlVariant, HtmlVisitor, IndexCatalogRelationship, IndexTermLabel, RenderOptions,
    constants::{encode_html_entities, escape_ampersands},
    image::image_link_control_attributes,
    image_helpers::attribute_scalar,
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

/// Escape a URL for a quoted HTML attribute.
pub(crate) fn escape_href(url: &str) -> String {
    escape_attribute(url)
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

pub(crate) fn escape_role(role: &str) -> String {
    // Roles retain source entity references, including an authored &amp;.
    escape_ampersands(role)
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    let role = (!role.is_empty()).then(|| escape_role(&role));
    match (bare, role) {
        (true, Some(role)) => format!(" class=\"bare {role}\""),
        (true, None) => " class=\"bare\"".to_string(),
        (false, Some(role)) => format!(" class=\"{role}\""),
        (false, None) => String::new(),
    }
}

fn link_id_attr(id: Option<ElementIdRef<'_>>) -> String {
    id.map_or_else(String::new, |id| {
        format!(" id=\"{}\"", escape_attribute(id.as_str()))
    })
}

/// Apply hyperlink window and relationship controls, retaining the `target` alias.
fn link_control_attributes(attributes: AttributesRef<'_>) -> String {
    let get_str = |key: &str| {
        attributes.get(key).and_then(|value| {
            if let AttributeValueRef::Text(s) = value {
                (!s.is_empty()).then_some(s)
            } else {
                None
            }
        })
    };
    let window = get_str("window").or_else(|| get_str("target"));
    let has_option = |name| matches!(attributes.get("opts"), Some(AttributeValueRef::Tokens(options)) if options.contains(name));
    let nofollow = has_option("nofollow");
    let noopener = window.is_some_and(|window| window == "_blank" || has_option("noopener"));
    let mut controls = window.map_or_else(String::new, |window| {
        format!(" target=\"{}\"", escape_attribute(window))
    });
    match (nofollow, noopener) {
        (true, true) => controls.push_str(" rel=\"nofollow noopener\""),
        (true, false) => controls.push_str(" rel=\"nofollow\""),
        (false, true) => controls.push_str(" rel=\"noopener\""),
        (false, false) => {}
    }
    controls
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
    let role = role.map(escape_role);
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

fn link_label_needs_split(node: InlineRef<'_>, in_reference: bool, index: bool) -> bool {
    let children = match node.kind() {
        InlineKind::Footnote(_)
        | InlineKind::Link(_)
        | InlineKind::Url(_)
        | InlineKind::Mailto(_)
        | InlineKind::Autolink(_)
        | InlineKind::Anchor(_) => return true,
        // Automatic references already being resolved render as text to break cycles.
        InlineKind::CrossReference(n) if !n.text().is_empty() || !in_reference => return true,
        InlineKind::Image(n) => return n.attributes().contains("link"),
        InlineKind::CrossReference(n) => n.text(),
        InlineKind::Bold(n) => n.content(),
        InlineKind::Italic(n) => n.content(),
        InlineKind::Monospace(n) => n.content(),
        InlineKind::Highlight(n) => n.content(),
        InlineKind::Subscript(n) => n.content(),
        InlineKind::Superscript(n) => n.content(),
        InlineKind::CurvedQuotation(n) => n.content(),
        InlineKind::CurvedApostrophe(n) => n.content(),
        InlineKind::IndexTerm(_) if index => return true,
        InlineKind::IndexTerm(n) if n.is_visible() => n.term(),
        InlineKind::Plain(_)
        | InlineKind::Raw(_)
        | InlineKind::Verbatim(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::CharacterReference(_)
        | InlineKind::Symbol(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Callout(_)
        | InlineKind::Icon(_)
        | InlineKind::Keyboard(_)
        | InlineKind::Button(_)
        | InlineKind::Menu(_)
        | InlineKind::Pass(_)
        | InlineKind::Stem(_)
        | InlineKind::IndexTerm(_)
        | _ => return false,
    };
    children
        .inlines()
        .any(|node| link_label_needs_split(node, in_reference, index))
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn visit_inline_ref(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        let saved = self.render_options.in_inline_span;
        if acdc_converters_core::visitor::is_formatting_span(node) {
            self.render_options.in_inline_span = true;
        }
        let options = self.render_options.clone();
        let subs = self.current_subs.clone();
        let result = self.render_inline_node(traversal, node, &options, &subs);
        self.render_options.in_inline_span = saved;
        result
    }

    pub(crate) fn visit_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        self.visit_inline_refs(traversal, content.inlines())
    }

    pub(crate) fn visit_inline_refs<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
    ) -> Result<(), Error> {
        let previous_boundaries = self.text_boundaries;
        let siblings = nodes.clone();
        let result = (|| {
            for (index, node) in nodes.enumerate() {
                let boundaries = if self.render_options.in_inline_span {
                    TextBoundaries::NONE
                } else {
                    previous_boundaries
                };
                self.text_boundaries = boundaries
                    .with_ordinary_replacements(false)
                    .for_inline_refs(
                        Some(node),
                        siblings.clone().take(index).rev(),
                        siblings.clone().skip(index + 1),
                    );
                self.visit_inline_ref(traversal, node)?;
            }
            self.close_inline_link()
        })();
        self.text_boundaries = previous_boundaries;
        result
    }

    /// Render inline content without nesting generated anchors inside label links.
    pub(crate) fn render_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
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
        node: InlineRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        match node.kind() {
            InlineKind::Plain(p) => self.render_plain(p, options, subs),
            InlineKind::CharacterReference(reference) => {
                write!(self.writer_mut(), "{}", reference.kind())?;
                Ok(())
            }
            InlineKind::Symbol(symbol) => {
                write!(
                    self.writer_mut(),
                    "{}",
                    replacements().symbol(symbol.kind())
                )?;
                Ok(())
            }
            InlineKind::Raw(r) => self.render_raw(r),
            InlineKind::Verbatim(v) => self.render_verbatim(v, subs),
            InlineKind::Callout(c) => self.render_callout_ref(traversal, c),
            InlineKind::Bold(b) => self.render_bold(traversal, b, options),
            InlineKind::Italic(i) => self.render_italic(traversal, i, options),
            InlineKind::Highlight(h) => self.render_highlight(traversal, h, options),
            InlineKind::Monospace(m) => self.render_monospace(traversal, m, options),
            InlineKind::CurvedQuotation(c) => self.render_curved_quotation(traversal, c),
            InlineKind::CurvedApostrophe(c) => self.render_curved_apostrophe(traversal, c),
            InlineKind::StandaloneCurvedApostrophe(_) => self.render_standalone_apostrophe(),
            InlineKind::Superscript(s) => self.render_superscript(traversal, s),
            InlineKind::Subscript(s) => self.render_subscript(traversal, s),
            InlineKind::LineBreak(_) => {
                writeln!(self.writer_mut(), "<br>")?;
                Ok(())
            }
            InlineKind::Anchor(anchor) if !options.toc_mode => {
                self.render_inline_anchor(traversal, anchor, options, subs)
            }
            // TOC links cannot contain nested anchors.
            InlineKind::Anchor(_) => Ok(()),
            InlineKind::Autolink(_)
            | InlineKind::Link(_)
            | InlineKind::Image(_)
            | InlineKind::Pass(_)
            | InlineKind::Url(_)
            | InlineKind::Mailto(_)
            | InlineKind::Footnote(_)
            | InlineKind::Button(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Stem(_)
            | InlineKind::Icon(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Menu(_)
            | InlineKind::IndexTerm(_) => self.render_inline_macro(traversal, node, options, subs),
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
        anchor: AnchorRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        write!(
            self.writer_mut(),
            "<a id=\"{}\"></a>",
            escape_attribute(anchor.id())
        )?;
        if !anchor.is_bibliography() {
            return Ok(());
        }

        if let Some(label) = anchor.bibliography_label() {
            let html = self.render_bibliography_inline_fragment(traversal, label, options, subs)?;
            write!(self.writer_mut(), "[{html}]")?;
            return Ok(());
        }

        write!(self.writer_mut(), "[{}]", escape_pcdata(anchor.id()))?;
        Ok(())
    }

    fn render_bibliography_inline_fragment(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: ContentRef<'_>,
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
        visitor.visit_content(traversal, inlines)?;
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
        content: ContentRef<'_>,
        basic: bool,
    ) -> Result<(), Error> {
        if !basic {
            write_tag_with_attrs(self.writer_mut(), tag, id, role)?;
        }
        self.visit_content(traversal, content)?;
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
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        let (open_entity, close_entity) = entities;
        let has_attrs = id.is_some() || role.is_some();
        if has_attrs {
            write_tag_with_attrs(self.writer_mut(), "span", id, role)?;
        }
        self.open_inline_link()?;
        write!(self.writer_mut(), "{open_entity}")?;
        self.visit_content(traversal, content)?;
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
        let text = escape_inline_text(p.text(), subs);
        let w = self.writer_mut();
        if options.hardbreaks {
            write!(w, "{}", text.replace('\n', "<br>"))?;
        } else {
            write!(w, "{text}")?;
        }
        Ok(())
    }

    fn render_raw(&mut self, r: RawRef<'_>) -> Result<(), Error> {
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

    fn render_verbatim(&mut self, v: VerbatimRef<'_>, subs: &[Substitution]) -> Result<(), Error> {
        write!(self.writer_mut(), "{}", escape_inline_text(v.text(), subs))?;
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
        span: BoldRef<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        self.render_simple_quote(
            traversal,
            "strong",
            span.id().map(ElementIdRef::as_str),
            span.role(),
            span.content(),
            options.inlines_basic,
        )
    }

    fn render_italic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        span: ItalicRef<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        self.render_simple_quote(
            traversal,
            "em",
            span.id().map(ElementIdRef::as_str),
            span.role(),
            span.content(),
            options.inlines_basic,
        )
    }

    fn render_monospace(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        span: MonospaceRef<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        self.render_simple_quote(
            traversal,
            "code",
            span.id().map(ElementIdRef::as_str),
            span.role(),
            span.content(),
            options.inlines_basic,
        )
    }

    fn render_highlight(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        span: HighlightRef<'_>,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        // Warn about deprecated built-in roles.
        if let Some(role) = span.role() {
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
        let use_s_tag = is_semantic && span.role() == Some("line-through");
        // Asciidoctor treats an ID or role as an attributed span rather than a mark.
        let tag = if use_s_tag {
            "s"
        } else if span.id().is_some() || span.role().is_some() {
            "span"
        } else {
            "mark"
        };
        // The semantic <s> tag replaces the line-through role.
        let tag_role = if use_s_tag { None } else { span.role() };

        if !options.inlines_basic {
            write_tag_with_attrs(
                self.writer_mut(),
                tag,
                span.id().map(ElementIdRef::as_str),
                tag_role,
            )?;
        }
        self.visit_content(traversal, span.content())?;
        if !options.inlines_basic {
            write!(self.writer_mut(), "</{tag}>")?;
        }
        Ok(())
    }

    fn render_curved_quotation(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        span: CurvedQuotationRef<'_>,
    ) -> Result<(), Error> {
        self.render_curved(
            traversal,
            ("&ldquo;", "&rdquo;"),
            span.id().map(ElementIdRef::as_str),
            span.role(),
            span.content(),
        )
    }

    fn render_curved_apostrophe(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        span: CurvedApostropheRef<'_>,
    ) -> Result<(), Error> {
        self.render_curved(
            traversal,
            ("&lsquo;", "&rsquo;"),
            span.id().map(ElementIdRef::as_str),
            span.role(),
            span.content(),
        )
    }

    fn render_standalone_apostrophe(&mut self) -> Result<(), Error> {
        write!(self.writer_mut(), "&rsquo;")?;
        Ok(())
    }

    fn render_superscript(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        span: SuperscriptRef<'_>,
    ) -> Result<(), Error> {
        // Note: superscript doesn't check inlines_basic (pass false to preserve behavior).
        self.render_simple_quote(
            traversal,
            "sup",
            span.id().map(ElementIdRef::as_str),
            span.role(),
            span.content(),
            false,
        )
    }

    fn render_subscript(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        span: SubscriptRef<'_>,
    ) -> Result<(), Error> {
        // Note: subscript doesn't check inlines_basic (pass false to preserve behavior).
        self.render_simple_quote(
            traversal,
            "sub",
            span.id().map(ElementIdRef::as_str),
            span.role(),
            span.content(),
            false,
        )
    }

    /// Render an inline macro by dispatching to the per-variant renderer.
    fn render_inline_macro(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        m: InlineRef<'_>,
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
        match m.kind() {
            InlineKind::Autolink(al) => self.render_autolink(al, options),
            InlineKind::Link(l) => self.render_link(traversal, l, options, subs),
            InlineKind::Image(i) => self.render_inline_image(traversal, i),
            InlineKind::Pass(p) => self.render_pass(p),
            InlineKind::Url(u) => self.render_url(traversal, u, options, subs),
            InlineKind::Mailto(m) => self.render_mailto(traversal, m, options, subs),
            InlineKind::Footnote(f) => self.render_footnote(f, options),
            InlineKind::Button(b) => self.render_button(traversal, b),
            InlineKind::CrossReference(xref) => self.render_xref(traversal, xref, options, subs),
            InlineKind::Stem(s) => self.render_stem(traversal, s),
            InlineKind::Icon(i) => self.render_icon(traversal, i),
            InlineKind::Keyboard(k) => self.render_keyboard(traversal, k),
            InlineKind::Menu(menu) => self.render_menu(traversal, menu),
            InlineKind::IndexTerm(it) => self.render_indexterm(traversal, it, options, subs),
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::CharacterReference(_)
            | InlineKind::Symbol(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Callout(_)
            | _ => Err(io::Error::new(
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
        l: LinkRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target_str = l.target().to_string();
        let fallback = escape_href(link_fallback(&target_str, l.hides_uri_scheme()));

        if options.inlines_basic || options.toc_mode {
            if l.text().is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in l.text().inlines() {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let link = l;
        let id_attr = link_id_attr(link.id());
        let class_attr = link_class_attr(link.roles(), l.text().is_empty());
        let target_attr = link_control_attributes(link.attributes());
        let tooltip = tooltip_attribute(link.tooltip());
        self.with_inline_link(
            InlineLink::new(
                escape_href(&target_str),
                id_attr,
                format!("{class_attr}{target_attr}{tooltip}"),
            ),
            |visitor| {
                if l.text().is_empty() {
                    visitor.open_inline_link()?;
                    write!(visitor.writer_mut(), "{fallback}")?;
                } else {
                    for inline in l.text().inlines() {
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
        image: InlineImageRef<'_>,
    ) -> Result<(), Error> {
        let is_semantic = self.processor.variant() == HtmlVariant::Semantic;
        let markup = self.image_markup(
            traversal,
            image.source(),
            image.attributes(),
            image.options(),
            image.alt(),
            image.tooltip(),
        )?;
        let mut classes = vec!["image"];
        if let Some(AttributeValueRef::Text(float)) = image.attributes().get("float") {
            classes.push(float);
        }
        classes.extend(image.roles().iter());
        let has_wrapper = !is_semantic || classes.len() > 1;
        let w = self.writer_mut();
        if has_wrapper {
            write!(w, "<span class=\"{}\">", escape_role(&classes.join(" ")))?;
        }

        let link = image.attributes().get("link").and_then(attribute_scalar);
        let link_controls = image_link_control_attributes(image.attributes(), image.options());
        if let Some(link) = link {
            write!(
                w,
                "<a class=\"image\" href=\"{}\"{link_controls}>",
                escape_attribute(link),
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

    fn render_pass(&mut self, p: PassRef<'_>) -> Result<(), Error> {
        let Some(text) = p.text() else { return Ok(()) };
        let text = if p
            .substitutions()
            .any(|sub| sub == Substitution::SpecialChars)
        {
            encode_html_entities(
                &escape_ampersands(text)
                    .replace('>', "&gt;")
                    .replace('<', "&lt;"),
            )
        } else {
            text.to_owned()
        };
        write!(self.writer_mut(), "{text}")?;
        Ok(())
    }

    fn render_url(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        u: UrlRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target_str = u.target().to_string();
        let fallback = escape_href(link_fallback(&target_str, u.hides_uri_scheme()));

        if options.toc_mode {
            if u.text().is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in u.text().inlines() {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let link = u;
        let id_attr = link_id_attr(link.id());
        let class_attr = link_class_attr(link.roles(), u.text().is_empty());
        let target_attr = link_control_attributes(link.attributes());
        let tooltip = tooltip_attribute(link.tooltip());
        self.with_inline_link(
            InlineLink::new(
                escape_href(&target_str),
                id_attr,
                format!("{class_attr}{target_attr}{tooltip}"),
            ),
            |visitor| {
                if u.text().is_empty() {
                    visitor.open_inline_link()?;
                    write!(visitor.writer_mut(), "{fallback}")?;
                } else {
                    for inline in u.text().inlines() {
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
        m: MailtoRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target_str = m.target().to_string();
        // `mailto:` never emits `class="bare"` (asciidoctor's convention).
        let fallback = escape_href(mailto_fallback(&target_str));

        if options.toc_mode {
            if m.text().is_empty() {
                write!(self.writer_mut(), "{fallback}")?;
            } else {
                for inline in m.text().inlines() {
                    self.render_inline_node(traversal, inline, options, subs)?;
                }
            }
            return Ok(());
        }

        let link = m;
        let id_attr = link_id_attr(link.id());
        let class_attr = link_class_attr(link.roles(), false);
        let target_attr = link_control_attributes(link.attributes());
        let tooltip = tooltip_attribute(link.tooltip());
        self.with_inline_link(
            InlineLink::new(
                escape_href(&mailto_target(link)),
                id_attr,
                format!("{class_attr}{target_attr}{tooltip}"),
            ),
            |visitor| {
                if m.text().is_empty() {
                    visitor.open_inline_link()?;
                    write!(visitor.writer_mut(), "{fallback}")?;
                } else {
                    for inline in m.text().inlines() {
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
            write!(w, " id=\"_footnote_{}\"", escape_attribute(id))?;
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
        b: ButtonRef<'_>,
    ) -> Result<(), Error> {
        let content = self.content_html(traversal, b.label())?;
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
        xref: CrossReferenceRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        let target = xref.target();
        // An `xref:` macro's `role=` becomes the link's class, as in
        // asciidoctor, whatever the link text. An empty role adds no class,
        // matching how a `link:` macro's role is handled here.
        let mut class = if xref.roles().is_empty() {
            String::new()
        } else {
            format!(
                " class=\"{}\"",
                escape_role(&xref.roles().iter().collect::<Vec<_>>().join(" "))
            )
        };
        if !xref.target_is_local() {
            class.push_str(&link_control_attributes(xref.attributes()));
        }
        if xref.text().is_empty() {
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
                    InlineLink::new(escape_href(&format!("#{target}")), String::new(), class),
                    render,
                )
            } else {
                render(self)
            };
        }

        if options.inlines_basic || options.toc_mode {
            for inline in xref.text().inlines() {
                self.render_inline_node(traversal, inline, options, subs)?;
            }
            return Ok(());
        }

        let href = if !xref.target_is_local()
            && let Some((external_target, _)) = Self::interdocument_xref(traversal, target)
        {
            escape_href(&external_target)
        } else {
            escape_href(&format!("#{target}"))
        };
        self.with_inline_link(InlineLink::new(href, String::new(), class), |visitor| {
            for inline in xref.text().inlines() {
                visitor.render_inline_node(traversal, inline, options, subs)?;
            }
            Ok(())
        })
    }

    fn render_xref_display(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        display: XrefDisplay<'_, &[acdc_parser::InlineNode<'_>]>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<(), Error> {
        match display {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => {
                for inline in inlines {
                    self.render_inline_node(traversal, inline.into(), options, subs)?;
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
                    self.render_inline_node(traversal, inline.into(), options, subs)?;
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
                        node.into(),
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
                    self.render_inline_node(traversal, inline.into(), options, subs)?;
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
        icon: IconRef<'_>,
    ) -> Result<(), Error> {
        let markup = self.icon_markup(traversal, icon)?;
        write!(self.writer_mut(), "{markup}")?;
        Ok(())
    }

    fn render_keyboard(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        keyboard: KeyboardRef<'_>,
    ) -> Result<(), Error> {
        let semantic = self.processor.variant() == HtmlVariant::Semantic;
        let tag = if semantic { "kbd" } else { "span" };
        let class = if semantic { " class=\"key\"" } else { "" };
        let multiple = keyboard.keys().take(2).count() != 1;
        if multiple {
            write!(self.writer_mut(), "<{tag} class=\"keyseq\">")?;
        }
        for (index, key) in keyboard.keys().enumerate() {
            let content = self.content_html(traversal, key)?;
            if index > 0 {
                write!(self.writer_mut(), "+")?;
            }
            write!(self.writer_mut(), "<kbd{class}>{content}</kbd>")?;
        }
        if multiple {
            write!(self.writer_mut(), "</{tag}>")?;
        }
        Ok(())
    }

    fn render_menu(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        menu: MenuRef<'_>,
    ) -> Result<(), Error> {
        let semantic = self.processor.variant() == HtmlVariant::Semantic;
        let target = self.content_html(traversal, menu.target())?;
        let mut items = menu.items().peekable();
        if items.peek().is_none() {
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
        while let Some(item) = items.next() {
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
                let class = if items.peek().is_none() {
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
        content: ContentRef<'_>,
    ) -> Result<String, Error> {
        let mut visitor = HtmlVisitor::new(
            Vec::new(),
            Rc::clone(&self.processor),
            self.render_options.clone(),
            self.diagnostics.reborrow(),
        );
        visitor.current_subs.clone_from(&self.current_subs);
        visitor.visit_content(traversal, content)?;
        Ok(String::from_utf8(visitor.into_writer()).map_err(io::Error::other)?)
    }

    pub(crate) fn register_indexterm(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        it: IndexTermRef<'_>,
        options: &RenderOptions,
        subs: &[Substitution],
    ) -> Result<Option<String>, Error> {
        let it = it.catalog();
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
            let relationship = match it.relationship() {
                Some(IndexTermRelationshipRef::See { target }) => IndexCatalogRelationship::See(
                    self.render_index_term_label(traversal, target, options, subs)?,
                ),
                Some(IndexTermRelationshipRef::SeeAlso { targets }) => {
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
        it: IndexTermRef<'_>,
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
            let siblings = it.term().inlines();
            let result = it
                .term()
                .inlines()
                .enumerate()
                .try_for_each(|(index, inline)| {
                    self.text_boundaries = boundaries
                        .with_ordinary_replacements(false)
                        .for_inline_refs(
                            Some(inline),
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
        inlines: ContentRef<'_>,
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
        visitor.visit_content(traversal, inlines)?;
        let html = String::from_utf8(visitor.into_writer()).map_err(io::Error::other)?;
        Ok(IndexTermLabel {
            plain: InlineTextTransform::default().content_to_string(inlines),
            html,
        })
    }
}

fn escape_inline_text(text: &str, subs: &[Substitution]) -> String {
    if !subs.contains(&Substitution::SpecialChars) {
        return text.to_owned();
    }
    let text = text
        .replace('&', "&amp;")
        .replace('>', "&gt;")
        .replace('<', "&lt;");
    encode_html_entities(&text)
}

pub(crate) fn tooltip_attribute(content: Option<ContentRef<'_>>) -> String {
    content.map_or_else(String::new, |content| {
        let text = InlineTextTransform::default()
            .rendered()
            .decode_char_refs(true)
            .content_to_string(content);
        format!(
            " title=\"{}\"",
            crate::image_helpers::escape_image_text(&text)
        )
    })
}

impl<'doc, W: Write> acdc_converters_core::visitor::InlineVisitor<'doc>
    for HtmlVisitor<'doc, '_, W>
{
    type Error = Error;

    fn visit_inline_nodes(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        content: ContentRef<'_>,
    ) -> Result<(), Self::Error> {
        self.visit_content(traversal, content)
    }

    fn visit_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        node: InlineRef<'_>,
    ) -> Result<(), Self::Error> {
        self.visit_inline_ref(traversal, node)
    }
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
