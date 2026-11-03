use crate::Location;

/// Resolved typography, including spacing that belongs to the replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SymbolKind {
    /// An em dash between thin spaces.
    SpacedEmDash,
    /// An em dash followed by a possible word-break position.
    WordBoundedEmDash,
    /// A horizontal ellipsis.
    Ellipsis,
    /// The copyright sign.
    Copyright,
    /// The registered trademark sign.
    RegisteredTrademark,
    /// The trademark sign.
    Trademark,
    /// A typographic apostrophe.
    CurvedApostrophe,
    /// A rightwards arrow.
    RightArrow,
    /// A leftwards arrow.
    LeftArrow,
    /// A rightwards double arrow.
    RightDoubleArrow,
    /// A leftwards double arrow.
    LeftDoubleArrow,
}

impl SymbolKind {
    /// The resolved Unicode text, without backend encoding.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::SpacedEmDash => "\u{2009}\u{2014}\u{2009}",
            Self::WordBoundedEmDash => "\u{2014}\u{200b}",
            Self::Ellipsis => "\u{2026}",
            Self::Copyright => "\u{00a9}",
            Self::RegisteredTrademark => "\u{00ae}",
            Self::Trademark => "\u{2122}",
            Self::CurvedApostrophe => "\u{2019}",
            Self::RightArrow => "\u{2192}",
            Self::LeftArrow => "\u{2190}",
            Self::RightDoubleArrow => "\u{21d2}",
            Self::LeftDoubleArrow => "\u{21d0}",
        }
    }
}

/// A resolved typographic symbol and its authored source span.
#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub location: Location,
}
