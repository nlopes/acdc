use crate::{
    LineBreak, Pass, PassthroughKind, Plain, Raw, StandaloneCurvedApostrophe, Substitution,
    Verbatim, ast::TextPartRef,
};

use super::{
    LineBreakRef, PassRef, PlainRef, RawRef, StandaloneCurvedApostropheRef, SymbolRef, VerbatimRef,
};

impl<'doc> PlainRef<'doc> {
    /// The parsed text, without rendering or reconstructing source escapes.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
    }

    /// Whether an escape prevented this text from becoming inline formatting.
    /// Quote substitution must not reinterpret escaped text.
    #[must_use]
    pub fn is_escaped(self) -> bool {
        self.node.escaped
    }
}

impl<'doc> RawRef<'doc> {
    /// The passthrough's retained identity and role tokens.
    #[must_use]
    pub fn attributes(self) -> super::super::AttributesRef<'doc> {
        super::super::AttributesRef::raw(self.node)
    }

    /// The retained element ID.
    #[must_use]
    pub fn id(self) -> Option<super::super::ElementIdRef<'doc>> {
        self.attributes().id()
    }

    /// The retained role tokens, in order.
    #[must_use]
    pub fn roles(self) -> super::super::TokensRef<'doc> {
        self.attributes().roles()
    }

    /// The passthrough text, without rendering.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
    }

    /// The text and remaining substitutions as a shared view.
    ///
    /// This is the same view returned for document attribute pieces. Identity,
    /// roles, and source location remain available on this raw inline node.
    #[must_use]
    pub fn text_part(self) -> TextPartRef<'doc> {
        TextPartRef::new(self.node.content, &self.node.subs)
    }

    /// Remaining substitutions in order, including repeated entries.
    /// These replace the enclosing block's substitutions; an empty sequence
    /// leaves the text unchanged for raw output.
    pub fn substitutions(self) -> impl Iterator<Item = Substitution> + Send + Sync + 'doc {
        self.node.subs.iter().cloned()
    }
}

impl<'doc> VerbatimRef<'doc> {
    /// The literal or code text, with its retained whitespace.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
    }
}

impl<'doc> PassRef<'doc> {
    /// Unprocessed macro content, distinguishing absent from empty text.
    /// Document parsing lowers passthroughs into text or structured inlines;
    /// standalone inline parsing can retain a passthrough macro.
    #[must_use]
    pub fn text(self) -> Option<&'doc str> {
        self.node.text
    }

    /// The retained passthrough form, without reconstructing delimiters.
    #[must_use]
    pub fn kind(self) -> PassthroughKind {
        self.node.kind.clone()
    }

    /// Requested substitutions in order, before processing the macro content.
    pub fn substitutions(self) -> impl Iterator<Item = Substitution> + Send + Sync + 'doc {
        self.node.substitutions.iter().cloned()
    }
}

impl<'doc> From<&'doc Plain<'doc>> for PlainRef<'doc> {
    fn from(owner: &'doc Plain<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<RawRef<'doc>> for Raw<'doc> {
    fn from(owner: RawRef<'doc>) -> Self {
        owner.node.clone()
    }
}

impl<'doc> From<&'doc Raw<'doc>> for RawRef<'doc> {
    fn from(owner: &'doc Raw<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc Verbatim<'doc>> for VerbatimRef<'doc> {
    fn from(owner: &'doc Verbatim<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc Pass<'doc>> for PassRef<'doc> {
    fn from(owner: &'doc Pass<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc LineBreak> for LineBreakRef<'doc> {
    fn from(owner: &'doc LineBreak) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc StandaloneCurvedApostrophe> for StandaloneCurvedApostropheRef<'doc> {
    fn from(owner: &'doc StandaloneCurvedApostrophe) -> Self {
        Self::new(owner)
    }
}

impl SymbolRef<'_> {
    /// The resolved typographic meaning.
    #[must_use]
    pub fn kind(self) -> crate::SymbolKind {
        self.node.kind
    }

    /// The resolved Unicode text, without backend encoding.
    #[must_use]
    pub fn text(self) -> &'static str {
        self.node.kind.text()
    }
}

impl<'doc> From<&'doc crate::Symbol> for SymbolRef<'doc> {
    fn from(owner: &'doc crate::Symbol) -> Self {
        Self::new(owner)
    }
}

impl<'doc> super::CharacterReferenceRef<'doc> {
    /// The identified name or numeric value.
    #[must_use]
    pub fn kind(self) -> crate::CharacterReferenceKind<'doc> {
        self.node.kind()
    }
}

impl<'doc> From<&'doc crate::CharacterReference<'doc>> for super::CharacterReferenceRef<'doc> {
    fn from(owner: &'doc crate::CharacterReference<'doc>) -> Self {
        Self::new(owner)
    }
}

/// Copy the selected atom, retaining its prepared value and source span.
impl<'doc> From<PlainRef<'doc>> for Plain<'doc> {
    fn from(value: PlainRef<'doc>) -> Self {
        value.node.clone()
    }
}

/// Copy the selected atom, retaining its prepared value and source span.
impl<'doc> From<VerbatimRef<'doc>> for Verbatim<'doc> {
    fn from(value: VerbatimRef<'doc>) -> Self {
        value.node.clone()
    }
}

/// Copy the selected atom, retaining its prepared value and source span.
impl<'doc> From<SymbolRef<'doc>> for crate::Symbol {
    fn from(value: SymbolRef<'doc>) -> Self {
        value.node.clone()
    }
}

/// Copy the selected atom, retaining its prepared value and source span.
impl<'doc> From<super::CharacterReferenceRef<'doc>> for crate::CharacterReference<'doc> {
    fn from(value: super::CharacterReferenceRef<'doc>) -> Self {
        value.node.clone()
    }
}

/// Copy the selected atom, retaining its prepared value and source span.
impl<'doc> From<LineBreakRef<'doc>> for LineBreak {
    fn from(value: LineBreakRef<'doc>) -> Self {
        value.node.clone()
    }
}

/// Copy the selected atom, retaining its prepared value and source span.
impl<'doc> From<StandaloneCurvedApostropheRef<'doc>> for StandaloneCurvedApostrophe {
    fn from(value: StandaloneCurvedApostropheRef<'doc>) -> Self {
        value.node.clone()
    }
}

/// Copy the selected passthrough, retaining its requested interpretation and source span.
impl<'doc> From<PassRef<'doc>> for Pass<'doc> {
    fn from(value: PassRef<'doc>) -> Self {
        value.node.clone()
    }
}
