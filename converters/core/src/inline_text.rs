//! Plain-text extraction from inline nodes.
//!
//! Headings, captions, and other places that cannot carry markup still have to
//! carry the *text* of what they hold. Every inline node contributes its text
//! here: a link contributes its link text, and a cross-reference contributes the
//! reference text of its target when a catalog is supplied.

use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::{self, Write},
};

use acdc_parser::{InlineMacro, InlineNode, Reference};

use crate::{decode_numeric_char_refs, xref::reference_text};

/// Plain-text extraction policy for inline nodes.
#[derive(Clone, Copy, Debug)]
pub struct InlineTextTransform<'t, 'd> {
    line_break: &'t str,
    decode_char_refs: bool,
    references: Option<&'t HashMap<&'d str, Reference<'d>>>,
    /// Set while extracting a target's reference text, so a cross-reference
    /// inside it falls back to `[id]` instead of recursing. See
    /// [`crate::xref::XrefGuard`], which does the same for rendered output.
    resolving_xref: bool,
}

impl Default for InlineTextTransform<'_, '_> {
    fn default() -> Self {
        Self {
            line_break: " ",
            decode_char_refs: false,
            references: None,
            resolving_xref: false,
        }
    }
}

impl<'t, 'd> InlineTextTransform<'t, 'd> {
    /// Render hard line breaks as `line_break`.
    #[must_use]
    pub fn line_break(mut self, line_break: &'t str) -> Self {
        self.line_break = line_break;
        self
    }

    /// Decode numeric character references (`&#39;`) in raw text.
    ///
    /// Non-HTML backends want the character; HTML keeps the reference.
    #[must_use]
    pub fn decode_char_refs(mut self, decode: bool) -> Self {
        self.decode_char_refs = decode;
        self
    }

    /// Resolve a cross-reference with no text of its own through this catalog,
    /// so it contributes its target's reference text.
    ///
    /// Without a catalog such a reference contributes its target id, which is
    /// all an inter-document reference has.
    #[must_use]
    pub fn references(mut self, references: &'t HashMap<&'d str, Reference<'d>>) -> Self {
        self.references = Some(references);
        self
    }

    /// Write plain text extracted from `inlines` into `w`.
    ///
    /// # Errors
    ///
    /// Returns any error produced by the underlying writer.
    pub fn write<W: Write + ?Sized>(self, w: &mut W, inlines: &[InlineNode<'_>]) -> fmt::Result {
        for node in inlines {
            self.write_inline_node(w, node)?;
        }
        Ok(())
    }

    fn write_inline_node<W: Write + ?Sized>(self, w: &mut W, node: &InlineNode<'_>) -> fmt::Result {
        #[expect(
            clippy::match_same_arms,
            reason = "plain-text extraction intentionally ignores unknown non-exhaustive inline nodes"
        )]
        match node {
            InlineNode::PlainText(text) => w.write_str(text.content),
            InlineNode::RawText(text) if self.decode_char_refs => {
                w.write_str(&decode_numeric_char_refs(text.content))
            }
            InlineNode::RawText(text) => w.write_str(text.content),
            InlineNode::VerbatimText(text) => w.write_str(text.content),
            InlineNode::BoldText(bold) => self.write(w, &bold.content),
            InlineNode::ItalicText(italic) => self.write(w, &italic.content),
            InlineNode::MonospaceText(mono) => self.write(w, &mono.content),
            InlineNode::HighlightText(highlight) => self.write(w, &highlight.content),
            InlineNode::SubscriptText(sub) => self.write(w, &sub.content),
            InlineNode::SuperscriptText(sup) => self.write(w, &sup.content),
            InlineNode::CurvedQuotationText(quote) => self.write(w, &quote.content),
            InlineNode::CurvedApostropheText(apos) => self.write(w, &apos.content),
            InlineNode::StandaloneCurvedApostrophe(_) => w.write_char('\u{2019}'),
            InlineNode::LineBreak(_) => w.write_str(self.line_break),
            InlineNode::InlineAnchor(_) => Ok(()),
            InlineNode::Macro(macro_node) => self.write_inline_macro(w, macro_node),
            InlineNode::CalloutRef(callout) => write!(w, "<{}>", callout.number),
            _ => Ok(()),
        }
    }

    fn write_inline_macro<W: Write + ?Sized>(
        self,
        w: &mut W,
        macro_node: &InlineMacro<'_>,
    ) -> fmt::Result {
        match macro_node {
            InlineMacro::Link(link) => self.write_link_text(w, &link.text, &link.target),
            InlineMacro::Url(url) => self.write_link_text(w, &url.text, &url.target),
            InlineMacro::Mailto(mailto) => self.write_link_text(w, &mailto.text, &mailto.target),
            InlineMacro::Autolink(autolink) => write!(w, "{}", autolink.url),
            InlineMacro::CrossReference(xref) => self.write_xref_text(w, xref),
            InlineMacro::IndexTerm(index_term) if index_term.is_visible() => {
                self.write(w, index_term.term())
            }
            InlineMacro::Pass(pass) => w.write_str(pass.text.unwrap_or_default()),
            InlineMacro::Keyboard(keyboard) => write!(w, "{}", keyboard.keys.join("+")),
            InlineMacro::Button(button) => w.write_str(button.label),
            InlineMacro::Menu(menu) => {
                w.write_str(menu.target)?;
                for item in &menu.items {
                    write!(w, " > {item}")?;
                }
                Ok(())
            }
            // These carry no prose, but they do carry an identifier or a marker
            // that a heading or caption would otherwise lose. Asciidoctor
            // brackets them, as acdc does when it renders them inline.
            InlineMacro::Image(image) => {
                w.write_char('[')?;
                if image.title.is_empty() {
                    write!(w, "{}", image.source)?;
                } else {
                    self.write(w, &image.title)?;
                }
                w.write_char(']')
            }
            InlineMacro::Icon(icon) => {
                write!(w, "[{}]", crate::icon::alt(&icon.target, &icon.attributes))
            }
            InlineMacro::Footnote(footnote) => write!(w, "[{}]", footnote.number),
            InlineMacro::Stem(stem) => w.write_str(stem.content),
            InlineMacro::IndexTerm(_) | _ => Ok(()),
        }
    }

    /// Write what a cross-reference reads as: its own text, else the target's
    /// reference text, else the `[id]` fallback asciidoctor uses.
    fn write_xref_text<W: Write + ?Sized>(
        self,
        w: &mut W,
        xref: &acdc_parser::CrossReference<'_>,
    ) -> fmt::Result {
        if !xref.text.is_empty() {
            return self.write(w, &xref.text);
        }
        let Some(references) = self.references else {
            return write!(w, "{}", xref.target);
        };
        if self.resolving_xref {
            return write!(w, "[{}]", xref.target);
        }
        match references.get(xref.target).and_then(reference_text) {
            Some(nodes) => Self {
                resolving_xref: true,
                ..self
            }
            .write(w, nodes),
            None => write!(w, "[{}]", stylized_id(xref.target)),
        }
    }

    fn write_link_text<W: Write + ?Sized>(
        self,
        w: &mut W,
        text: &[InlineNode<'_>],
        target: &impl fmt::Display,
    ) -> fmt::Result {
        if text.is_empty() {
            write!(w, "{target}")
        } else {
            self.write(w, text)
        }
    }

    /// Extract plain text from `inlines` as a `String`.
    #[must_use]
    pub fn to_string(self, inlines: &[InlineNode<'_>]) -> String {
        let mut s = String::new();
        // Writing into a `String` is infallible.
        let _ = self.write(&mut s, inlines);
        s
    }
}

/// The stylized form of a reference target, which is what a reference to an
/// unknown target reads as.
///
/// An inter-document target keeps its fragment but drops the file extension,
/// so `other.adoc#part` reads as `other#part`, matching asciidoctor.
fn stylized_id(target: &str) -> Cow<'_, str> {
    let (path, fragment) = match target.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (target, None),
    };
    let stem = path
        .rsplit_once('.')
        .filter(|(before, _)| !before.is_empty() && !before.ends_with('/'))
        .map_or(path, |(before, _)| before);
    match fragment {
        _ if stem.len() == path.len() => Cow::Borrowed(target),
        None => Cow::Borrowed(stem),
        Some(fragment) => Cow::Owned(format!("{stem}#{fragment}")),
    }
}

/// Extract plain text from `inlines` as a `String`.
#[must_use]
pub fn inlines_to_string(inlines: &[InlineNode<'_>]) -> String {
    InlineTextTransform::default().to_string(inlines)
}

#[cfg(test)]
mod tests {
    use acdc_parser::{InlineNode, LineBreak, Location, Plain};

    use super::{InlineTextTransform, inlines_to_string};

    fn plain(content: &str) -> InlineNode<'_> {
        InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })
    }

    #[test]
    fn inlines_to_string_collapses_line_break_to_space() {
        let inlines = vec![
            plain("first"),
            InlineNode::LineBreak(LineBreak {
                location: Location::default(),
            }),
            plain("second"),
        ];

        assert_eq!(inlines_to_string(&inlines), "first second");
    }

    #[test]
    fn transform_uses_requested_line_break() {
        let inlines = vec![
            plain("first"),
            InlineNode::LineBreak(LineBreak {
                location: Location::default(),
            }),
            plain("second"),
        ];

        assert_eq!(
            InlineTextTransform::default()
                .line_break("\n")
                .to_string(&inlines),
            "first\nsecond"
        );
    }

    /// Parse a document whose body paragraph holds the cross-references under
    /// test, and whose catalog holds `titled`, `labelled`, and `untitled`.
    fn document(body: &str) -> Result<acdc_parser::ParseResult, acdc_parser::Error> {
        let input = format!(
            "= Doc\n:experimental:\n\n{body}\n\n\
             [[titled]]\n.A *title*\n====\nbody\n====\n\n\
             Some [[labelled,A label]]text.\n\n\
             [[untitled]]\n====\nbody\n====\n\n\
             [[recursive]]\n.See <<recursive>> again\n====\nbody\n====\n"
        );
        acdc_parser::parse(&input, &acdc_parser::Options::default())
    }

    /// The inline nodes of the document's first body paragraph.
    fn body_inlines<'a>(doc: &'a acdc_parser::Document<'a>) -> &'a [InlineNode<'a>] {
        doc.blocks
            .iter()
            .find_map(|block| {
                if let acdc_parser::Block::Paragraph(paragraph) = block {
                    Some(paragraph.content.as_slice())
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_cross_reference_without_a_catalog_reads_as_its_target() -> Result<(), acdc_parser::Error> {
        // All an inter-document reference has is its target.
        let parsed = document("See <<titled>>.")?;
        let doc = parsed.document();
        assert_eq!(inlines_to_string(body_inlines(doc)), "See titled.");
        Ok(())
    }

    #[test]
    fn a_cross_reference_reads_as_its_target_reference_text() -> Result<(), acdc_parser::Error> {
        let parsed = document("See <<titled>>, <<labelled>>, <<untitled>>, and <<missing>>.")?;
        let doc = parsed.document();
        assert_eq!(
            InlineTextTransform::default()
                .references(&doc.references)
                .to_string(body_inlines(doc)),
            "See A title, A label, [untitled], and [missing]."
        );
        Ok(())
    }

    #[test]
    fn a_cross_reference_inside_reference_text_reads_as_its_id() -> Result<(), acdc_parser::Error> {
        let parsed = document("See <<recursive>>.")?;
        let doc = parsed.document();
        assert_eq!(
            InlineTextTransform::default()
                .references(&doc.references)
                .to_string(body_inlines(doc)),
            "See See [recursive] again."
        );
        Ok(())
    }

    #[test]
    fn a_link_reads_as_its_text_then_its_target() -> Result<(), acdc_parser::Error> {
        let parsed = document("See https://example.com[the site] and https://bare.example[].")?;
        let doc = parsed.document();
        assert_eq!(
            inlines_to_string(body_inlines(doc)),
            "See the site and https://bare.example."
        );
        Ok(())
    }

    #[test]
    fn a_menu_reads_as_its_target_then_its_items() -> Result<(), acdc_parser::Error> {
        let parsed = document("Choose menu:File[Open > Recent].")?;
        let doc = parsed.document();
        assert_eq!(
            inlines_to_string(body_inlines(doc)),
            "Choose File > Open > Recent."
        );
        Ok(())
    }

    #[test]
    fn character_references_decode_only_when_asked() -> Result<(), acdc_parser::Error> {
        let parsed = document("It{apos}s here.")?;
        let doc = parsed.document();
        let inlines = body_inlines(doc);
        assert_eq!(inlines_to_string(inlines), "It&#39;s here.");
        assert_eq!(
            InlineTextTransform::default()
                .decode_char_refs(true)
                .to_string(inlines),
            "It's here."
        );
        Ok(())
    }

    #[test]
    fn an_unknown_target_reads_as_its_stylized_id() {
        assert_eq!(super::stylized_id("plain-id"), "plain-id");
        assert_eq!(super::stylized_id("other.adoc"), "other");
        assert_eq!(super::stylized_id("other.adoc#part"), "other#part");
        assert_eq!(super::stylized_id("dir/other.adoc#part"), "dir/other#part");
        assert_eq!(super::stylized_id("no-extension#part"), "no-extension#part");
    }
}
