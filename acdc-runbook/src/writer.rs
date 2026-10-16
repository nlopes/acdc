use std::{
    io::{self, Write},
    time::Duration,
};

use serde::Serialize;

use crate::{
    Capture, EVENT_CODE, Event, MAX_LINE_BYTES, RunResult, SCHEMA_VERSION, model::Envelope,
};

/// A recording could not be written. An I/O failure makes its writer unusable.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("recording write failed: {0}")]
    Io(#[from] io::Error),
    #[error("recording encoding failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("terminal dimensions must be nonzero")]
    InvalidSize,
    #[error("recording time moved backward from {previous:?} to {next:?}")]
    TimeOrder { previous: Duration, next: Duration },
    #[error("recording line exceeds {MAX_LINE_BYTES} bytes")]
    LineTooLong,
    #[error("recording writer had an earlier I/O failure")]
    Failed,
}

/// A plain asciicast v3 writer. It records no input or environment variables.
///
/// Supply absolute elapsed times from one monotonic clock. Calls are serialized
/// through `&mut self`; equal-time events retain call order. Dropping the writer
/// does not synthesize a successful finish. The caller owns destination creation
/// and must refuse to overwrite an existing recording.
pub struct Writer<W> {
    output: W,
    previous: Duration,
    failed: bool,
}

impl<W: Write> Writer<W> {
    /// Write a minimal v3 header immediately, without reading the environment.
    ///
    /// # Errors
    /// Returns an error for zero dimensions, an oversized header, or failed output.
    pub fn new(output: W, cols: u16, rows: u16, title: Option<&str>) -> Result<Self, WriteError> {
        #[derive(Serialize)]
        struct Term {
            cols: u16,
            rows: u16,
        }
        #[derive(Serialize)]
        struct Header<'a> {
            version: u8,
            term: Term,
            #[serde(skip_serializing_if = "Option::is_none")]
            title: Option<&'a str>,
        }
        check_size(cols, rows)?;
        let mut writer = Self {
            output,
            previous: Duration::ZERO,
            failed: false,
        };
        writer.line(&Header {
            version: 3,
            term: Term { cols, rows },
            title,
        })?;
        Ok(writer)
    }

    /// Write UTF-8 terminal output. Preserve split UTF-8 bytes in the capture layer
    /// until a complete string is available; do not decode each PTY read separately.
    ///
    /// # Errors
    /// Returns an error for backward time, an oversized line, or failed output.
    pub fn output(&mut self, at: Duration, data: &str) -> Result<(), WriteError> {
        self.event(at, "o", data)
    }

    /// Write a terminal resize on the same clock as output and metadata.
    ///
    /// # Errors
    /// Returns an error for zero dimensions or a failed event write.
    pub fn resize(&mut self, at: Duration, cols: u16, rows: u16) -> Result<(), WriteError> {
        check_size(cols, rows)?;
        self.event(at, "r", &format!("{cols}x{rows}"))
    }

    /// Write a human-readable navigation label, separate from machine metadata.
    ///
    /// # Errors
    /// Returns an error for backward time, an oversized line, or failed output.
    pub fn marker(&mut self, at: Duration, label: &str) -> Result<(), WriteError> {
        self.event(at, "m", label)
    }

    /// Write versioned lifecycle metadata. The caller supplies events in execution order.
    ///
    /// # Errors
    /// Returns an error for backward time, an oversized line, or failed output.
    pub fn metadata(&mut self, at: Duration, event: &Event) -> Result<(), WriteError> {
        self.check_ready()?;
        let data = encode(&Envelope {
            version: SCHEMA_VERSION,
            event,
        })?;
        let data = std::str::from_utf8(&data).map_err(io::Error::other)?;
        self.event(at, EVENT_CODE, data)
    }

    /// Flush a step boundary. On failure no more events can be written.
    ///
    /// # Errors
    /// Returns the original flush error, or [`WriteError::Failed`] after an earlier failure.
    pub fn flush(&mut self) -> Result<(), WriteError> {
        self.check_ready()?;
        if let Err(error) = self.output.flush() {
            self.failed = true;
            return Err(error.into());
        }
        Ok(())
    }

    /// Write run completion and a final standard exit event, then flush.
    /// The aggregate exit is zero only for successful execution and complete capture.
    /// A write failure leaves the existing prefix and does not attempt a repair.
    ///
    /// # Errors
    /// Returns an error if either final event or the flush fails.
    pub fn finish(
        mut self,
        at: Duration,
        result: RunResult,
        capture: Capture,
    ) -> Result<W, WriteError> {
        let success = result == RunResult::Succeeded && capture == Capture::Complete;
        self.metadata(at, &Event::RunFinished { result, capture })?;
        self.event(at, "x", if success { "0" } else { "1" })?;
        self.flush()?;
        Ok(self.output)
    }

    fn event(&mut self, at: Duration, code: &str, data: &str) -> Result<(), WriteError> {
        self.check_ready()?;
        let interval = at.checked_sub(self.previous).ok_or(WriteError::TimeOrder {
            previous: self.previous,
            next: at,
        })?;
        self.line(&(interval.as_secs_f64(), code, data))?;
        self.previous = at;
        Ok(())
    }

    fn line(&mut self, value: &impl Serialize) -> Result<(), WriteError> {
        let mut line = encode(value)?;
        line.push(b'\n');
        if let Err(error) = self.output.write_all(&line) {
            self.failed = true;
            return Err(error.into());
        }
        Ok(())
    }

    fn check_ready(&self) -> Result<(), WriteError> {
        if self.failed {
            Err(WriteError::Failed)
        } else {
            Ok(())
        }
    }
}

fn check_size(cols: u16, rows: u16) -> Result<(), WriteError> {
    if cols == 0 || rows == 0 {
        Err(WriteError::InvalidSize)
    } else {
        Ok(())
    }
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>, WriteError> {
    struct Limited(Vec<u8>);
    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() >= MAX_LINE_BYTES.saturating_sub(self.0.len()) {
                return Err(io::Error::other("recording line limit reached"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Limited(Vec::new());
    serde_json::to_writer(&mut buffer, value).map_err(|error| {
        if error.is_io() {
            WriteError::LineTooLong
        } else {
            WriteError::Json(error)
        }
    })?;
    Ok(buffer.0)
}
