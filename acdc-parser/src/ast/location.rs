use crate::{Location, Position};

/// A source span with inclusive endpoints.
///
/// Positions use the original source coordinates. Resolve the original
/// file through [`crate::ParseResult::resolve_location`] for a parsed node.
#[derive(Clone, Copy, Debug)]
pub struct LocationRef<'doc> {
    location: &'doc Location,
}

impl<'doc> LocationRef<'doc> {
    pub(super) fn new(location: &'doc Location) -> Self {
        Self { location }
    }

    pub(crate) fn as_location(self) -> &'doc Location {
        self.location
    }

    /// The start position, including its include chain.
    #[must_use]
    pub fn start(self) -> PositionRef<'doc> {
        PositionRef::new(&self.location.start)
    }

    /// The inclusive end position, including its include chain.
    #[must_use]
    pub fn end(self) -> PositionRef<'doc> {
        PositionRef::new(&self.location.end)
    }

    /// The start byte offset, normally in the original source file.
    ///
    /// Reindented includes can retain offsets in transformed text. Do not treat
    /// these offsets as indices into [`crate::ParseResult::source`].
    #[must_use]
    pub fn absolute_start(self) -> usize {
        self.location.absolute_start
    }

    /// The inclusive end byte offset, with the same rules as the start.
    #[must_use]
    pub fn absolute_end(self) -> usize {
        self.location.absolute_end
    }
}

/// A position in its original source context.
#[derive(Clone, Copy, Debug)]
pub struct PositionRef<'doc> {
    position: &'doc Position,
}

impl<'doc> PositionRef<'doc> {
    pub(super) fn new(position: &'doc Position) -> Self {
        Self { position }
    }

    /// The one-based line number.
    #[must_use]
    pub fn line(self) -> u32 {
        self.position.line
    }

    /// The one-based column, counted in Unicode scalar values.
    #[must_use]
    pub fn column(self) -> u32 {
        self.position.column
    }

    /// Include targets as written, outermost first; absent for the primary input.
    #[must_use]
    pub fn include_chain(self) -> Option<impl Iterator<Item = &'doc str> + Send + Sync + 'doc> {
        self.position
            .file
            .as_ref()
            .map(|chain| chain.iter().map(String::as_str))
    }
}

impl<'doc> From<&'doc Location> for LocationRef<'doc> {
    fn from(location: &'doc Location) -> Self {
        Self::new(location)
    }
}

impl<'doc> From<&'doc Position> for PositionRef<'doc> {
    fn from(position: &'doc Position) -> Self {
        Self::new(position)
    }
}

impl From<LocationRef<'_>> for Location {
    /// Copy the source span, including both endpoints' include chains.
    fn from(location: LocationRef<'_>) -> Self {
        location.location.clone()
    }
}
