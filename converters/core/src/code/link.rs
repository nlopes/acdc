//! Link labels, destinations, and footnote markers used inside code blocks.

use std::fmt::Write as _;

use acdc_parser::ast::{ContentRef, InlineKind, InlineRef, ReferencesRef};

use crate::{
    InlineTextTransform,
    link::{autolink_fallback, link_fallback, mailto_fallback, mailto_target},
    xref::{XrefDisplay, XrefGuard, interdocument_xref, resolve_reference, with_caption_prefix},
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
    /// Display text supplied to the syntax highlighter, with parsed references decoded.
    pub text: String,
    /// Absent when a cross-reference has no valid destination.
    pub target: Option<CodeLinkTarget>,
    /// An explicit ID owned by this occurrence.
    pub anchor: Option<String>,
}

/// Resolve a link node without interpreting its label as programming language syntax.
///
/// Accepts typed inline handles and existing owner references. The result owns
/// its text, destination, and ID, so it can outlive the inline input.
#[must_use]
pub fn resolve_code_link<'inline>(
    node: impl Into<InlineRef<'inline>>,
    references: ReferencesRef<'_>,
    output_extension: &str,
) -> Option<CodeLink> {
    let node = node.into();
    let kind = node.kind();
    let text = InlineTextTransform::default()
        .line_break("\n")
        .decode_char_refs(true)
        .references(references);
    let anchor = link_anchor(node, references);
    let (target, label, fallback) = if let InlineKind::Link(link) = kind {
        let target = link.target().to_string();
        let fallback = link_fallback(&target, link.hides_uri_scheme()).to_string();
        (target, link.text(), fallback)
    } else if let InlineKind::UrlMacro(link) = kind {
        let target = link.target().to_string();
        let fallback = link_fallback(&target, link.hides_uri_scheme()).to_string();
        (target, link.text(), fallback)
    } else if let InlineKind::Mailto(link) = kind {
        let target = link.target().to_string();
        let fallback = mailto_fallback(&target).to_string();
        (mailto_target(link), link.text(), fallback)
    } else if let InlineKind::Autolink(link) = kind {
        let target = link.target().to_string();
        let (label, brackets) =
            autolink_fallback(&target, link.is_bracketed(), link.hides_uri_scheme());
        return Some(CodeLink {
            text: if brackets {
                format!("<{label}>")
            } else {
                label.to_string()
            },
            target: Some(CodeLinkTarget::External(target)),
            anchor,
        });
    } else if let InlineKind::CrossReference(xref) = kind {
        let external = (!xref.target_is_local())
            .then(|| interdocument_xref(xref.target(), output_extension))
            .flatten();
        let target = if xref.target_is_local() && xref.target().is_empty() {
            Some(CodeLinkTarget::DocumentTop)
        } else if let Some((target, _)) = &external {
            Some(CodeLinkTarget::External(target.clone()))
        } else if references.contains(xref.target()) {
            Some(CodeLinkTarget::Internal(xref.target().to_string()))
        } else {
            None
        };
        let label = if xref.text().is_empty() {
            let guard = XrefGuard::default();
            let text = text.in_reference();
            match resolve_reference(references.get(xref.target()), xref, &guard) {
                XrefDisplay::Title(nodes, _) | XrefDisplay::Label(nodes, _) => {
                    text.content_to_string(nodes)
                }
                XrefDisplay::ShortCaption(prefix) => prefix,
                XrefDisplay::FullCaption(prefix, nodes, _) => {
                    format!("{prefix}, “{}”", text.content_to_string(nodes))
                }
                XrefDisplay::ShortCustomCaption(prefix, _scope) => {
                    with_caption_prefix(prefix, |prefix| text.content_to_string(prefix))
                }
                XrefDisplay::FullCustomCaption(prefix, nodes, _scope) => {
                    let prefix =
                        with_caption_prefix(prefix, |prefix| text.content_to_string(prefix));
                    format!("{prefix}, “{}”", text.content_to_string(nodes))
                }
                XrefDisplay::Emphasized(prefix, nodes, _) => {
                    let label = text.content_to_string(nodes);
                    prefix.map_or_else(|| label.clone(), |prefix| format!("{prefix}, {label}"))
                }
                XrefDisplay::External(target) => external.map_or(target, |(_, label)| label),
                XrefDisplay::Fallback(label)
                | XrefDisplay::Unresolved(label)
                | XrefDisplay::Nested(label) => label,
            }
        } else {
            text.content_to_string(xref.text())
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
            text.content_to_string(label)
        },
        target: Some(CodeLinkTarget::External(target)),
        anchor,
    })
}

fn link_anchor(node: InlineRef<'_>, references: ReferencesRef<'_>) -> Option<String> {
    if !matches!(
        node.kind(),
        InlineKind::Link(_) | InlineKind::UrlMacro(_) | InlineKind::Mailto(_)
    ) {
        return None;
    }
    let id = node.id()?.as_str();
    references
        .get(id)
        .filter(|reference| reference.location() == node.location())
        .map(|_| id.to_owned())
}

/// Direct content of formatting spans and visible index terms in source code.
///
/// Hidden index terms and other inline kinds have no code child content.
/// An explicitly empty formatting span still returns `Some`.
#[must_use]
pub fn code_inline_content(node: InlineRef<'_>) -> Option<ContentRef<'_>> {
    match node.kind() {
        InlineKind::Bold(text) => Some(text.content()),
        InlineKind::Italic(text) => Some(text.content()),
        InlineKind::Monospace(text) => Some(text.content()),
        InlineKind::Highlight(text) => Some(text.content()),
        InlineKind::Subscript(text) => Some(text.content()),
        InlineKind::Superscript(text) => Some(text.content()),
        InlineKind::DoubleQuotedSpan(text) => Some(text.content()),
        InlineKind::SingleQuotedSpan(text) => Some(text.content()),
        InlineKind::IndexTerm(term) if term.is_visible() => Some(term.term()),
        InlineKind::Plain(_)
        | InlineKind::Raw(_)
        | InlineKind::Verbatim(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::CharacterReference(_)
        | InlineKind::Symbol(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Anchor(_)
        | InlineKind::Callout(_)
        | InlineKind::Footnote(_)
        | InlineKind::Icon(_)
        | InlineKind::Image(_)
        | InlineKind::KeyboardShortcut(_)
        | InlineKind::Button(_)
        | InlineKind::MenuPath(_)
        | InlineKind::UrlMacro(_)
        | InlineKind::Link(_)
        | InlineKind::Mailto(_)
        | InlineKind::Autolink(_)
        | InlineKind::CrossReference(_)
        | InlineKind::Passthrough(_)
        | InlineKind::Stem(_)
        | InlineKind::IndexTerm(_)
        | _ => None,
    }
}

/// Resolve code text and report whether it needs links, anchors, or footnote markers.
///
/// Accepts typed inline iterators and existing owner slices.
#[must_use]
pub fn code_inline_text<'inline>(
    nodes: impl IntoIterator<Item = impl Into<InlineRef<'inline>>>,
    references: ReferencesRef<'_>,
    output_extension: &str,
) -> (String, bool) {
    let mut text = String::new();
    let mut linked = false;
    for node in nodes {
        let node = node.into();
        if matches!(node.kind(), InlineKind::Anchor(_)) {
            linked = true;
        } else if let InlineKind::Footnote(note) = node.kind() {
            let _ = write!(text, "[{}]", note.number());
            linked = true;
        } else if let Some(link) = resolve_code_link(node, references, output_extension) {
            text.push_str(&link.text);
            linked = true;
        } else if let Some(children) = code_inline_content(node) {
            let (child, has_links) =
                code_inline_text(children.inlines(), references, output_extension);
            text.push_str(&child);
            linked |= has_links;
        } else {
            let _ = InlineTextTransform::default()
                .line_break("\n")
                .decode_char_refs(true)
                .references(references)
                .write_inlines(&mut text, std::iter::once(node));
        }
    }
    (text, linked)
}
