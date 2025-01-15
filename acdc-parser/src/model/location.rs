use serde::{
    de::{SeqAccess, Visitor},
    ser::{SerializeSeq, Serializer},
    Deserialize, Serialize,
};

/// A `Location` represents a location in a document.
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct Location {
    /// The absolute start position of the location.
    pub(crate) absolute_start: usize,
    /// The absolute end position of the location.
    pub(crate) absolute_end: usize,

    /// The start position of the location.
    pub start: Position,
    /// The end position of the location.
    pub end: Position,
}

impl Location {
    #[must_use]
    pub fn from_pair<R: pest::RuleType>(pair: &pest::iterators::Pair<R>) -> Self {
        let mut location = Location::default();
        let start = pair.as_span().start_pos();
        let end = pair.as_span().end_pos();
        location.set_start_from_pos(&start);
        location.set_end_from_pos(&end);
        location
    }

    pub fn set_start_from_pos(&mut self, start: &pest::Position) {
        let (line, column) = start.line_col();
        self.absolute_start = start.pos();
        self.start.line = line;
        self.start.column = column;
    }

    pub fn set_end_from_pos(&mut self, end: &pest::Position) {
        let (line, column) = end.line_col();
        self.absolute_end = end.pos();
        self.end.line = line;
        self.end.column = column - 1;
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

    // TODO(nlopes): shift_start should be shift_inline_start
    pub fn shift_start(&mut self, parent: Option<&Location>) {
        if let Some(parent) = parent {
            if parent.start.line == 0 {
                return;
            }
            self.absolute_start += parent.absolute_start;
            self.start.line += parent.start.line - 1;
        }
    }

    // TODO(nlopes): shift_end should be shift_inline_end
    pub fn shift_end(&mut self, parent: Option<&Location>) {
        if let Some(parent) = parent {
            if parent.start.line == 0 {
                return;
            }
            self.absolute_end += parent.absolute_start;
            self.end.line += parent.start.line - 1;
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

/// A `Position` represents a position in a document.
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// The line number of the position.
    pub line: usize,
    /// The column number of the position.
    #[serde(rename = "col")]
    pub column: usize,
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line: {}, column: {}", self.line, self.column)
    }
}
