use serde::{
    Deserialize, Serialize,
    de::{SeqAccess, Visitor},
    ser::{SerializeSeq, Serializer},
};

pub(crate) trait Locateable {
    /// Get a reference to the location.
    fn location(&self) -> &Location;
}

/// A `Location` represents a location in a document.
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
#[non_exhaustive]
pub struct Location {
    /// The absolute start position of the location.
    pub absolute_start: usize,
    /// The absolute end position of the location.
    pub absolute_end: usize,

    /// The start position of the location.
    pub start: Position,
    /// The end position of the location.
    pub end: Position,
}

impl Location {
    /// Validates that this location satisfies all invariants.
    ///
    /// Checks:
    /// - `absolute_start <= absolute_end` (valid range)
    /// - `absolute_end <= input.len()` (within bounds)
    /// - Both offsets are on UTF-8 character boundaries
    ///
    /// # Errors
    /// Returned as strings for easier debugging.
    pub fn validate(&self, input: &str) -> Result<(), String> {
        // Check range validity using the canonical byte offsets
        if self.absolute_start > self.absolute_end {
            return Err(format!(
                "Invalid range: start {} > end {}",
                self.absolute_start, self.absolute_end
            ));
        }

        // Check bounds
        if self.absolute_end > input.len() {
            return Err(format!(
                "End offset {} exceeds input length {}",
                self.absolute_end,
                input.len()
            ));
        }

        // Check UTF-8 boundaries on the canonical offsets
        if !input.is_char_boundary(self.absolute_start) {
            return Err(format!(
                "Start offset {} not on UTF-8 boundary",
                self.absolute_start
            ));
        }

        if !input.is_char_boundary(self.absolute_end) {
            return Err(format!(
                "End offset {} not on UTF-8 boundary",
                self.absolute_end
            ));
        }

        Ok(())
    }

    /// Shift the start and end positions of the location by the parent location.
    ///
    /// This is super useful to adjust the location of a block that is inside another
    /// block, like anything inside a delimiter block.
    pub fn shift(&mut self, parent: Option<&Location>) {
        if let Some(parent) = parent {
            if parent.start.line == 0 {
                return;
            }
            self.absolute_start += parent.absolute_start;
            self.absolute_end += parent.absolute_start;
            self.start.line += parent.start.line;
            self.end.line += parent.start.line;
        }
    }

    /// Shifts the location inline. We subtract 1 from the line number of the start and
    /// end to account for the fact that inlines are always in the same line as the
    /// parent calling the parsing function.
    pub fn shift_inline(&mut self, parent: Option<&Location>) {
        if let Some(parent) = parent {
            if parent.start.line != 0 || parent.start.column != 0 {
                self.absolute_start += parent.absolute_start;
                self.absolute_end += parent.absolute_start;
            }
            if parent.start.line != 0 {
                self.start.line += parent.start.line - 1;
                self.end.line += parent.start.line - 1;
            }
            if parent.start.column != 0 {
                self.start.column += parent.start.column - 1;
                self.end.column += parent.start.column - 1;
            }
        }
    }

    pub fn shift_line_column(&mut self, line: usize, column: usize) {
        self.start.line += line - 1;
        self.end.line += line - 1;
        self.start.column += column - 1;
        self.end.column += column - 1;
    }
}

// We need to implement `Serialize` because I prefer our current `Location` struct to the
// `asciidoc` `ASG` definition.
//
// We serialize `Location` into the ASG format, which is a sequence of two elements: the
// start and end positions as an array.
impl Serialize for Location {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_seq(Some(4))?;
        state.serialize_element(&self.start)?;
        state.serialize_element(&self.end)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Location {
    fn deserialize<D>(deserializer: D) -> Result<Location, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct LocationVisitor;

        impl<'de> Visitor<'de> for LocationVisitor {
            type Value = Location;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence of two elements")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let start = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let end = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                Ok(Location {
                    start,
                    end,
                    ..Location::default()
                })
            }
        }
        deserializer.deserialize_seq(LocationVisitor)
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "location.start({}), location.end({})",
            self.start, self.end
        )
    }
}

/// A `Position` represents a human-readable position in a document.
///
/// This is purely for display/error reporting purposes. For byte offsets,
/// use `Location.absolute_start` and `Location.absolute_end`.
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Position {
    /// The line number of the position (1-indexed).
    pub line: usize,
    /// The column number of the position (1-indexed, counted as Unicode scalar values).
    #[serde(rename = "col")]
    pub column: usize,
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line: {}, column: {}", self.line, self.column)
    }
}
