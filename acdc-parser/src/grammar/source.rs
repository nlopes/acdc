//! Stable, line-aligned input windows with document-wide coordinates.

use std::{
    cell::{Cell, OnceCell, RefCell},
    ops::Range,
    rc::Rc,
};

use bumpalo::{Bump, collections::String as BumpString};

use crate::{Position, model::SourceRange};

use super::{
    LineMap,
    utf8_utils::{self, RoundDirection},
};

#[derive(Debug)]
struct Chunk<'a> {
    start: usize,
    text: &'a str,
    first_line: usize,
    line_map: OnceCell<Rc<LineMap>>,
}

#[derive(Debug)]
struct WindowText<'a> {
    start: usize,
    text: &'a str,
}

/// The reader appends complete lines. Previously emitted text never changes;
/// a window spanning several chunks is joined once in the document arena.
#[derive(Debug, Default)]
pub(super) struct SourceIndex<'a> {
    length: Cell<usize>,
    chunks: RefCell<Vec<Chunk<'a>>>,
    windows: RefCell<Vec<WindowText<'a>>>,
}

#[derive(Clone, Debug)]
pub(super) struct WindowOrigin<'a> {
    pub(super) base: usize,
    pub(super) first_line: usize,
    pub(super) index: Rc<SourceIndex<'a>>,
}

/// An owned location context can outlive advancement of the active window.
/// Ordinary and inline inputs retain a local map with no origin or index.
#[derive(Clone, Debug)]
pub(super) struct SourceView<'a> {
    pub(super) input: &'a str,
    pub(super) line_map: Rc<LineMap>,
    pub(super) origin: Option<Rc<WindowOrigin<'a>>>,
}

impl<'a> SourceIndex<'a> {
    pub(super) fn append(&self, text: &'a str) -> usize {
        let start = self.length.get();
        if text.is_empty() {
            return start;
        }
        let mut chunks = self.chunks.borrow_mut();
        let first_line = chunks.last().map_or(1, |chunk| {
            debug_assert!(
                chunk.text.ends_with('\n'),
                "source chunks end at line boundaries"
            );
            chunk.first_line + chunk.text.bytes().filter(|byte| *byte == b'\n').count()
        });
        chunks.push(Chunk {
            start,
            text,
            first_line,
            line_map: OnceCell::new(),
        });
        self.length.set(start + text.len());
        start
    }

    pub(super) fn seal(&self, length: usize) {
        debug_assert!(length <= self.length.get());
        self.length.set(length);
    }

    pub(super) fn len(&self) -> usize {
        self.length.get()
    }

    fn remember_window(&self, start: usize, text: &'a str) {
        let mut windows = self.windows.borrow_mut();
        if let Some(last) = windows.last() {
            debug_assert!(
                start >= last.start,
                "source windows advance in source order"
            );
            if start + text.len() <= last.start + last.text.len() {
                return;
            }
        }
        windows.push(WindowText { start, text });
    }

    pub(super) fn window(
        self: &Rc<Self>,
        range: Range<usize>,
        arena: &'a Bump,
    ) -> Option<SourceView<'a>> {
        if range.start > range.end || range.end > self.len() {
            return None;
        }
        {
            let chunks = self.chunks.borrow();
            let i = chunks
                .partition_point(|chunk| chunk.start <= range.start)
                .checked_sub(1);
            if let Some(chunk) = i.and_then(|i| chunks.get(i))
                && range.end <= chunk.start + chunk.text.len()
            {
                chunk
                    .text
                    .get(range.start - chunk.start..range.end - chunk.start)?;
                let input = chunk
                    .text
                    .get(..chunk.text.len().min(self.len() - chunk.start))?;
                let line_map = if input.len() == chunk.text.len() {
                    Rc::clone(chunk.line_map.get_or_init(|| Rc::new(LineMap::new(input))))
                } else {
                    Rc::new(LineMap::new(input))
                };
                return Some(SourceView {
                    input,
                    line_map,
                    origin: Some(Rc::new(WindowOrigin {
                        base: chunk.start,
                        first_line: chunk.first_line,
                        index: Rc::clone(self),
                    })),
                });
            }
        }
        if let Some((start, input)) = self.window_text(&range) {
            return Some(SourceView {
                input,
                line_map: Rc::new(LineMap::new(input)),
                origin: Some(Rc::new(WindowOrigin {
                    base: start,
                    first_line: self.position(start).line as usize,
                    index: Rc::clone(self),
                })),
            });
        }
        let start = self.line_start(range.start)?;
        let mut joined = BumpString::new_in(arena);
        for chunk in self.chunks.borrow().iter() {
            let left = start.max(chunk.start);
            let right = range.end.min(chunk.start + chunk.text.len());
            if left < right {
                joined.push_str(chunk.text.get(left - chunk.start..right - chunk.start)?);
            }
            if chunk.start >= range.end {
                break;
            }
        }
        if joined.len() != range.end - start {
            return None;
        }
        let input = joined.into_bump_str();
        self.remember_window(start, input);
        Some(SourceView {
            input,
            line_map: Rc::new(LineMap::new(input)),
            origin: Some(Rc::new(WindowOrigin {
                base: start,
                first_line: self.position(start).line as usize,
                index: Rc::clone(self),
            })),
        })
    }

    fn window_text(&self, range: &Range<usize>) -> Option<(usize, &'a str)> {
        let windows = self.windows.borrow();
        let i = windows
            .partition_point(|window| window.start <= range.start)
            .checked_sub(1)?;
        let window = windows.get(i)?;
        window
            .text
            .get(range.start - window.start..range.end.checked_sub(window.start)?)?;
        Some((window.start, window.text))
    }

    fn line_start(&self, offset: usize) -> Option<usize> {
        if offset == 0 {
            return Some(0);
        }
        let chunks = self.chunks.borrow();
        let i = chunks
            .partition_point(|chunk| chunk.start <= offset)
            .checked_sub(1)?;
        let chunk = chunks.get(i)?;
        let prefix = chunk.text.get(..offset - chunk.start)?;
        Some(chunk.start + prefix.rfind('\n').map_or(0, |offset| offset + 1))
    }

    fn get(&self, range: Range<usize>) -> Option<&'a str> {
        if range.start > range.end || range.end > self.len() {
            return None;
        }
        if let Some((start, text)) = self.window_text(&range) {
            return text.get(range.start - start..range.end - start);
        }
        let chunks = self.chunks.borrow();
        let i = chunks
            .partition_point(|chunk| chunk.start <= range.start)
            .checked_sub(1)?;
        let chunk = chunks.get(i)?;
        chunk
            .text
            .get(range.start - chunk.start..range.end.checked_sub(chunk.start)?)
    }

    pub(super) fn tail(&self, offset: usize) -> Option<&'a str> {
        if offset > self.len() {
            return None;
        }
        let chunks = self.chunks.borrow();
        let i = chunks
            .partition_point(|chunk| chunk.start <= offset)
            .checked_sub(1)?;
        let chunk = chunks.get(i)?;
        chunk
            .text
            .get(offset - chunk.start..(self.len() - chunk.start).min(chunk.text.len()))
    }

    fn byte(&self, offset: usize) -> Option<u8> {
        if offset >= self.len() {
            return None;
        }
        let chunks = self.chunks.borrow();
        let i = chunks
            .partition_point(|chunk| chunk.start <= offset)
            .checked_sub(1)?;
        let chunk = chunks.get(i)?;
        chunk.text.as_bytes().get(offset - chunk.start).copied()
    }

    fn snap(&self, offset: usize, direction: RoundDirection) -> usize {
        let offset = offset.min(self.len());
        let chunks = self.chunks.borrow();
        let Some(chunk) = chunks
            .partition_point(|chunk| chunk.start <= offset)
            .checked_sub(1)
            .and_then(|i| chunks.get(i))
        else {
            return 0;
        };
        chunk.start + utf8_utils::snap_to_boundary(chunk.text, offset - chunk.start, direction)
    }

    fn position(&self, offset: usize) -> Position {
        let offset = self.snap(offset, RoundDirection::Backward);
        let chunks = self.chunks.borrow();
        let Some(chunk) = chunks
            .partition_point(|chunk| chunk.start <= offset)
            .checked_sub(1)
            .and_then(|i| chunks.get(i))
        else {
            return Position::new(1, 1);
        };
        if let Some(map) = chunk.line_map.get() {
            let mut position = map.offset_to_position(offset - chunk.start, chunk.text);
            position.line = position
                .line
                .saturating_add(u32::try_from(chunk.first_line - 1).unwrap_or(u32::MAX));
            return position;
        }
        let prefix = chunk.text.get(..offset - chunk.start).unwrap_or_default();
        let line = chunk.first_line + prefix.bytes().filter(|byte| *byte == b'\n').count();
        let column = prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count()
            + 1;
        Position::new(
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(column).unwrap_or(u32::MAX),
        )
    }
}

#[cfg(test)]
mod tests;

impl<'a> SourceView<'a> {
    pub(super) fn get(&self, range: Range<usize>) -> Option<&'a str> {
        slice(self.input, self.origin.as_deref(), range)
    }
    pub(super) fn position(&self, offset: usize) -> Position {
        position(self.input, &self.line_map, self.origin.as_deref(), offset)
    }
    pub(super) fn snap(&self, offset: usize, direction: RoundDirection) -> usize {
        snap(self.input, self.origin.as_deref(), offset, direction)
    }
    pub(super) fn step(&self, offset: usize, direction: RoundDirection) -> usize {
        step(self.input, self.origin.as_deref(), offset, direction)
    }

    pub(super) fn previous_char(&self, offset: usize) -> Option<char> {
        let end = self.snap(offset, RoundDirection::Backward);
        let start = self.step(end, RoundDirection::Backward);
        self.get(start..end)?.chars().next_back()
    }
}

#[inline]
pub(super) fn len(input: &str, origin: Option<&WindowOrigin<'_>>) -> usize {
    origin.map_or(input.len(), |origin| origin.index.len())
}

#[inline]
pub(super) fn slice<'a>(
    input: &'a str,
    origin: Option<&WindowOrigin<'a>>,
    range: Range<usize>,
) -> Option<&'a str> {
    let Some(origin) = origin else {
        return input.get(range);
    };
    if range.start > range.end || range.end > origin.index.len() {
        return None;
    }
    if range.start >= origin.base && range.end <= origin.base + input.len() {
        input.get(range.start - origin.base..range.end - origin.base)
    } else {
        origin.index.get(range)
    }
}

#[inline]
pub(super) fn tail<'a>(
    input: &'a str,
    origin: Option<&WindowOrigin<'a>>,
    offset: usize,
) -> Option<&'a str> {
    let Some(origin) = origin else {
        return input.get(offset..);
    };
    if offset > origin.index.len() {
        return None;
    }
    if offset >= origin.base && offset <= origin.base + input.len() {
        input.get(offset - origin.base..(origin.index.len() - origin.base).min(input.len()))
    } else {
        origin.index.tail(offset)
    }
}

#[inline]
pub(super) fn byte(input: &str, origin: Option<&WindowOrigin<'_>>, offset: usize) -> Option<u8> {
    let Some(origin) = origin else {
        return input.as_bytes().get(offset).copied();
    };
    if offset >= origin.index.len() {
        return None;
    }
    if offset >= origin.base && offset < origin.base + input.len() {
        input.as_bytes().get(offset - origin.base).copied()
    } else {
        origin.index.byte(offset)
    }
}

#[inline]
pub(super) fn snap(
    input: &str,
    origin: Option<&WindowOrigin<'_>>,
    offset: usize,
    direction: RoundDirection,
) -> usize {
    let Some(origin) = origin else {
        return utf8_utils::snap_to_boundary(input, offset, direction);
    };
    let offset = offset.min(origin.index.len());
    if offset >= origin.base && offset <= origin.base + input.len() {
        origin.base + utf8_utils::snap_to_boundary(input, offset - origin.base, direction)
    } else {
        origin.index.snap(offset, direction)
    }
}

#[inline]
pub(super) fn step(
    input: &str,
    origin: Option<&WindowOrigin<'_>>,
    offset: usize,
    direction: RoundDirection,
) -> usize {
    match direction {
        RoundDirection::Backward => snap(input, origin, offset.saturating_sub(1), direction),
        RoundDirection::Forward if offset >= len(input, origin) => len(input, origin),
        RoundDirection::Forward => snap(input, origin, offset + 1, direction),
    }
}

#[inline]
pub(super) fn position(
    input: &str,
    map: &LineMap,
    origin: Option<&WindowOrigin<'_>>,
    offset: usize,
) -> Position {
    let Some(origin) = origin else {
        return map.offset_to_position(offset, input);
    };
    let offset = offset.min(origin.index.len());
    if offset >= origin.base && offset <= origin.base + input.len() {
        let mut position = map.offset_to_position(offset - origin.base, input);
        position.line = position
            .line
            .saturating_add(u32::try_from(origin.first_line - 1).unwrap_or(u32::MAX));
        position
    } else {
        origin.index.position(offset)
    }
}

#[inline]
pub(super) fn source_line(
    input: &str,
    map: &LineMap,
    origin: Option<&WindowOrigin<'_>>,
    range: &SourceRange,
    offset: usize,
) -> u32 {
    let Some(origin) = origin else {
        return map.source_line(range, offset);
    };
    let start_line = origin.index.position(range.start_offset).line;
    u32::try_from(range.start_line)
        .unwrap_or(u32::MAX)
        .saturating_add(
            position(input, map, Some(origin), offset)
                .line
                .saturating_sub(start_line),
        )
}
