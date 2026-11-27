use crate::InlineNode;

use super::{ContentRef, InlineImageRef, LocationRef};

mod hyperlinks;
mod icons;
mod index_terms;
mod notes;
mod spans;
mod stem;
mod text;
mod ui;
mod xrefs;

pub use hyperlinks::HyperlinkRef;
pub use index_terms::{IndexTermLabelsRef, IndexTermRelationshipRef};

node_ref!(
    SymbolRef<'doc>,
    crate::Symbol,
    "A resolved typographic symbol."
);
node_ref!(
    CharacterReferenceRef<'doc>,
    crate::CharacterReference<'doc>,
    "An identified character reference."
);
node_ref!(PlainRef<'doc>, crate::Plain<'doc>, "Plain text.");
node_ref!(
    RawRef<'doc>,
    crate::Raw<'doc>,
    "Passthrough text with its own substitutions."
);
node_ref!(
    VerbatimRef<'doc>,
    crate::Verbatim<'doc>,
    "Text with verbatim whitespace and escaping."
);
node_ref!(BoldRef<'doc>, crate::Bold<'doc>, "A bold span.");
node_ref!(ItalicRef<'doc>, crate::Italic<'doc>, "An italic span.");
node_ref!(
    MonospaceRef<'doc>,
    crate::Monospace<'doc>,
    "A monospace span."
);
node_ref!(
    HighlightRef<'doc>,
    crate::Highlight<'doc>,
    "A marked or generic span."
);
node_ref!(
    SubscriptRef<'doc>,
    crate::Subscript<'doc>,
    "A subscript span."
);
node_ref!(
    SuperscriptRef<'doc>,
    crate::Superscript<'doc>,
    "A superscript span."
);
node_ref!(
    DoubleQuotedSpanRef<'doc>,
    crate::DoubleQuotedSpan<'doc>,
    "A span enclosed in typographic double quotation marks."
);
node_ref!(
    SingleQuotedSpanRef<'doc>,
    crate::SingleQuotedSpan<'doc>,
    "A span enclosed in typographic single quotation marks."
);
node_ref!(
    StandaloneCurvedApostropheRef<'doc>,
    crate::StandaloneCurvedApostrophe,
    "A standalone curved apostrophe."
);
node_ref!(
    LineBreakRef<'doc>,
    crate::LineBreak,
    "An inline line break."
);
node_ref!(AnchorRef<'doc>, crate::Anchor<'doc>, "An anchor.");
node_ref!(
    CalloutMarkerRef<'doc>,
    crate::CalloutMarker,
    "A callout marker."
);
node_ref!(
    FootnoteRef<'doc>,
    crate::Footnote<'doc>,
    "A footnote definition or reference."
);
node_ref!(IconRef<'doc>, crate::Icon<'doc>, "An icon macro.");
node_ref!(
    KeyboardShortcutRef<'doc>,
    crate::KeyboardShortcut<'doc>,
    "A keyboard shortcut."
);
node_ref!(ButtonRef<'doc>, crate::Button<'doc>, "A button macro.");
node_ref!(MenuPathRef<'doc>, crate::MenuPath<'doc>, "A menu path.");
node_ref!(
    UrlMacroRef<'doc>,
    crate::UrlMacro<'doc>,
    "A URL with optional label content."
);
node_ref!(LinkRef<'doc>, crate::Link<'doc>, "An explicit link.");
node_ref!(MailtoRef<'doc>, crate::Mailto<'doc>, "An email link.");
node_ref!(
    AutolinkRef<'doc>,
    crate::Autolink<'doc>,
    "An automatic link."
);
node_ref!(
    CrossReferenceRef<'doc>,
    crate::CrossReference<'doc>,
    "A cross-reference."
);
node_ref!(
    InlinePassthroughRef<'doc>,
    crate::InlinePassthrough<'doc>,
    "An inline passthrough macro."
);
node_ref!(
    StemRef<'doc>,
    crate::Stem<'doc>,
    "Inline mathematical content."
);
node_ref!(IndexTermRef<'doc>, crate::IndexTerm<'doc>, "An index term.");

/// An inline node with a semantic kind and source location.
///
/// Copy a selected node with `InlineNode::from(inline)`. Copies keep source
/// locations and registration snapshots, and can still borrow source text:
///
/// ```compile_fail,E0597
/// use acdc_parser::{Options, parse_inline, edit::InlineNode};
/// let copied = {
///     let parsed = parse_inline("Text", &Options::default()).unwrap();
///     InlineNode::from(parsed.content().inlines().next().unwrap())
/// };
/// drop(copied);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct InlineRef<'doc> {
    node: &'doc InlineNode<'doc>,
}

/// The semantic kind of an inline, including macros.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum InlineKind<'doc> {
    Plain(PlainRef<'doc>),
    Symbol(SymbolRef<'doc>),
    CharacterReference(CharacterReferenceRef<'doc>),
    Raw(RawRef<'doc>),
    Verbatim(VerbatimRef<'doc>),
    Bold(BoldRef<'doc>),
    Italic(ItalicRef<'doc>),
    Monospace(MonospaceRef<'doc>),
    Highlight(HighlightRef<'doc>),
    Subscript(SubscriptRef<'doc>),
    Superscript(SuperscriptRef<'doc>),
    DoubleQuotedSpan(DoubleQuotedSpanRef<'doc>),
    SingleQuotedSpan(SingleQuotedSpanRef<'doc>),
    StandaloneCurvedApostrophe(StandaloneCurvedApostropheRef<'doc>),
    LineBreak(LineBreakRef<'doc>),
    Anchor(AnchorRef<'doc>),
    Callout(CalloutMarkerRef<'doc>),
    Footnote(FootnoteRef<'doc>),
    Icon(IconRef<'doc>),
    Image(InlineImageRef<'doc>),
    KeyboardShortcut(KeyboardShortcutRef<'doc>),
    Button(ButtonRef<'doc>),
    MenuPath(MenuPathRef<'doc>),
    UrlMacro(UrlMacroRef<'doc>),
    Link(LinkRef<'doc>),
    Mailto(MailtoRef<'doc>),
    Autolink(AutolinkRef<'doc>),
    CrossReference(CrossReferenceRef<'doc>),
    Passthrough(InlinePassthroughRef<'doc>),
    Stem(StemRef<'doc>),
    IndexTerm(IndexTermRef<'doc>),
}

impl<'doc> InlineRef<'doc> {
    /// Match the semantic kind while retaining a concrete typed handle.
    #[must_use]
    pub fn kind(self) -> InlineKind<'doc> {
        InlineKind::from(self.node)
    }

    /// The source span of this inline node.
    #[must_use]
    pub fn location(self) -> LocationRef<'doc> {
        LocationRef::from(self.node.location())
    }
}

impl<'doc> From<&'doc InlineNode<'doc>> for InlineRef<'doc> {
    fn from(inline: &'doc InlineNode<'doc>) -> Self {
        Self { node: inline }
    }
}

impl<'doc> From<&'doc InlineNode<'doc>> for InlineKind<'doc> {
    fn from(inline: &'doc InlineNode<'doc>) -> Self {
        match inline {
            InlineNode::Symbol(node) => InlineKind::Symbol(SymbolRef::new(node)),
            InlineNode::CharacterReference(node) => {
                InlineKind::CharacterReference(CharacterReferenceRef::new(node))
            }
            InlineNode::Extension(call) => InlineKind::Plain(PlainRef::new(&call.literal)),
            InlineNode::Plain(node) => InlineKind::Plain(PlainRef::new(node)),
            InlineNode::Raw(node) => InlineKind::Raw(RawRef::new(node)),
            InlineNode::Verbatim(node) => InlineKind::Verbatim(VerbatimRef::new(node)),
            InlineNode::Bold(node) => InlineKind::Bold(BoldRef::new(node)),
            InlineNode::Italic(node) => InlineKind::Italic(ItalicRef::new(node)),
            InlineNode::Monospace(node) => InlineKind::Monospace(MonospaceRef::new(node)),
            InlineNode::Highlight(node) => InlineKind::Highlight(HighlightRef::new(node)),
            InlineNode::Subscript(node) => InlineKind::Subscript(SubscriptRef::new(node)),
            InlineNode::Superscript(node) => InlineKind::Superscript(SuperscriptRef::new(node)),
            InlineNode::DoubleQuotedSpan(node) => {
                InlineKind::DoubleQuotedSpan(DoubleQuotedSpanRef::new(node))
            }
            InlineNode::SingleQuotedSpan(node) => {
                InlineKind::SingleQuotedSpan(SingleQuotedSpanRef::new(node))
            }
            InlineNode::StandaloneCurvedApostrophe(node) => {
                InlineKind::StandaloneCurvedApostrophe(StandaloneCurvedApostropheRef::new(node))
            }
            InlineNode::LineBreak(node) => InlineKind::LineBreak(LineBreakRef::new(node)),
            InlineNode::Anchor(node) => InlineKind::Anchor(AnchorRef::new(node)),
            InlineNode::Callout(node) => InlineKind::Callout(CalloutMarkerRef::new(node)),
            InlineNode::Footnote(node) => InlineKind::Footnote(FootnoteRef::new(node)),
            InlineNode::Icon(node) => InlineKind::Icon(IconRef::new(node)),
            InlineNode::Image(node) => InlineKind::Image(InlineImageRef::new(node)),
            InlineNode::KeyboardShortcut(node) => {
                InlineKind::KeyboardShortcut(KeyboardShortcutRef::new(node))
            }
            InlineNode::Button(node) => InlineKind::Button(ButtonRef::new(node)),
            InlineNode::MenuPath(node) => InlineKind::MenuPath(MenuPathRef::new(node)),
            InlineNode::UrlMacro(node) => InlineKind::UrlMacro(UrlMacroRef::new(node)),
            InlineNode::Link(node) => InlineKind::Link(LinkRef::new(node)),
            InlineNode::Mailto(node) => InlineKind::Mailto(MailtoRef::new(node)),
            InlineNode::Autolink(node) => InlineKind::Autolink(AutolinkRef::new(node)),
            InlineNode::CrossReference(node) => {
                InlineKind::CrossReference(CrossReferenceRef::new(node))
            }
            InlineNode::Passthrough(node) => {
                InlineKind::Passthrough(InlinePassthroughRef::new(node))
            }
            InlineNode::Stem(node) => InlineKind::Stem(StemRef::new(node)),
            InlineNode::IndexTerm(node) => InlineKind::IndexTerm(IndexTermRef::new(node)),
        }
    }
}

impl<'doc> AnchorRef<'doc> {
    /// The anchor's ID text.
    #[must_use]
    pub fn id(self) -> &'doc str {
        self.node.id
    }

    /// Supplied reference text, including explicitly empty content.
    #[must_use]
    pub fn reftext(self) -> Option<ContentRef<'doc>> {
        self.node.xreflabel.as_ref().map(ContentRef::from)
    }

    /// Whether this anchor identifies a bibliography entry.
    #[must_use]
    pub fn is_bibliography(self) -> bool {
        self.node.is_bibliography()
    }

    /// The bibliography entry's display label, separate from citation `reftext()`.
    /// `None` means an ordinary anchor or an entry that uses its ID as the label.
    #[must_use]
    pub fn bibliography_label(self) -> Option<ContentRef<'doc>> {
        self.node.bibliography_label().map(ContentRef::new)
    }
}

impl<'doc> From<&'doc crate::Anchor<'doc>> for AnchorRef<'doc> {
    fn from(owner: &'doc crate::Anchor<'doc>) -> Self {
        Self::new(owner)
    }
}

impl CalloutMarkerRef<'_> {
    /// Whether the marker was explicit or automatically numbered.
    #[must_use]
    pub fn kind(self) -> crate::CalloutMarkerKind {
        self.node.kind
    }

    /// The resolved number within its callout sequence, not a global identity.
    #[must_use]
    pub fn number(self) -> usize {
        self.node.number
    }
}

impl<'doc> From<&'doc crate::CalloutMarker> for CalloutMarkerRef<'doc> {
    fn from(owner: &'doc crate::CalloutMarker) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<AnchorRef<'doc>> for crate::Anchor<'doc> {
    fn from(read: AnchorRef<'doc>) -> Self {
        read.node.clone()
    }
}

impl From<CalloutMarkerRef<'_>> for crate::CalloutMarker {
    fn from(read: CalloutMarkerRef<'_>) -> Self {
        read.node.clone()
    }
}

impl<'doc> From<InlineRef<'doc>> for InlineNode<'doc> {
    fn from(read: InlineRef<'doc>) -> Self {
        read.node.clone()
    }
}
