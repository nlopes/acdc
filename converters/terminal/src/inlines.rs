use std::{
    borrow::Cow,
    io::{Error as IoError, ErrorKind, Write},
};

use acdc_converters_core::{
    Diagnostics, InlineTextTransform, TraversalContext, WarningSource, decode_numeric_char_refs,
    link::{autolink_fallback, link_fallback, mailto_fallback, mailto_target},
    substitutions::{Replacements, TextBoundaries, apply_passthrough_replacements_iter},
    xref::{XrefDisplay, resolve_reference},
};
use acdc_parser::{
    Substitution,
    ast::{
        ButtonRef, ContentRef, CrossReferenceRef, IconRef, IndexTermRef, IndexTermRelationshipRef,
        InlineImageRef, InlineKind, InlineRef, KeyboardRef, LinkRef, RawRef, ReferenceRef,
    },
};

use crossterm::{
    QueueableCommand,
    style::{Attribute, Color, Print, PrintStyledContent, SetAttribute, Stylize},
};

use crate::{
    Error, IndexCatalogRelationship, IndexTermEntry, IndexTermLabel, Processor,
    terminal_visitor::attribute_text,
};

/// Try to convert a character to its Unicode superscript equivalent.
/// Only digits and a few reliable symbols are mapped; letters are not included
/// because Unicode superscript coverage for letters is incomplete and inconsistent.
fn to_superscript(c: char) -> Option<char> {
    match c {
        '0' => Some('\u{2070}'),
        '1' => Some('\u{00B9}'),
        '2' => Some('\u{00B2}'),
        '3' => Some('\u{00B3}'),
        '4' => Some('\u{2074}'),
        '5' => Some('\u{2075}'),
        '6' => Some('\u{2076}'),
        '7' => Some('\u{2077}'),
        '8' => Some('\u{2078}'),
        '9' => Some('\u{2079}'),
        '+' => Some('\u{207A}'),
        '-' => Some('\u{207B}'),
        '=' => Some('\u{207C}'),
        '(' => Some('\u{207D}'),
        ')' => Some('\u{207E}'),
        _ => None,
    }
}

/// Try to convert a character to its Unicode subscript equivalent.
/// Only digits and a few reliable symbols are mapped; letters are not included
/// because Unicode subscript coverage for letters is incomplete and inconsistent.
fn to_subscript(c: char) -> Option<char> {
    match c {
        '0' => Some('\u{2080}'),
        '1' => Some('\u{2081}'),
        '2' => Some('\u{2082}'),
        '3' => Some('\u{2083}'),
        '4' => Some('\u{2084}'),
        '5' => Some('\u{2085}'),
        '6' => Some('\u{2086}'),
        '7' => Some('\u{2087}'),
        '8' => Some('\u{2088}'),
        '9' => Some('\u{2089}'),
        '+' => Some('\u{208A}'),
        '-' => Some('\u{208B}'),
        '=' => Some('\u{208C}'),
        '(' => Some('\u{208D}'),
        ')' => Some('\u{208E}'),
        _ => None,
    }
}

/// Try to convert a string entirely to Unicode superscript characters.
/// Returns `None` if any character cannot be converted.
fn try_to_unicode_superscript(text: &str) -> Option<String> {
    text.chars().map(to_superscript).collect()
}

/// Try to convert a string entirely to Unicode subscript characters.
/// Returns `None` if any character cannot be converted.
fn try_to_unicode_subscript(text: &str) -> Option<String> {
    text.chars().map(to_subscript).collect()
}

/// Render super/subscript content: try Unicode conversion first, fall back to
/// dim-styled text to subtly indicate super/subscript.
fn render_script_text<'a, W: Write + ?Sized>(
    nodes: ContentRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
    converter: fn(&str) -> Option<String>,
) -> Result<(), Error> {
    // Collect plain text to attempt Unicode conversion
    let plain: Option<String> = nodes
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(text) = node.kind() {
                Some(text.text())
            } else {
                None
            }
        })
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join(""));

    if let Some(ref text) = plain
        && let Some(converted) = converter(text)
    {
        write!(w, "{converted}")?;
        return Ok(());
    }

    // Fall back to dim-styled text
    w.queue(SetAttribute(Attribute::Dim))?;
    for node in nodes.inlines() {
        render_inline_node_to_writer(node, w, processor, traversal)?;
    }
    w.queue(SetAttribute(Attribute::NormalIntensity))?;
    Ok(())
}

/// Render a link label or script fallback, trimming surrounding whitespace.
fn render_inline_nodes_to_string<'a, 'inline>(
    nodes: impl Iterator<Item = InlineRef<'inline>>,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<String, Error> {
    Ok(render_inline_nodes_to_owned(nodes, processor, traversal)?
        .trim()
        .to_string())
}

/// Render inline nodes to a string, keeping surrounding whitespace.
///
/// Reference text is rendered with this variant: the space in `.A title` or
/// around a formatted span belongs to the text.
fn render_inline_nodes_to_owned<'a, 'inline>(
    nodes: impl Iterator<Item = InlineRef<'inline>>,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<String, Error> {
    let mut buffer = std::io::BufWriter::new(Vec::new());
    for node in nodes {
        render_inline_node_to_writer(node, &mut buffer, processor, traversal)?;
    }
    buffer.flush()?;
    Ok(String::from_utf8(buffer.into_inner()?)?)
}

pub(crate) fn render_content_with_styles_to_owned<'a>(
    nodes: ContentRef<'_>,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<String, Error> {
    let source = WarningSource::new("terminal");
    let mut warnings = Vec::new();
    let mut visitor = crate::TerminalVisitor::new(
        Vec::new(),
        processor,
        Diagnostics::new(&source, &mut warnings),
    );
    visitor.text_boundaries = TextBoundaries::NONE;
    visitor.write_content(traversal, nodes)?;
    Ok(String::from_utf8(visitor.into_writer())?)
}

fn write_raw_text<W: Write + ?Sized>(
    w: &mut W,
    raw: RawRef<'_>,
    boundaries: TextBoundaries,
) -> Result<(), Error> {
    let replaced = apply_passthrough_replacements_iter(
        raw.text(),
        raw.substitutions(),
        &Replacements::unicode(),
        boundaries,
    );
    let text = if raw.substitutions().last() == Some(Substitution::SpecialChars) {
        Cow::Borrowed(replaced.as_ref())
    } else {
        decode_numeric_char_refs(&replaced)
    };
    write!(w, "{text}")?;
    Ok(())
}

/// Render one node inside a span, where text edges are not paragraph edges.
fn render_inline_node_to_writer<'a, W: Write + ?Sized>(
    node: InlineRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    match node.kind() {
        InlineKind::Plain(p) => write!(w, "{}", p.text())?,
        InlineKind::CharacterReference(reference) => write!(
            w,
            "{}",
            acdc_converters_core::character_reference_text(reference.kind())
        )?,
        InlineKind::Raw(r) => {
            write_raw_text(w, r, TextBoundaries::NONE)?;
        }
        InlineKind::Verbatim(v) => write!(w, "{}", v.text())?,
        InlineKind::Italic(i) => {
            for inner in i.content().inlines() {
                render_inline_node_to_writer(inner, w, processor, traversal)?;
            }
        }
        InlineKind::Bold(b) => {
            for inner in b.content().inlines() {
                render_inline_node_to_writer(inner, w, processor, traversal)?;
            }
        }
        InlineKind::Highlight(h) => {
            for inner in h.content().inlines() {
                render_inline_node_to_writer(inner, w, processor, traversal)?;
            }
        }
        InlineKind::Monospace(m) => {
            for inner in m.content().inlines() {
                render_inline_node_to_writer(inner, w, processor, traversal)?;
            }
        }
        InlineKind::Url(_)
        | InlineKind::Link(_)
        | InlineKind::Mailto(_)
        | InlineKind::Autolink(_)
        | InlineKind::Footnote(_)
        | InlineKind::Button(_)
        | InlineKind::CrossReference(_)
        | InlineKind::Pass(_)
        | InlineKind::Image(_)
        | InlineKind::Icon(_)
        | InlineKind::Keyboard(_)
        | InlineKind::Menu(_)
        | InlineKind::Stem(_)
        | InlineKind::IndexTerm(_) => {
            render_inline_macro_to_writer(node, w, processor, traversal)?;
        }
        InlineKind::Superscript(s) => {
            render_script_text(
                s.content(),
                w,
                processor,
                traversal,
                try_to_unicode_superscript,
            )?;
        }
        InlineKind::Subscript(s) => {
            render_script_text(
                s.content(),
                w,
                processor,
                traversal,
                try_to_unicode_subscript,
            )?;
        }
        InlineKind::CurvedQuotation(c) => {
            write!(w, "\u{201C}")?;
            for inner in c.content().inlines() {
                render_inline_node_to_writer(inner, w, processor, traversal)?;
            }
            write!(w, "\u{201D}")?;
        }
        InlineKind::CurvedApostrophe(c) => {
            write!(w, "\u{2018}")?;
            for inner in c.content().inlines() {
                render_inline_node_to_writer(inner, w, processor, traversal)?;
            }
            write!(w, "\u{2019}")?;
        }
        InlineKind::Symbol(symbol) => write!(w, "{}", symbol.text())?,
        InlineKind::StandaloneCurvedApostrophe(_) => {
            write!(w, "\u{2019}")?;
        }
        InlineKind::LineBreak(_) => {
            writeln!(w)?;
        }
        InlineKind::Anchor(_) => {
            // Anchors are invisible
        }
        InlineKind::Callout(callout) => {
            // Render callout reference as (N)
            write!(w, "({})", callout.number())?;
        }
        _ => {
            return Err(IoError::new(
                ErrorKind::Unsupported,
                format!("Unsupported inline node in buffer: {node:?}"),
            )
            .into());
        }
    }
    Ok(())
}

impl<'a, W: Write> crate::TerminalVisitor<'a, '_, W> {
    /// Internal implementation for visiting inline nodes
    pub(crate) fn render_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        let processor = self.processor;
        match node.kind() {
            InlineKind::Plain(p) => {
                write!(self.writer_mut(), "{}", p.text())?;
            }
            InlineKind::CharacterReference(reference) => write!(
                self.writer_mut(),
                "{}",
                acdc_converters_core::character_reference_text(reference.kind())
            )?,
            InlineKind::Italic(_)
            | InlineKind::Bold(_)
            | InlineKind::Highlight(_)
            | InlineKind::Monospace(_) => {
                self.render_formatted_inline_node(traversal, node, processor)?;
            }
            InlineKind::Superscript(_) | InlineKind::Subscript(_) => {
                self.render_script_inline_node(traversal, node, processor)?;
            }
            InlineKind::CrossReference(xref) => {
                self.render_cross_reference(traversal, xref, processor)?;
            }
            InlineKind::Button(button) => {
                let w = self.writer_mut();
                render_button(button, w, processor, traversal)?;
            }
            InlineKind::IndexTerm(term) => {
                register_index_term(term, processor, traversal)?;
                if term.is_visible() {
                    self.write_content(traversal, term.term())?;
                }
            }
            InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::Footnote(_)
            | InlineKind::Pass(_)
            | InlineKind::Image(_)
            | InlineKind::Icon(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Menu(_)
            | InlineKind::Stem(_) => {
                render_inline_macro_to_writer(node, &mut self.writer, processor, traversal)?;
            }
            InlineKind::Anchor(_) => {
                // Anchors are invisible in terminal output
            }
            InlineKind::Raw(r) => {
                write_raw_text(&mut self.writer, r, self.text_boundaries)?;
            }
            InlineKind::Verbatim(v) => {
                let w = self.writer_mut();
                write!(w, "{}", v.text())?;
            }
            InlineKind::CurvedQuotation(c) => {
                let w = self.writer_mut();
                write!(w, "\u{201C}")?;
                self.write_content(traversal, c.content())?;
                let w = self.writer_mut();
                write!(w, "\u{201D}")?;
            }
            InlineKind::CurvedApostrophe(c) => {
                let w = self.writer_mut();
                write!(w, "\u{2018}")?;
                self.write_content(traversal, c.content())?;
                let w = self.writer_mut();
                write!(w, "\u{2019}")?;
            }
            InlineKind::Symbol(symbol) => write!(self.writer_mut(), "{}", symbol.text())?,
            InlineKind::StandaloneCurvedApostrophe(_) => {
                let w = self.writer_mut();
                write!(w, "\u{2019}")?;
            }
            InlineKind::LineBreak(_) => {
                let w = self.writer_mut();
                writeln!(w)?;
            }
            InlineKind::Callout(callout) => {
                // Render callout reference as bold (N)
                let w = self.writer_mut();
                w.queue(crossterm::style::SetAttribute(
                    crossterm::style::Attribute::Bold,
                ))?;
                write!(w, "({})", callout.number())?;
                w.queue(crossterm::style::SetAttribute(
                    crossterm::style::Attribute::NormalIntensity,
                ))?;
            }
            _ => {
                return Err(IoError::new(
                    ErrorKind::Unsupported,
                    format!("Unsupported inline node in terminal: {node:?}"),
                )
                .into());
            }
        }
        Ok(())
    }

    fn render_cross_reference(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        xref: CrossReferenceRef<'_>,
        processor: &Processor<'a>,
    ) -> Result<(), Error> {
        if !xref.text().is_empty() {
            return self.write_link_styled(processor, |visitor| {
                visitor.write_content(traversal, xref.text())
            });
        }

        match resolve_reference(
            processor
                .references
                .get(xref.target())
                .map(ReferenceRef::from),
            xref,
            &processor.xref_guard,
        ) {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => self
                .write_link_styled(processor, |visitor| {
                    visitor.write_content(traversal, inlines)
                }),
            XrefDisplay::ShortCaption(prefix) => self.write_link_styled(processor, |visitor| {
                write!(visitor.writer_mut(), "{prefix}")?;
                Ok(())
            }),
            XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                self.write_link_styled(processor, |visitor| {
                    write!(visitor.writer_mut(), "{prefix}, “")?;
                    visitor.write_content(traversal, inlines)?;
                    write!(visitor.writer_mut(), "”")?;
                    Ok(())
                })
            }
            XrefDisplay::Emphasized(prefix, inlines, _scope) => {
                self.write_link_styled(processor, |visitor| {
                    if let Some(prefix) = prefix {
                        write!(visitor.writer_mut(), "{prefix}, ")?;
                    }
                    visitor
                        .writer_mut()
                        .queue(SetAttribute(Attribute::Italic))?;
                    visitor.write_content(traversal, inlines)?;
                    visitor
                        .writer_mut()
                        .queue(SetAttribute(Attribute::NoItalic))?;
                    Ok(())
                })
            }
            XrefDisplay::Fallback(text) | XrefDisplay::Unresolved(text) => {
                self.write_link_styled(processor, |visitor| {
                    write!(visitor.writer_mut(), "{text}")?;
                    Ok(())
                })
            }
            XrefDisplay::External(target) => self.write_link_styled(processor, |visitor| {
                write!(visitor.writer_mut(), "[{target}]")?;
                Ok(())
            }),
            // Already inside a reference's styling: plain text, so the outer
            // reference keeps its color and underline.
            XrefDisplay::Nested(text) => {
                write!(self.writer_mut(), "{text}")?;
                Ok(())
            }
        }
    }

    /// Write reference text in the link color, underlined.
    fn write_link_styled(
        &mut self,
        processor: &Processor<'a>,
        text: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.push_colors(Some(processor.appearance.colors.link), None)?;
        self.writer_mut()
            .queue(SetAttribute(Attribute::Underlined))?;
        let result = text(self);
        self.writer_mut()
            .queue(SetAttribute(Attribute::NoUnderline))?;
        self.pop_colors()?;
        result
    }

    /// Render bold, italic, highlight, or monospace inline nodes with crossterm styling.
    fn render_formatted_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
        processor: &crate::Processor<'a>,
    ) -> Result<(), Error> {
        match node.kind() {
            InlineKind::Italic(i) => {
                let w = self.writer_mut();
                w.queue(SetAttribute(Attribute::Italic))?;
                self.write_content(traversal, i.content())?;
                let w = self.writer_mut();
                w.queue(SetAttribute(Attribute::NoItalic))?;
            }
            InlineKind::Bold(b) => {
                let w = self.writer_mut();
                w.queue(SetAttribute(Attribute::Bold))?;
                self.write_content(traversal, b.content())?;
                let w = self.writer_mut();
                w.queue(SetAttribute(Attribute::NormalIntensity))?;
            }
            InlineKind::Highlight(h) => {
                if h.role() == Some("underline") {
                    let w = self.writer_mut();
                    w.queue(SetAttribute(Attribute::Underlined))?;
                    self.write_content(traversal, h.content())?;
                    let w = self.writer_mut();
                    w.queue(SetAttribute(Attribute::NoUnderline))?;
                } else if h.role() == Some("line-through") {
                    let w = self.writer_mut();
                    w.queue(SetAttribute(Attribute::CrossedOut))?;
                    self.write_content(traversal, h.content())?;
                    let w = self.writer_mut();
                    w.queue(SetAttribute(Attribute::NotCrossedOut))?;
                } else {
                    self.push_colors(Some(Color::Black), Some(Color::Yellow))?;
                    self.write_content(traversal, h.content())?;
                    self.pop_colors()?;
                }
            }
            InlineKind::Monospace(m) => {
                self.push_colors(Some(processor.appearance.colors.inline_monospace), None)?;
                self.write_content(traversal, m.content())?;
                self.pop_colors()?;
            }
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::CharacterReference(_)
            | InlineKind::Symbol(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::Footnote(_)
            | InlineKind::Button(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Image(_)
            | InlineKind::Icon(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Menu(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | InlineKind::Callout(_)
            | _ => {}
        }
        Ok(())
    }

    /// Render superscript or subscript inline nodes.
    fn render_script_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
        processor: &crate::Processor<'a>,
    ) -> Result<(), Error> {
        match node.kind() {
            InlineKind::Superscript(s) => {
                let text =
                    render_inline_nodes_to_string(s.content().inlines(), processor, traversal)?;
                let w = self.writer_mut();
                if let Some(converted) = try_to_unicode_superscript(&text) {
                    write!(w, "{converted}")?;
                } else {
                    w.queue(SetAttribute(Attribute::Dim))?;
                    write!(w, "{text}")?;
                    w.queue(SetAttribute(Attribute::NormalIntensity))?;
                }
            }
            InlineKind::Subscript(s) => {
                let text =
                    render_inline_nodes_to_string(s.content().inlines(), processor, traversal)?;
                let w = self.writer_mut();
                if let Some(converted) = try_to_unicode_subscript(&text) {
                    write!(w, "{converted}")?;
                } else {
                    w.queue(SetAttribute(Attribute::Dim))?;
                    write!(w, "{text}")?;
                    w.queue(SetAttribute(Attribute::NormalIntensity))?;
                }
            }
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::CharacterReference(_)
            | InlineKind::Symbol(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::Footnote(_)
            | InlineKind::Button(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Image(_)
            | InlineKind::Icon(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Menu(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | InlineKind::Callout(_)
            | _ => {}
        }
        Ok(())
    }
}

pub(crate) fn maybe_render_osc8_link<W: Write + ?Sized>(
    target: &str,
    text: &str,
    w: &mut W,
    processor: &Processor<'_>,
) -> Result<(), Error> {
    if processor.appearance.capabilities.osc8_links {
        w.queue(Print(
            format!("\x1B]8;;{target}\x1B\\{text}\x1B]8;;\x1B\\")
                .with(processor.appearance.colors.link),
        ))?;
    } else {
        // Non-OSC8: show "text (url)" with text styled and URL dim
        w.queue(PrintStyledContent(
            text.with(processor.appearance.colors.link),
        ))?;
        if text != target {
            w.queue(PrintStyledContent(format!(" ({target})").dim()))?;
        }
    }
    Ok(())
}

fn render_link_to_writer<'a, W: Write + ?Sized>(
    l: LinkRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    let target = l.target().to_string();
    if l.text().inlines().any(is_self_linked_image) {
        for node in l.text().inlines() {
            if is_self_linked_image(node) {
                render_inline_node_to_writer(node, w, processor, traversal)?;
            } else {
                let text =
                    render_inline_nodes_to_string(std::iter::once(node), processor, traversal)?;
                maybe_render_osc8_link(&target, &text, w, processor)?;
            }
        }
    } else {
        let text = if l.text().is_empty() {
            link_fallback(&target, l.hides_uri_scheme()).to_string()
        } else {
            render_inline_nodes_to_string(l.text().inlines(), processor, traversal)?
        };
        maybe_render_osc8_link(&target, &text, w, processor)?;
    }
    Ok(())
}

fn render_inline_macro_to_writer<'a, W: Write + ?Sized>(
    node: InlineRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    match node.kind() {
        InlineKind::Link(link) => render_link_to_writer(link, w, processor, traversal)?,
        InlineKind::Url(u) => {
            let target = u.target().to_string();
            let text = if u.text().is_empty() {
                link_fallback(&target, u.hides_uri_scheme()).to_string()
            } else {
                render_inline_nodes_to_string(u.text().inlines(), processor, traversal)?
            };
            maybe_render_osc8_link(&target, &text, w, processor)?;
        }
        InlineKind::Mailto(m) => {
            let target = m.target().to_string();
            let text = if m.text().is_empty() {
                mailto_fallback(&target).to_string()
            } else {
                render_inline_nodes_to_string(m.text().inlines(), processor, traversal)?
            };
            maybe_render_osc8_link(&mailto_target(m), &text, w, processor)?;
        }
        InlineKind::Autolink(a) => {
            let target = a.target().to_string();
            let (text, bracketed) =
                autolink_fallback(&target, a.is_bracketed(), a.hides_uri_scheme());
            let text = if bracketed {
                format!("<{text}>")
            } else {
                text.to_string()
            };
            maybe_render_osc8_link(&target, &text, w, processor)?;
        }
        InlineKind::Footnote(footnote) => {
            // Render footnote as superscript number in terminal
            // For terminal output, we'll show [n] format since true superscript is limited
            w.queue(PrintStyledContent(
                format!("[{}]", footnote.number()).cyan().bold(),
            ))?;
        }
        InlineKind::Button(b) => render_button(b, w, processor, traversal)?,
        InlineKind::CrossReference(xref) => {
            render_cross_reference_to_writer(xref, w, processor, traversal)?;
        }
        InlineKind::Pass(p) => {
            // Pass content through as-is
            if let Some(text) = p.text() {
                write!(w, "{text}")?;
            }
        }
        InlineKind::Image(img) => {
            render_inline_image(img, w, processor, traversal)?;
        }
        InlineKind::Icon(icon) => render_icon_to_writer(icon, w, processor, traversal)?,
        InlineKind::Keyboard(kbd) => render_keyboard_to_writer(kbd, w, processor, traversal)?,
        InlineKind::Menu(menu) => {
            // Show menu path
            write!(
                w,
                "{}",
                render_content_with_styles_to_owned(menu.target(), processor, traversal)?
            )?;
            for item in menu.items() {
                write!(
                    w,
                    " > {}",
                    render_content_with_styles_to_owned(item, processor, traversal)?
                )?;
            }
        }
        InlineKind::Stem(stem) => {
            // Show stem content as-is (terminal can't render math)
            write!(w, "[{}]", stem.text())?;
        }
        InlineKind::IndexTerm(term) => {
            render_index_term_to_writer(term, w, processor, traversal)?;
        }
        InlineKind::Plain(_)
        | InlineKind::Raw(_)
        | InlineKind::CharacterReference(_)
        | InlineKind::Symbol(_)
        | InlineKind::Verbatim(_)
        | InlineKind::Italic(_)
        | InlineKind::Bold(_)
        | InlineKind::Highlight(_)
        | InlineKind::Monospace(_)
        | InlineKind::Superscript(_)
        | InlineKind::Subscript(_)
        | InlineKind::CurvedQuotation(_)
        | InlineKind::CurvedApostrophe(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Anchor(_)
        | InlineKind::Callout(_)
        | _ => {
            return Err(unsupported_inline_macro(node));
        }
    }
    Ok(())
}

fn render_icon_to_writer<'a, W: Write + ?Sized>(
    icon: IconRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    let alt = match icon.alt() {
        Some(content) => render_content_with_styles_to_owned(content, processor, traversal)?,
        None => acdc_converters_core::icon::alt_from_content(icon.target(), None).into_owned(),
    };
    let label = format!("[Icon: {alt}]");
    if let Some(target) = attribute_text(icon.attributes(), "link") {
        maybe_render_osc8_link(target, &label, w, processor)?;
    } else {
        write!(w, "{label}")?;
    }
    Ok(())
}

fn render_keyboard_to_writer<'a, W: Write + ?Sized>(
    keyboard: KeyboardRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    write!(w, "[")?;
    for (index, key) in keyboard.keys().enumerate() {
        if index > 0 {
            write!(w, "+")?;
        }
        write!(
            w,
            "{}",
            render_content_with_styles_to_owned(key, processor, traversal)?
        )?;
    }
    write!(w, "]")?;
    Ok(())
}

fn is_self_linked_image(node: InlineRef<'_>) -> bool {
    matches!(node.kind(), InlineKind::Image(image) if attribute_text(image.attributes(), "link").is_some())
}

fn render_inline_image<'a, W: Write + ?Sized>(
    image: InlineImageRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    let alt = match image.alt() {
        Some(content) => render_content_with_styles_to_owned(content, processor, traversal)?,
        None => acdc_converters_core::media::image_alt_from_content(image.source(), image.alt()),
    };
    let text = format!("[Image: {alt}]");
    if let Some(target) = attribute_text(image.attributes(), "link") {
        maybe_render_osc8_link(target, &text, w, processor)
    } else {
        write!(w, "{text}")?;
        Ok(())
    }
}

pub(crate) fn register_index_term<'a>(
    term: IndexTermRef<'_>,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    let term = term.catalog();
    let mut render_label = |inlines: ContentRef<'_>| -> Result<IndexTermLabel, Error> {
        Ok(IndexTermLabel {
            plain: InlineTextTransform::default()
                .decode_char_refs(true)
                .content_to_string(inlines),
            rendered: render_content_with_styles_to_owned(inlines, processor, traversal)?,
        })
    };
    processor.add_index_entry(IndexTermEntry {
        primary: render_label(term.term())?,
        secondary: term.secondary().map(&mut render_label).transpose()?,
        tertiary: term.tertiary().map(&mut render_label).transpose()?,
        relationship: match term.relationship() {
            Some(IndexTermRelationshipRef::See { target }) => {
                IndexCatalogRelationship::See(render_label(target)?)
            }
            Some(IndexTermRelationshipRef::SeeAlso { targets }) => {
                IndexCatalogRelationship::SeeAlso(
                    targets.iter().map(render_label).collect::<Result<_, _>>()?,
                )
            }
            None | Some(_) => IndexCatalogRelationship::None,
        },
    });

    Ok(())
}

fn render_index_term_to_writer<'a, W: Write + ?Sized>(
    term: IndexTermRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    register_index_term(term, processor, traversal)?;
    if term.is_visible() {
        write!(
            w,
            "{}",
            render_content_with_styles_to_owned(term.term(), processor, traversal)?
        )?;
    }
    Ok(())
}

fn render_button<'a, W: Write + ?Sized>(
    button: ButtonRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    let label = render_content_with_styles_to_owned(button.label(), processor, traversal)?;
    w.queue(PrintStyledContent(format!("[{label}]").white().bold()))?;
    Ok(())
}

/// Render a cross-reference into a buffer.
///
/// Used inside buffered inline spans, such as superscripts and subscripts.
fn render_cross_reference_to_writer<'a, W: Write + ?Sized>(
    xref: CrossReferenceRef<'_>,
    w: &mut W,
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
) -> Result<(), Error> {
    let text = if xref.text().is_empty() {
        match resolve_reference(
            processor
                .references
                .get(xref.target())
                .map(ReferenceRef::from),
            xref,
            &processor.xref_guard,
        ) {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => {
                render_inline_nodes_to_owned(inlines.inlines(), processor, traversal)?
            }
            XrefDisplay::ShortCaption(prefix) => prefix,
            XrefDisplay::FullCaption(prefix, inlines, _scope) => format!(
                "{prefix}, “{}”",
                render_inline_nodes_to_owned(inlines.inlines(), processor, traversal)?
            ),
            XrefDisplay::Emphasized(prefix, inlines, _scope) => {
                let prefix = prefix.map_or_else(String::new, |mut prefix| {
                    prefix.push_str(", ");
                    prefix
                });
                let title = render_content_with_styles_to_owned(inlines, processor, traversal)?;
                format!(
                    "{prefix}{}{title}{}",
                    SetAttribute(Attribute::Italic),
                    SetAttribute(Attribute::NoItalic),
                )
            }
            XrefDisplay::Fallback(text)
            | XrefDisplay::Unresolved(text)
            | XrefDisplay::Nested(text) => text,
            XrefDisplay::External(target) => format!("[{target}]"),
        }
    } else {
        InlineTextTransform::default()
            .decode_char_refs(true)
            .content_to_string(xref.text())
    };
    w.queue(PrintStyledContent(
        text.with(processor.appearance.colors.link).underlined(),
    ))?;
    Ok(())
}

fn unsupported_inline_macro(node: InlineRef<'_>) -> Error {
    IoError::new(
        ErrorKind::Unsupported,
        format!("Unsupported inline macro in terminal: {node:?}"),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TerminalVisitor, create_test_processor};

    use acdc_parser::{
        Anchor, Bold, CrossReference, CurvedApostrophe, CurvedQuotation, Form, Highlight,
        InlineImage, InlineMacro, InlineNode, Italic, Keyboard, LineBreak, Link, Location,
        Monospace, Paragraph, Plain, Source, StandaloneCurvedApostrophe, Subscript, Superscript,
    };

    /// Create simple plain text inline node for testing
    fn create_plain_text(content: &str) -> InlineNode<'_> {
        InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })
    }

    /// Helper to render a paragraph with inline nodes and return the output
    fn render_paragraph(inlines: Vec<InlineNode>) -> Result<String, Error> {
        let paragraph = Paragraph::new(inlines, Location::default());

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let attribute_header =
            acdc_converters_core::Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(buffer, &processor, diagnostics.reborrow());
        visitor.render_paragraph(&mut traversal, (&paragraph).into())?;
        let output = visitor.into_writer();

        Ok(String::from_utf8_lossy(&output).to_string())
    }

    #[test]
    fn test_plain_text() -> Result<(), Error> {
        let output = render_paragraph(vec![create_plain_text("Hello, world!")])?;
        assert!(
            output.contains("Hello, world!"),
            "Should contain plain text"
        );
        Ok(())
    }

    #[test]
    fn test_bold_text() -> Result<(), Error> {
        let bold = InlineNode::BoldText(Bold {
            content: vec![create_plain_text("bold text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![bold])?;
        // Bold text should contain ANSI bold escape codes
        assert!(
            output.contains("bold text"),
            "Should contain bold text content"
        );
        assert!(output.contains("\x1b[1m"), "Should contain ANSI bold code");
        Ok(())
    }

    #[test]
    fn test_italic_text() -> Result<(), Error> {
        let italic = InlineNode::ItalicText(Italic {
            content: vec![create_plain_text("italic text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![italic])?;
        // Italic text should contain ANSI italic escape codes
        assert!(
            output.contains("italic text"),
            "Should contain italic text content"
        );
        assert!(
            output.contains("\x1b[3m"),
            "Should contain ANSI italic code"
        );
        Ok(())
    }

    #[test]
    fn test_monospace_text() -> Result<(), Error> {
        let monospace = InlineNode::MonospaceText(Monospace {
            content: vec![create_plain_text("monospace text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![monospace])?;
        assert!(
            output.contains("monospace text"),
            "Should contain monospace text content"
        );
        // Monospace uses black text on gray background (codes 30 and 100)
        assert!(
            output.contains("\x1b["),
            "Should contain ANSI escape codes for styling"
        );
        Ok(())
    }

    #[test]
    fn test_mixed_formatting() -> Result<(), Error> {
        let output = render_paragraph(vec![
            create_plain_text("Normal "),
            InlineNode::BoldText(Bold {
                content: vec![create_plain_text("bold")],
                role: None,
                id: None,
                form: Form::Constrained,
                location: Location::default(),
            }),
            create_plain_text(" and "),
            InlineNode::ItalicText(Italic {
                content: vec![create_plain_text("italic")],
                role: None,
                id: None,
                form: Form::Constrained,
                location: Location::default(),
            }),
        ])?;

        assert!(output.contains("Normal"), "Should contain normal text");
        assert!(output.contains("bold"), "Should contain bold text");
        assert!(output.contains("italic"), "Should contain italic text");
        Ok(())
    }

    #[test]
    fn test_highlight_text() -> Result<(), Error> {
        let highlight = InlineNode::HighlightText(Highlight {
            content: vec![create_plain_text("highlighted")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![highlight])?;
        assert!(
            output.contains("highlighted"),
            "Should contain highlighted text"
        );
        // Highlight uses yellow background (code 43 or 103)
        assert!(output.contains("\x1b["), "Should contain ANSI escape codes");
        Ok(())
    }

    #[test]
    fn test_superscript_text() -> Result<(), Error> {
        let superscript = InlineNode::SuperscriptText(Superscript {
            content: vec![create_plain_text("2")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("x"), superscript])?;

        assert!(output.contains('x'), "Should contain base text");
        assert!(
            output.contains('\u{00B2}'),
            "Should render '2' as Unicode superscript '²'"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_text() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("2")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("a"), subscript])?;

        assert!(output.contains('a'), "Should contain base text");
        assert!(
            output.contains('\u{2082}'),
            "Should render '2' as Unicode subscript '₂'"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_fallback_for_letters() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("n")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("a"), subscript])?;
        assert!(
            output.contains("\x1b[2m"),
            "Letters should fall back to dim styling, got: {output:?}"
        );
        assert!(
            output.contains('n'),
            "Should contain the original text, got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_superscript_fallback_for_letters() -> Result<(), Error> {
        let superscript = InlineNode::SuperscriptText(Superscript {
            content: vec![create_plain_text("abc")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![superscript])?;
        assert!(
            output.contains("\x1b[2m"),
            "Letters should fall back to dim styling, got: {output:?}"
        );
        assert!(
            output.contains("abc"),
            "Should contain the original text, got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_curved_quotation_text() -> Result<(), Error> {
        let quoted = InlineNode::CurvedQuotationText(CurvedQuotation {
            content: vec![create_plain_text("quoted text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![quoted])?;
        assert!(
            output.contains("\u{201C}"),
            "Should contain opening curly quote"
        );
        assert!(
            output.contains("\u{201D}"),
            "Should contain closing curly quote"
        );
        assert!(output.contains("quoted text"), "Should contain quoted text");
        Ok(())
    }

    #[test]
    fn test_curved_apostrophe_text() -> Result<(), Error> {
        let apostrophe = InlineNode::CurvedApostropheText(CurvedApostrophe {
            content: vec![create_plain_text("text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![apostrophe])?;
        assert!(
            output.contains("\u{2018}"),
            "Should contain opening curly apostrophe"
        );
        assert!(
            output.contains("\u{2019}"),
            "Should contain closing curly apostrophe"
        );
        Ok(())
    }

    #[test]
    fn test_standalone_curved_apostrophe() -> Result<(), Error> {
        let apostrophe = InlineNode::StandaloneCurvedApostrophe(StandaloneCurvedApostrophe {
            location: Location::default(),
        });

        let output = render_paragraph(vec![apostrophe])?;
        assert!(
            output.contains("\u{2019}"),
            "Should contain curly apostrophe"
        );
        Ok(())
    }

    #[test]
    fn test_link_macro() -> Result<(), Error> {
        let link = InlineNode::Macro(InlineMacro::Link(Box::new(Link::new(
            Source::Name("https://example.com"),
            Location::default(),
        ))));

        let output = render_paragraph(vec![link])?;
        assert!(
            output.contains("https://example.com"),
            "Should render link URL"
        );
        Ok(())
    }

    #[test]
    fn test_image_macro_placeholder() -> Result<(), Error> {
        let image = InlineNode::Macro(InlineMacro::Image(Box::new(InlineImage::new(
            Source::Name("logo.png"),
            Location::default(),
        ))));

        let output = render_paragraph(vec![image])?;
        assert!(
            output.contains("[Image: logo]"),
            "Should render image placeholder"
        );
        Ok(())
    }

    #[test]
    fn test_keyboard_macro() -> Result<(), Error> {
        let kbd = InlineNode::Macro(InlineMacro::Keyboard(Keyboard::new(
            ["Ctrl", "C"]
                .into_iter()
                .map(|content| {
                    acdc_parser::ContentAttribute::new(vec![InlineNode::PlainText(Plain {
                        content,
                        location: Location::default(),
                        escaped: false,
                    })])
                })
                .collect(),
            Location::default(),
        )));

        let output = render_paragraph(vec![kbd])?;
        assert!(
            output.contains("[Ctrl+C]"),
            "Should render keyboard shortcut"
        );
        Ok(())
    }

    #[test]
    fn test_cross_reference_with_text() -> Result<(), Error> {
        let xref = InlineNode::Macro(InlineMacro::CrossReference(Box::new(
            CrossReference::new("section-id", Location::default())
                .with_text(vec![create_plain_text("See Section 1")]),
        )));

        let output = render_paragraph(vec![xref])?;
        assert!(
            output.contains("See Section 1"),
            "Should render xref custom text"
        );
        assert!(
            output.contains("\x1b["),
            "Should contain ANSI codes for styling"
        );
        Ok(())
    }

    #[test]
    fn test_cross_reference_without_text() -> Result<(), Error> {
        let xref = InlineNode::Macro(InlineMacro::CrossReference(Box::new(CrossReference::new(
            "section-id",
            Location::default(),
        ))));

        let output = render_paragraph(vec![xref])?;
        assert!(
            output.contains("[section-id]"),
            "Should render xref with target in brackets"
        );
        Ok(())
    }

    #[test]
    fn test_line_break() -> Result<(), Error> {
        let output = render_paragraph(vec![
            create_plain_text("First line"),
            InlineNode::LineBreak(LineBreak {
                location: Location::default(),
            }),
            create_plain_text("Second line"),
        ])?;

        // Line break should create a newline
        assert!(output.contains("First line"), "Should contain first line");
        assert!(output.contains("Second line"), "Should contain second line");
        // Should have newline between them
        assert!(output.lines().count() >= 2, "Should have multiple lines");
        Ok(())
    }

    #[test]
    fn test_inline_anchor_invisible() -> Result<(), Error> {
        let output = render_paragraph(vec![
            create_plain_text("Before"),
            InlineNode::InlineAnchor(Anchor::new("anchor-id", Location::default())),
            create_plain_text("After"),
        ])?;

        // Anchor should be invisible
        assert!(
            output.contains("Before"),
            "Should contain text before anchor"
        );
        assert!(output.contains("After"), "Should contain text after anchor");
        assert!(
            !output.contains("anchor-id"),
            "Anchor ID should not be visible"
        );
        Ok(())
    }

    #[test]
    fn test_highlight_text_with_underline_role() -> Result<(), Error> {
        let highlight = InlineNode::HighlightText(Highlight {
            content: vec![create_plain_text("underlined text")],
            role: Some("underline"),
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![highlight])?;
        assert!(
            output.contains("underlined text"),
            "Should contain the text"
        );
        // Should use ANSI underline attribute (\x1b[4m), NOT highlight colors
        assert!(
            output.contains("\x1b[4m"),
            "Should contain ANSI underline code, got: {output:?}"
        );
        // Should NOT contain yellow background (highlight styling)
        assert!(
            !output.contains("\x1b[48;5;11m"),
            "Should NOT use highlight background color for underline role"
        );
        Ok(())
    }

    #[test]
    fn test_superscript_renders_unicode() -> Result<(), Error> {
        let superscript = InlineNode::SuperscriptText(Superscript {
            content: vec![create_plain_text("2")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("x"), superscript])?;
        assert!(output.contains('x'), "Should contain base text");
        assert!(
            output.contains('\u{00B2}'),
            "Should render '2' as Unicode superscript '²', got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_renders_unicode() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("2")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("H"), subscript])?;
        assert!(output.contains('H'), "Should contain base text");
        assert!(
            output.contains('\u{2082}'),
            "Should render '2' as Unicode subscript '₂', got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_superscript_fallback_for_unsupported_chars() -> Result<(), Error> {
        // Characters without Unicode superscript equivalents should fall back
        let superscript = InlineNode::SuperscriptText(Superscript {
            content: vec![create_plain_text("@")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![superscript])?;
        assert!(
            output.contains("\x1b[2m"),
            "Should fall back to dim styling for unsupported chars, got: {output:?}"
        );
        assert!(
            output.contains('@'),
            "Should contain the original text, got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_fallback_for_unsupported_chars() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("@")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![subscript])?;
        assert!(
            output.contains("\x1b[2m"),
            "Should fall back to dim styling for unsupported chars, got: {output:?}"
        );
        assert!(
            output.contains('@'),
            "Should contain the original text, got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_nested_formatting() -> Result<(), Error> {
        // Test bold text containing italic text
        let nested = InlineNode::BoldText(Bold {
            content: vec![InlineNode::ItalicText(Italic {
                content: vec![create_plain_text("bold italic")],
                role: None,
                id: None,
                form: Form::Constrained,
                location: Location::default(),
            })],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![nested])?;
        assert!(
            output.contains("bold italic"),
            "Should contain nested formatted text"
        );
        // Should have escape codes for both bold and italic
        assert!(output.contains("\x1b["), "Should contain ANSI escape codes");
        Ok(())
    }
}
