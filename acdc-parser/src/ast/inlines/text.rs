use crate::{
    LineBreak, Pass, PassthroughKind, Plain, Raw, StandaloneCurvedApostrophe, Substitution,
    Verbatim,
};

use super::{LineBreakRef, PassRef, PlainRef, RawRef, StandaloneCurvedApostropheRef, VerbatimRef};

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
    /// The passthrough text, without rendering.
    #[must_use]
    pub fn text(self) -> &'doc str {
        self.node.content
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
