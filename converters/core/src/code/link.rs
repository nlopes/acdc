//! Link labels and destinations used inside code blocks.

use std::{borrow::Cow, collections::HashMap};

use acdc_parser::{InlineMacro, InlineNode, Reference};

use crate::{
    InlineTextTransform,
    link::{autolink_fallback, link_fallback, mailto_fallback},
    xref::{XrefDisplay, XrefGuard, interdocument_xref, resolve_xref},
};

/// A destination for a link embedded in source code.
#[derive(Clone, Debug)]
pub enum CodeLinkTarget {
    /// A URI or another document.
    External(String),
    /// An existing ID in the current document.
    Internal(String),
    /// The start of the current document.
    DocumentTop,
}

/// The visible text and optional destination of a code link.
#[derive(Clone, Debug)]
pub struct CodeLink {
    /// Text supplied to the syntax highlighter.
    pub text: String,
    /// Absent when a cross-reference has no valid destination.
    pub target: Option<CodeLinkTarget>,
    /// An explicit ID owned by this occurrence.
    pub anchor: Option<String>,
}

/// Resolve a link node without interpreting its label as programming language syntax.
#[must_use]
#[expect(
    clippy::implicit_hasher,
    reason = "InlineTextTransform uses the parser reference catalog type"
)]
pub fn resolve_code_link(
    node: &InlineNode<'_>,
    references: &HashMap<&str, Reference<'_>>,
    output_extension: &str,
) -> Option<CodeLink> {
    let location = node.location();
    let InlineNode::Macro(node) = node else {
        return None;
    };
    let text = InlineTextTransform::default()
        .line_break("\n")
        .references(references);
    let anchor = link_anchor(node, location, references);
    let (target, label, fallback) = if let InlineMacro::Link(link) = node {
        let target = link.target.to_string();
        let fallback = link_fallback(&target, link.hides_uri_scheme()).to_string();
        (target, &link.text, fallback)
    } else if let InlineMacro::Url(link) = node {
        let target = link.target.to_string();
        let fallback = link_fallback(&target, link.hides_uri_scheme()).to_string();
        (target, &link.text, fallback)
    } else if let InlineMacro::Mailto(link) = node {
        let target = link.target.to_string();
        let fallback = mailto_fallback(&target).to_string();
        (target, &link.text, fallback)
    } else if let InlineMacro::Autolink(link) = node {
        let target = link.url.to_string();
        let (label, brackets) = autolink_fallback(&target, link.bracketed, link.hides_uri_scheme());
        return Some(CodeLink {
            text: if brackets {
                format!("<{label}>")
            } else {
                label.to_string()
            },
            target: Some(CodeLinkTarget::External(target)),
            anchor,
        });
    } else if let InlineMacro::CrossReference(xref) = node {
        let external = (!xref.target_is_local)
            .then(|| interdocument_xref(xref.target, output_extension))
            .flatten();
        let target = if xref.target_is_local && xref.target.is_empty() {
            Some(CodeLinkTarget::DocumentTop)
        } else if let Some((target, _)) = &external {
            Some(CodeLinkTarget::External(target.clone()))
        } else if references.contains_key(xref.target) {
            Some(CodeLinkTarget::Internal(xref.target.to_string()))
        } else {
            None
        };
        let label = if xref.text.is_empty() {
            let guard = XrefGuard::default();
            let text = text.in_reference();
            match resolve_xref(references.get(xref.target), xref, &guard) {
                XrefDisplay::Title(nodes, _) | XrefDisplay::Label(nodes, _) => {
                    text.to_string(nodes)
                }
                XrefDisplay::ShortCaption(prefix) => prefix,
                XrefDisplay::FullCaption(prefix, nodes, _) => {
                    format!("{prefix}, “{}”", text.to_string(nodes))
                }
                XrefDisplay::Emphasized(prefix, nodes, _) => {
                    let label = text.to_string(nodes);
                    prefix.map_or_else(|| label.clone(), |prefix| format!("{prefix}, {label}"))
                }
                XrefDisplay::External(target) => external.map_or(target, |(_, label)| label),
                XrefDisplay::Fallback(label)
                | XrefDisplay::Unresolved(label)
                | XrefDisplay::Nested(label) => label,
            }
        } else {
            text.to_string(&xref.text)
        };
        return Some(CodeLink {
            text: label,
            target,
            anchor,
        });
    } else {
        return None;
    };
    Some(CodeLink {
        text: if label.is_empty() {
            fallback
        } else {
            text.to_string(label)
        },
        target: Some(CodeLinkTarget::External(target)),
        anchor,
    })
}

fn link_anchor(
    node: &InlineMacro<'_>,
    location: &acdc_parser::Location,
    references: &HashMap<&str, Reference<'_>>,
) -> Option<String> {
    let attributes = match node {
        InlineMacro::Link(link) => Some(&link.attributes),
        InlineMacro::Url(link) => Some(&link.attributes),
        InlineMacro::Mailto(link) => Some(&link.attributes),
        InlineMacro::Autolink(_)
        | InlineMacro::CrossReference(_)
        | InlineMacro::Footnote(_)
        | InlineMacro::Icon(_)
        | InlineMacro::Image(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Button(_)
        | InlineMacro::Menu(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Stem(_)
        | InlineMacro::IndexTerm(_)
        | _ => None,
    };
    attributes
        .and_then(|attributes| attributes.get_string("id"))
        .filter(|id| {
            references
                .get(id.as_ref())
                .is_some_and(|reference| reference.location == *location)
        })
        .map(Cow::into_owned)
}

/// Inline content that keeps its place in a code block's text.
#[must_use]
pub fn code_inline_children<'n, 'a>(node: &'n InlineNode<'a>) -> Option<&'n [InlineNode<'a>]> {
    match node {
        InlineNode::BoldText(text) => Some(&text.content),
        InlineNode::ItalicText(text) => Some(&text.content),
        InlineNode::MonospaceText(text) => Some(&text.content),
        InlineNode::HighlightText(text) => Some(&text.content),
        InlineNode::SubscriptText(text) => Some(&text.content),
        InlineNode::SuperscriptText(text) => Some(&text.content),
        InlineNode::CurvedQuotationText(text) => Some(&text.content),
        InlineNode::CurvedApostropheText(text) => Some(&text.content),
        InlineNode::Macro(InlineMacro::IndexTerm(term)) if term.is_visible() => Some(term.term()),
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::CalloutRef(_)
        | InlineNode::Macro(_)
        | _ => None,
    }
}

/// Resolve the text of a code fragment and report whether it contains links.
#[must_use]
#[expect(
    clippy::implicit_hasher,
    reason = "InlineTextTransform uses the parser reference catalog type"
)]
pub fn code_link_text(
    nodes: &[InlineNode<'_>],
    references: &HashMap<&str, Reference<'_>>,
    output_extension: &str,
) -> (String, bool) {
    let mut text = String::new();
    let mut linked = false;
    for node in nodes {
        if let Some(link) = resolve_code_link(node, references, output_extension) {
            text.push_str(&link.text);
            linked = true;
        } else if let Some(children) = code_inline_children(node) {
            let (child, has_links) = code_link_text(children, references, output_extension);
            text.push_str(&child);
            linked |= has_links;
        } else {
            text.push_str(
                &InlineTextTransform::default()
                    .line_break("\n")
                    .references(references)
                    .to_string(std::slice::from_ref(node)),
            );
        }
    }
    (text, linked)
}
