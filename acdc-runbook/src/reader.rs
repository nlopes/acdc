use std::{
    io::{self, BufRead, Read},
    iter::FusedIterator,
    time::Duration,
};

use asciicast_rs::{Streamable, V3, v3};

use crate::MAX_LINE_BYTES;

/// A bounded v3 read failed. Events yielded before the error remain usable.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("recording read failed: {0}")]
    Io(#[from] io::Error),
    #[error("invalid recording header: {0}")]
    Header(#[source] asciicast_rs::Error),
    #[error("recording line {line} exceeds {MAX_LINE_BYTES} bytes")]
    LineTooLong { line: u64 },
    #[error("invalid recording event on line {line}: {source}")]
    Event {
        line: u64,
        #[source]
        source: asciicast_rs::Error,
    },
    #[error("unterminated recording event on line {line}: {source}")]
    Truncated {
        line: u64,
        #[source]
        source: asciicast_rs::Error,
    },
    #[error("recording time is outside the supported duration range on line {line}")]
    TimeRange { line: u64 },
}

/// An event at its raw elapsed time. Input and unknown events remain inert payloads.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedEvent {
    pub at: Duration,
    pub payload: v3::EventPayload,
}

/// A streaming reader for plain v3 recordings, with bounded line storage.
///
/// Uses asciicast header types and event parsing. No event is executed.
/// The first read or parse error ends iteration; a partial final line cannot erase
/// previously yielded events. A valid final JSON line needs no trailing newline.
pub struct Reader<R> {
    input: R,
    header: v3::Header,
    buffer: Vec<u8>,
    line: u64,
    elapsed: f64,
    ended: bool,
}

impl<R: BufRead> Reader<R> {
    /// Read a bounded v3 header. Compressed input must be decoded by the caller.
    ///
    /// # Errors
    /// Returns an error for failed input, an oversized header, or an invalid v3 header.
    pub fn new(mut input: R) -> Result<Self, ReadError> {
        let mut buffer = Vec::new();
        read_line(&mut input, &mut buffer, 1)?;
        let header: v3::Header =
            serde_json::from_slice(&buffer).map_err(|error| ReadError::Header(error.into()))?;
        if header.version != 3 {
            return Err(ReadError::Header(asciicast_rs::Error::VersionMismatch {
                expected: 3,
                found: header.version,
            }));
        }
        if header.term.cols == 0 || header.term.rows == 0 {
            return Err(ReadError::Header(asciicast_rs::Error::MalformedEvent(
                "zero terminal dimensions".into(),
            )));
        }
        buffer.clear();
        Ok(Self {
            input,
            header,
            buffer,
            line: 1,
            elapsed: 0.0,
            ended: false,
        })
    }

    /// Initial terminal information and optional recording metadata.
    #[must_use]
    pub const fn header(&self) -> &v3::Header {
        &self.header
    }

    fn read_event(&mut self) -> Result<Option<TimedEvent>, ReadError> {
        loop {
            self.line += 1;
            if read_line(&mut self.input, &mut self.buffer, self.line)? == 0 {
                return Ok(None);
            }
            if self.buffer.iter().all(u8::is_ascii_whitespace) || self.buffer.starts_with(b"#") {
                continue;
            }
            let terminated = self.buffer.ends_with(b"\n");
            let line = std::str::from_utf8(&self.buffer).map_err(|error| {
                ReadError::Io(io::Error::new(io::ErrorKind::InvalidData, error))
            })?;
            let event = V3::parse_event(line).map_err(|source| {
                if terminated {
                    ReadError::Event {
                        line: self.line,
                        source,
                    }
                } else {
                    ReadError::Truncated {
                        line: self.line,
                        source,
                    }
                }
            })?;
            self.elapsed += event.interval;
            let at = Duration::try_from_secs_f64(self.elapsed)
                .map_err(|_| ReadError::TimeRange { line: self.line })?;
            return Ok(Some(TimedEvent {
                at,
                payload: event.payload,
            }));
        }
    }
}

impl<R: BufRead> Iterator for Reader<R> {
    type Item = Result<TimedEvent, ReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        match self.read_event() {
            Ok(Some(event)) => Some(Ok(event)),
            Ok(None) => {
                self.ended = true;
                None
            }
            Err(error) => {
                self.ended = true;
                Some(Err(error))
            }
        }
    }
}

impl<R: BufRead> FusedIterator for Reader<R> {}

fn read_line(
    input: &mut impl BufRead,
    buffer: &mut Vec<u8>,
    line: u64,
) -> Result<usize, ReadError> {
    buffer.clear();
    let read = input
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_until(b'\n', buffer)?;
    if read > MAX_LINE_BYTES {
        return Err(ReadError::LineTooLong { line });
    }
    Ok(read)
}
