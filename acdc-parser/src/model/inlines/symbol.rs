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

/// A resolved typographic symbol and its owner's consumed source span.
///
/// Split dash pairs stay with the first dash's owner. Their spans cover only
/// text consumed from that owner.
#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub(crate) kind: SymbolKind,
    pub(crate) location: Location,
}

impl Symbol {
    /// Construct an already resolved typographic symbol.
    #[must_use]
    pub fn new(kind: SymbolKind, location: Location) -> Self {
        Self { kind, location }
    }

    /// Replace the resolved symbol without changing its source span.
    pub fn set_kind(&mut self, kind: SymbolKind) {
        self.kind = kind;
    }

    /// Replace this node's recorded source span.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}
