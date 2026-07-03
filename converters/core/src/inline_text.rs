//! Plain-text extraction from inline nodes.

use std::fmt::{self, Write};

use acdc_parser::{InlineMacro, InlineNode};

/// Plain-text extraction policy for inline nodes.
#[derive(Clone, Copy, Debug)]
pub struct InlineTextTransform<'a> {
    line_break: &'a str,
}

impl Default for InlineTextTransform<'_> {
    fn default() -> Self {
        Self { line_break: " " }
    }
}

impl<'a> InlineTextTransform<'a> {
    /// Render hard line breaks as `line_break`.
    #[must_use]
    pub fn line_break(mut self, line_break: &'a str) -> Self {
        self.line_break = line_break;
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
        #[allow(clippy::match_same_arms, clippy::wildcard_enum_match_arm)]
        match node {
            InlineNode::PlainText(text) => w.write_str(text.content),
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
            InlineNode::StandaloneCurvedApostrophe(_) => w.write_char('\''),
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
        #[allow(clippy::wildcard_enum_match_arm)]
        match macro_node {
            InlineMacro::Link(link) => {
                if link.text.is_empty() {
                    write!(w, "{}", link.target)
                } else {
                    self.write(w, &link.text)
                }
            }
            InlineMacro::Url(url) => {
                if url.text.is_empty() {
                    write!(w, "{}", url.target)
                } else {
                    self.write(w, &url.text)
                }
            }
            InlineMacro::Mailto(mailto) => {
                if mailto.text.is_empty() {
                    write!(w, "{}", mailto.target)
                } else {
                    self.write(w, &mailto.text)
                }
            }
            InlineMacro::Autolink(autolink) => write!(w, "{}", autolink.url),
            InlineMacro::CrossReference(xref) => {
                if xref.text.is_empty() {
                    write!(w, "{}", xref.target)
                } else {
                    self.write(w, &xref.text)
                }
            }
            InlineMacro::IndexTerm(index_term) if index_term.is_visible() => {
                w.write_str(index_term.term())
            }
            InlineMacro::Pass(pass) => w.write_str(pass.text.unwrap_or_default()),
            InlineMacro::Keyboard(keyboard) => write!(w, "{}", keyboard.keys.join("+")),
            InlineMacro::Button(button) => w.write_str(button.label),
            InlineMacro::Menu(menu) => write!(w, "{}", menu.items.join(" > ")),
            InlineMacro::Image(_)
            | InlineMacro::Footnote(_)
            | InlineMacro::Stem(_)
            | InlineMacro::Icon(_)
            | InlineMacro::IndexTerm(_)
            | _ => Ok(()),
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
}
