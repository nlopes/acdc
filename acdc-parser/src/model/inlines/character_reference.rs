use std::fmt;

use serde::{Serialize, Serializer, ser::SerializeMap};

use crate::Location;

/// The name or numeric value identified by a character reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CharacterReferenceKind<'a> {
    /// A named reference, without the surrounding `&` and `;`.
    Named(&'a str),
    /// A numeric code point, independent of decimal or hexadecimal source syntax.
    Numeric(u32),
}

impl fmt::Display for CharacterReferenceKind<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name) => write!(f, "&{name};"),
            Self::Numeric(value) => write!(f, "&#{value};"),
        }
    }
}

impl Serialize for CharacterReferenceKind<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A named or numeric character reference.
///
/// Decimal/hexadecimal spelling and zero padding are source details. The numeric
/// value and named identity remain available without recognizing syntax again.
/// Unknown names and invalid Unicode code points can be retained for recovery.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CharacterReference<'a> {
    pub(crate) kind: CharacterReferenceKind<'a>,
    pub location: Location,
}

impl<'a> CharacterReference<'a> {
    /// Construct a reference from its identified value.
    ///
    /// Returns `None` for an invalid XML-style name. Numeric values need not be
    /// valid Unicode characters: they may describe recovered input.
    #[must_use]
    pub fn new(kind: CharacterReferenceKind<'a>, location: Location) -> Option<Self> {
        if let CharacterReferenceKind::Named(name) = kind {
            let mut characters = name.chars();
            if !characters.next().is_some_and(name_start) || !characters.all(name_continue) {
                return None;
            }
        }
        Some(Self { kind, location })
    }

    pub(crate) fn name_start(value: char) -> bool {
        name_start(value)
    }

    pub(crate) fn name_continue(value: char) -> bool {
        name_continue(value)
    }

    /// The named identity or numeric value, without source-format distinctions.
    #[must_use]
    pub fn kind(&self) -> CharacterReferenceKind<'a> {
        self.kind
    }
}

impl Serialize for CharacterReference<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("name", "charref")?;
        map.serialize_entry("type", "string")?;
        map.serialize_entry("value", &self.kind)?;
        map.serialize_entry("location", &self.location)?;
        map.end()
    }
}

// XML 1.0 (Fifth Edition), productions [4] and [4a].
fn name_start(value: char) -> bool {
    matches!(value, ':' | '_' | 'A'..='Z' | 'a'..='z' | '\u{c0}'..='\u{d6}'
        | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}' | '\u{370}'..='\u{37d}'
        | '\u{37f}'..='\u{1fff}' | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}'
        | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}'
        | '\u{fdf0}'..='\u{fffd}' | '\u{10000}'..='\u{effff}')
}

fn name_continue(value: char) -> bool {
    name_start(value)
        || matches!(value, '-' | '.' | '0'..='9' | '\u{b7}'
        | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}')
}
