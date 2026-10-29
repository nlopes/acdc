use crate::{InlineMacro, InlineNode};

use super::{ContentRef, InlineImageRef, LocationRef};

mod spans;

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
    CurvedQuotationRef<'doc>,
    crate::CurvedQuotation<'doc>,
    "A curved-quotation span."
);
node_ref!(
    CurvedApostropheRef<'doc>,
    crate::CurvedApostrophe<'doc>,
    "A curved-apostrophe span."
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
node_ref!(CalloutRef<'doc>, crate::CalloutRef, "A callout reference.");
node_ref!(
    FootnoteRef<'doc>,
    crate::Footnote<'doc>,
    "A footnote definition or reference."
);
node_ref!(IconRef<'doc>, crate::Icon<'doc>, "An icon macro.");
node_ref!(
    KeyboardRef<'doc>,
    crate::Keyboard<'doc>,
    "A keyboard shortcut."
);
node_ref!(ButtonRef<'doc>, crate::Button<'doc>, "A button macro.");
node_ref!(MenuRef<'doc>, crate::Menu<'doc>, "A menu path.");
node_ref!(
    UrlRef<'doc>,
    crate::Url<'doc>,
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
    PassRef<'doc>,
    crate::Pass<'doc>,
    "An inline passthrough macro."
);
node_ref!(
    StemRef<'doc>,
    crate::Stem<'doc>,
    "Inline mathematical content."
);
node_ref!(IndexTermRef<'doc>, crate::IndexTerm<'doc>, "An index term.");

/// An inline node with a semantic kind and source location.
#[derive(Clone, Copy, Debug)]
pub struct InlineRef<'doc> {
    kind: InlineKind<'doc>,
}

/// The semantic kind of an inline, including macros.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum InlineKind<'doc> {
    Plain(PlainRef<'doc>),
    Raw(RawRef<'doc>),
    Verbatim(VerbatimRef<'doc>),
    Bold(BoldRef<'doc>),
    Italic(ItalicRef<'doc>),
    Monospace(MonospaceRef<'doc>),
    Highlight(HighlightRef<'doc>),
    Subscript(SubscriptRef<'doc>),
    Superscript(SuperscriptRef<'doc>),
    CurvedQuotation(CurvedQuotationRef<'doc>),
    CurvedApostrophe(CurvedApostropheRef<'doc>),
    StandaloneCurvedApostrophe(StandaloneCurvedApostropheRef<'doc>),
    LineBreak(LineBreakRef<'doc>),
    Anchor(AnchorRef<'doc>),
    Callout(CalloutRef<'doc>),
    Footnote(FootnoteRef<'doc>),
    Icon(IconRef<'doc>),
    Image(InlineImageRef<'doc>),
    Keyboard(KeyboardRef<'doc>),
    Button(ButtonRef<'doc>),
    Menu(MenuRef<'doc>),
    Url(UrlRef<'doc>),
    Link(LinkRef<'doc>),
    Mailto(MailtoRef<'doc>),
    Autolink(AutolinkRef<'doc>),
    CrossReference(CrossReferenceRef<'doc>),
    Pass(PassRef<'doc>),
    Stem(StemRef<'doc>),
    IndexTerm(IndexTermRef<'doc>),
}

impl<'doc> InlineRef<'doc> {
    /// Match the semantic kind while retaining a concrete typed handle.
    #[must_use]
    pub fn kind(self) -> InlineKind<'doc> {
        self.kind
    }

    /// The source span of this inline node.
    #[must_use]
    pub fn location(self) -> LocationRef<'doc> {
        match self.kind {
            InlineKind::Plain(node) => node.location(),
            InlineKind::Raw(node) => node.location(),
            InlineKind::Verbatim(node) => node.location(),
            InlineKind::Bold(node) => node.location(),
            InlineKind::Italic(node) => node.location(),
            InlineKind::Monospace(node) => node.location(),
            InlineKind::Highlight(node) => node.location(),
            InlineKind::Subscript(node) => node.location(),
            InlineKind::Superscript(node) => node.location(),
            InlineKind::CurvedQuotation(node) => node.location(),
            InlineKind::CurvedApostrophe(node) => node.location(),
            InlineKind::StandaloneCurvedApostrophe(node) => node.location(),
            InlineKind::LineBreak(node) => node.location(),
            InlineKind::Anchor(node) => node.location(),
            InlineKind::Callout(node) => node.location(),
            InlineKind::Footnote(node) => node.location(),
            InlineKind::Icon(node) => node.location(),
            InlineKind::Image(node) => node.location(),
            InlineKind::Keyboard(node) => node.location(),
            InlineKind::Button(node) => node.location(),
            InlineKind::Menu(node) => node.location(),
            InlineKind::Url(node) => node.location(),
            InlineKind::Link(node) => node.location(),
            InlineKind::Mailto(node) => node.location(),
            InlineKind::Autolink(node) => node.location(),
            InlineKind::CrossReference(node) => node.location(),
            InlineKind::Pass(node) => node.location(),
            InlineKind::Stem(node) => node.location(),
            InlineKind::IndexTerm(node) => node.location(),
        }
    }
}

impl<'doc> PlainRef<'doc> {
    /// The parsed text, without rendering.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
    }
}

impl<'doc> RawRef<'doc> {
    /// The passthrough text, without rendering.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
    }
}

impl<'doc> VerbatimRef<'doc> {
    /// The literal or code text, without rendering.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
    }
}

impl<'doc> From<&'doc InlineNode<'doc>> for InlineRef<'doc> {
    fn from(inline: &'doc InlineNode<'doc>) -> Self {
        let kind = match inline {
            InlineNode::PlainText(node) => InlineKind::Plain(PlainRef::new(node)),
            InlineNode::RawText(node) => InlineKind::Raw(RawRef::new(node)),
            InlineNode::VerbatimText(node) => InlineKind::Verbatim(VerbatimRef::new(node)),
            InlineNode::BoldText(node) => InlineKind::Bold(BoldRef::new(node)),
            InlineNode::ItalicText(node) => InlineKind::Italic(ItalicRef::new(node)),
            InlineNode::MonospaceText(node) => InlineKind::Monospace(MonospaceRef::new(node)),
            InlineNode::HighlightText(node) => InlineKind::Highlight(HighlightRef::new(node)),
            InlineNode::SubscriptText(node) => InlineKind::Subscript(SubscriptRef::new(node)),
            InlineNode::SuperscriptText(node) => InlineKind::Superscript(SuperscriptRef::new(node)),
            InlineNode::CurvedQuotationText(node) => {
                InlineKind::CurvedQuotation(CurvedQuotationRef::new(node))
            }
            InlineNode::CurvedApostropheText(node) => {
                InlineKind::CurvedApostrophe(CurvedApostropheRef::new(node))
            }
            InlineNode::StandaloneCurvedApostrophe(node) => {
                InlineKind::StandaloneCurvedApostrophe(StandaloneCurvedApostropheRef::new(node))
            }
            InlineNode::LineBreak(node) => InlineKind::LineBreak(LineBreakRef::new(node)),
            InlineNode::InlineAnchor(node) => InlineKind::Anchor(AnchorRef::new(node)),
            InlineNode::CalloutRef(node) => InlineKind::Callout(CalloutRef::new(node)),
            InlineNode::Macro(node) => return Self::from(node),
        };
        Self { kind }
    }
}

impl<'doc> From<&'doc InlineMacro<'doc>> for InlineRef<'doc> {
    fn from(inline: &'doc InlineMacro<'doc>) -> Self {
        let kind = match inline {
            InlineMacro::Footnote(node) => InlineKind::Footnote(FootnoteRef::new(node)),
            InlineMacro::Icon(node) => InlineKind::Icon(IconRef::new(node)),
            InlineMacro::Image(node) => InlineKind::Image(InlineImageRef::new(node)),
            InlineMacro::Keyboard(node) => InlineKind::Keyboard(KeyboardRef::new(node)),
            InlineMacro::Button(node) => InlineKind::Button(ButtonRef::new(node)),
            InlineMacro::Menu(node) => InlineKind::Menu(MenuRef::new(node)),
            InlineMacro::Url(node) => InlineKind::Url(UrlRef::new(node)),
            InlineMacro::Link(node) => InlineKind::Link(LinkRef::new(node)),
            InlineMacro::Mailto(node) => InlineKind::Mailto(MailtoRef::new(node)),
            InlineMacro::Autolink(node) => InlineKind::Autolink(AutolinkRef::new(node)),
            InlineMacro::CrossReference(node) => {
                InlineKind::CrossReference(CrossReferenceRef::new(node))
            }
            InlineMacro::Pass(node) => InlineKind::Pass(PassRef::new(node)),
            InlineMacro::Stem(node) => InlineKind::Stem(StemRef::new(node)),
            InlineMacro::IndexTerm(node) => InlineKind::IndexTerm(IndexTermRef::new(node)),
        };
        Self { kind }
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
}

impl<'doc> From<&'doc crate::Anchor<'doc>> for AnchorRef<'doc> {
    fn from(owner: &'doc crate::Anchor<'doc>) -> Self {
        Self::new(owner)
    }
}

impl CalloutRef<'_> {
    /// Whether the marker was explicit or automatically numbered.
    #[must_use]
    pub fn kind(self) -> crate::CalloutRefKind {
        self.node.kind
    }

    /// The resolved number within its callout sequence, not a global identity.
    #[must_use]
    pub fn number(self) -> usize {
        self.node.number
    }
}

impl<'doc> From<&'doc crate::CalloutRef> for CalloutRef<'doc> {
    fn from(owner: &'doc crate::CalloutRef) -> Self {
        Self::new(owner)
    }
}
