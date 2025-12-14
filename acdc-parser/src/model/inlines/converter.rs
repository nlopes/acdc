use crate::{InlineMacro, InlineNode};

// TODO(nlopes): this could instead be impl ToString for InlineNode?
//
// To do so, we'd need to change Vec<InlineNode> to a newtype struct.
/// Extract plain text from inline nodes, recursively handling formatted text.
///
/// This function recursively extracts text from all inline formatting nodes
/// (bold, italic, monospace, etc.), which is useful for generating IDs,
/// alt text, and other plain text representations of formatted content.
#[allow(clippy::must_use_candidate)]
pub fn inlines_to_string(inlines: &[InlineNode]) -> String {
    inlines
        .iter()
        .map(|node| match node {
            InlineNode::PlainText(text) => text.content.clone(),
            InlineNode::RawText(text) => text.content.clone(),
            InlineNode::VerbatimText(text) => text.content.clone(),
            InlineNode::BoldText(bold) => inlines_to_string(&bold.content),
            InlineNode::ItalicText(italic) => inlines_to_string(&italic.content),
            InlineNode::MonospaceText(mono) => inlines_to_string(&mono.content),
            InlineNode::HighlightText(highlight) => inlines_to_string(&highlight.content),
            InlineNode::SubscriptText(sub) => inlines_to_string(&sub.content),
            InlineNode::SuperscriptText(sup) => inlines_to_string(&sup.content),
            InlineNode::CurvedQuotationText(quote) => inlines_to_string(&quote.content),
            InlineNode::CurvedApostropheText(apos) => inlines_to_string(&apos.content),
            InlineNode::StandaloneCurvedApostrophe(_) => "'".to_string(),
            InlineNode::LineBreak(_) => " ".to_string(),
            InlineNode::InlineAnchor(_) => String::new(),
            InlineNode::Macro(macro_node) => match macro_node {
                InlineMacro::Link(link) => {
                    link.text.clone().unwrap_or_else(|| link.target.to_string())
                }
                InlineMacro::Url(url) => {
                    if url.text.is_empty() {
                        url.target.to_string()
                    } else {
                        inlines_to_string(&url.text)
                    }
                }
                InlineMacro::Mailto(mailto) => {
                    if mailto.text.is_empty() {
                        mailto.target.to_string()
                    } else {
                        inlines_to_string(&mailto.text)
                    }
                }
                InlineMacro::Autolink(autolink) => autolink.url.to_string(),
                InlineMacro::CrossReference(xref) => {
                    xref.text.clone().unwrap_or_else(|| xref.target.clone())
                }
                // Skip other macro types (images, footnotes, buttons, icons, etc.)
                InlineMacro::Image(_)
                | InlineMacro::Footnote(_)
                | InlineMacro::Button(_)
                | InlineMacro::Pass(_)
                | InlineMacro::Keyboard(_)
                | InlineMacro::Menu(_)
                | InlineMacro::Stem(_)
                | InlineMacro::Icon(_) => String::new(),
            },
        })
        .collect()
}
