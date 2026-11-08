//! Shared source text with indexed position and snippet lookup.

use acdc_parser::SourceLocation;
use miette::{MietteError, MietteSpanContents, SourceCode, SourceSpan, SpanContents};

#[derive(Debug)]
pub(super) struct IndexedSource {
    text: String,
    index: SourceIndex,
}

impl IndexedSource {
    pub(super) fn new(text: String) -> Self {
        let index = SourceIndex::new(&text);
        Self { text, index }
    }

    #[cfg(test)]
    pub(super) fn as_str(&self) -> &str {
        &self.text
    }

    pub(super) fn span(&self, location: &SourceLocation) -> SourceSpan {
        self.index.span(location, &self.text)
    }
}

#[derive(Debug)]
pub(super) struct SourceIndex {
    /// Parser positions count LF-delimited lines and Unicode scalar columns.
    line_starts: Vec<usize>,
    /// Miette also recognizes bare CR. Most sources share the parser's index.
    snippet_line_starts: Option<Vec<usize>>,
}

impl SourceIndex {
    pub(super) fn new(source: &str) -> Self {
        let bytes = source.as_bytes();
        let mut line_starts = vec![0];
        let mut bare_cr = false;
        for (offset, &byte) in bytes.iter().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            } else if byte == b'\r' && bytes.get(offset + 1) != Some(&b'\n') {
                bare_cr = true;
            }
        }
        let snippet_line_starts = bare_cr.then(|| {
            let mut starts = vec![0];
            for (offset, &byte) in bytes.iter().enumerate() {
                if byte == b'\n' || (byte == b'\r' && bytes.get(offset + 1) != Some(&b'\n')) {
                    starts.push(offset + 1);
                }
            }
            starts
        });
        Self {
            line_starts,
            snippet_line_starts,
        }
    }

    pub(super) fn offset_from_position(&self, source: &str, line: u32, column: u32) -> usize {
        let Some(line_index) = line.checked_sub(1).map(|line| line as usize) else {
            return source.len();
        };
        let Some(&start) = self.line_starts.get(line_index) else {
            return source.len();
        };
        let end = self
            .line_starts
            .get(line_index + 1)
            .copied()
            .unwrap_or(source.len());
        let text = source.get(start..end).unwrap_or_default();
        let column = column.saturating_sub(1) as usize;
        let end = text.strip_suffix('\n').map_or(text.len(), str::len);
        start
            + text
                .char_indices()
                .nth(column)
                .map_or(end, |(offset, _)| offset.min(end))
    }

    pub(super) fn span(&self, location: &SourceLocation, source: &str) -> SourceSpan {
        let location = &location.location;
        let start = self.offset_from_position(source, location.start.line, location.start.column);
        let end = if location.start.file == location.end.file {
            self.offset_from_position(source, location.end.line, location.end.column)
                .max(start)
        } else {
            start
        };
        let end_exclusive = source
            .get(end..)
            .and_then(|remainder| remainder.chars().next())
            .map_or(end, |character| end + character.len_utf8());
        (start, end_exclusive.saturating_sub(start)).into()
    }
}

impl SourceCode for IndexedSource {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        span.offset()
            .checked_add(span.len())
            .filter(|&end| end <= self.text.len())
            .ok_or(MietteError::OutOfBounds)?;
        let starts = self
            .index
            .snippet_line_starts
            .as_deref()
            .unwrap_or(&self.index.line_starts);
        let line = starts
            .partition_point(|&start| start <= span.offset())
            .saturating_sub(1);
        // Include an extra preceding line so zero-length spans at a newline
        // retain miette's boundary behavior. Its scanner now visits only the
        // nearby context instead of every byte before the diagnostic.
        let first_line = line.saturating_sub(context_lines_before.saturating_add(1));
        let start = starts.get(first_line).copied().unwrap_or(0);
        let source = self.text.as_bytes().get(start..).unwrap_or_default();
        let relative = (span.offset().saturating_sub(start), span.len()).into();
        let contents = source.read_span(&relative, context_lines_before, context_lines_after)?;
        Ok(Box::new(MietteSpanContents::new(
            contents.data(),
            (contents.span().offset() + start, contents.span().len()).into(),
            contents.line() + first_line,
            contents.column(),
            contents.line_count() + first_line,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_match_miette_at_line_and_span_boundaries() {
        for text in [
            "",
            "one",
            "one\n",
            "one\ntwo\nthree\nfour\nfive\nsix\n",
            "\n\n\n\n\n\n",
            "éx\n日本語\n🙂last",
            "a\r\nb\r\nc\r\nd\r\n",
            "a\rb\rc\rd\r",
            "a\r\r\nb\n\rc\r\nd",
        ] {
            let source = IndexedSource::new(text.into());
            for offset in 0..=text.len() {
                for length in 0..=3.min(text.len() - offset) {
                    for before in 0..=3 {
                        for after in 0..=3 {
                            let span = (offset, length).into();
                            let expected = text.read_span(&span, before, after);
                            let actual = source.read_span(&span, before, after);
                            match (actual, expected) {
                                (Ok(actual), Ok(expected)) => {
                                    assert_eq!(
                                        (
                                            actual.data(),
                                            actual.span(),
                                            actual.line(),
                                            actual.column(),
                                            actual.line_count()
                                        ),
                                        (
                                            expected.data(),
                                            expected.span(),
                                            expected.line(),
                                            expected.column(),
                                            expected.line_count()
                                        ),
                                        "text={text:?} span={span:?} context={before}/{after}",
                                    );
                                }
                                (Err(_), Err(_)) => {}
                                (actual, expected) => {
                                    assert_eq!(
                                        actual.is_ok(),
                                        expected.is_ok(),
                                        "text={text:?} span={span:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_spans_are_reported_without_panicking() {
        let source = IndexedSource::new("source".into());
        for span in [(7, 0), (0, 7), (usize::MAX, 1), (1, usize::MAX)] {
            assert!(source.read_span(&span.into(), 1, 1).is_err());
        }
    }
}
