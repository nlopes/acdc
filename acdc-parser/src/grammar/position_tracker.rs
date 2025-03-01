use crate::{Location, Position};

/// A `PositionTracker` is used to track the position of a parser.
#[derive(Clone, Debug, PartialEq, Copy)]
pub(crate) struct PositionTracker {
    line: usize,
    column: usize,
    offset: usize,
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionTracker {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }

    pub(crate) fn set_initial_position(&mut self, location: &Location, absolute_offset: usize) {
        self.line = location.start.line;
        self.column = location.start.column;
        self.offset = absolute_offset;
    }

    pub(crate) fn get_position(&self) -> Position {
        Position {
            line: self.line,
            column: self.column,
        }
    }

    pub(crate) fn get_offset(&self) -> usize {
        self.offset
    }

    // TODO(nlopes): check if `#[inline(always)]` will help
    pub(crate) fn advance(&mut self, s: &str) {
        for c in s.chars() {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.offset += s.len();
    }

    pub(crate) fn advance_by(&mut self, n: usize) {
        self.column += n;
        self.offset += n;
    }

    pub(crate) fn calculate_location(
        &mut self,
        start: Position,
        content: &str,
        padding: usize,
    ) -> Location {
        let absolute_start = self.get_offset();
        self.advance(content);
        self.advance_by(padding);
        let absolute_end = self.get_offset();
        let end = self.get_position();
        Location {
            absolute_start,
            absolute_end,
            start,
            end,
        }
    }

    pub(crate) fn calculate_location_from_start_end(
        &mut self,
        start: Position,
        absolute_end: usize,
    ) -> Location {
        let absolute_start = self.get_offset();
        self.advance_by(absolute_end - absolute_start);
        Location {
            absolute_start,
            absolute_end,
            start,
            end: self.get_position(),
        }
    }
}
