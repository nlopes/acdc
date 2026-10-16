use std::{
    cell::RefCell,
    io::{self, Cursor, Write},
    rc::Rc,
    time::Duration,
};

use acdc_runbook::{
    Capture, Decoded, EVENT_CODE, Event, FailureStage, MAX_LINE_BYTES, Outcome, Position,
    ReadError, Reader, RunResult, SkipReason, Source, Step, Termination, WriteError, Writer,
    decode,
};
use asciicast_rs::v3;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn steps() -> Vec<Step> {
    vec![
        Step {
            occurrence: 0,
            id: "build".into(),
            description: Some("Build \"tools\"\n<script>\u{202e}".into()),
            section: Some("Setup".into()),
            interpreter: "tools/shell".into(),
            source: Source {
                path: Some("chapters/setup.adoc".into()),
                start: Position { line: 5, column: 1 },
                end: Position { line: 8, column: 5 },
            },
            dependencies: Vec::new(),
        },
        Step {
            occurrence: 1,
            id: "deploy".into(),
            description: None,
            section: None,
            interpreter: "sh".into(),
            source: Source {
                path: None,
                start: Position {
                    line: 10,
                    column: 1,
                },
                end: Position {
                    line: 13,
                    column: 5,
                },
            },
            dependencies: vec![0],
        },
    ]
}

#[test]
fn writer_round_trips_provenance_outcomes_and_one_clock() -> TestResult {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 80, 24, Some("Deploy\n\"tools\""))?;
    let start = Event::RunStarted { steps: steps() };
    writer.metadata(Duration::ZERO, &start)?;
    writer.metadata(Duration::ZERO, &Event::CommandStarted { occurrence: 0 })?;
    let output = "héllo\r\n\x1b]7501;id=task;st=working\x1b\\";
    writer.output(Duration::from_millis(125), output)?;
    writer.resize(Duration::from_millis(375), 120, 38)?;
    writer.marker(Duration::from_millis(375), "Build failed")?;
    let failed = Event::CommandFinished {
        occurrence: 0,
        outcome: Outcome::Failed {
            termination: Termination::Exited { code: 7 },
        },
    };
    writer.metadata(Duration::from_millis(600), &failed)?;
    let skipped = Event::CommandFinished {
        occurrence: 1,
        outcome: Outcome::Skipped {
            reason: SkipReason::DependencyFailed { occurrence: 0 },
        },
    };
    writer.metadata(Duration::from_millis(600), &skipped)?;
    writer.finish(
        Duration::from_millis(900),
        RunResult::Failed,
        Capture::Complete,
    )?;

    let reader = Reader::new(bytes.as_slice())?;
    assert_eq!(reader.header().term.cols, 80);
    assert_eq!(reader.header().title.as_deref(), Some("Deploy\n\"tools\""));
    assert!(reader.header().env.is_none());
    let events = reader.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        events
            .iter()
            .map(|event| event.at.as_millis())
            .collect::<Vec<_>>(),
        [0, 0, 125, 375, 375, 600, 600, 900, 900]
    );
    let metadata = events
        .iter()
        .filter_map(|event| match &event.payload {
            v3::EventPayload::Unknown { code, data } if code == EVENT_CODE => Some(decode(data)),
            v3::EventPayload::Output(_)
            | v3::EventPayload::Input(_)
            | v3::EventPayload::Marker(_)
            | v3::EventPayload::Resize(_)
            | v3::EventPayload::Exit(_)
            | v3::EventPayload::Unknown { .. }
            | _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        metadata,
        [
            Decoded::Event(start),
            Decoded::Event(Event::CommandStarted { occurrence: 0 }),
            Decoded::Event(failed),
            Decoded::Event(skipped),
            Decoded::Event(Event::RunFinished {
                result: RunResult::Failed,
                capture: Capture::Complete
            }),
        ]
    );
    assert!(
        events.iter().any(
            |event| matches!(&event.payload, v3::EventPayload::Output(data) if data == output)
        )
    );
    assert!(events.iter().any(|event| matches!(&event.payload, v3::EventPayload::Resize(size) if size.cols == 120 && size.rows == 38)));
    assert!(
        matches!(events.last().map(|event| &event.payload), Some(v3::EventPayload::Exit(status)) if status.code() == 1)
    );
    Ok(())
}

#[test]
fn all_process_and_skip_results_retain_their_meaning() -> TestResult {
    let outcomes = [
        Outcome::Succeeded,
        Outcome::Failed {
            termination: Termination::Exited { code: 0xc000_013a },
        },
        Outcome::Failed {
            termination: Termination::Signaled {
                signal: "SIGINT".into(),
            },
        },
        Outcome::Failed {
            termination: Termination::Unknown,
        },
        Outcome::Error {
            stage: FailureStage::Preparation,
            message: "disk full".into(),
        },
        Outcome::Error {
            stage: FailureStage::Process,
            message: "interpreter missing".into(),
        },
        Outcome::Skipped {
            reason: SkipReason::StoppedAfterFailure { occurrence: 3 },
        },
        Outcome::Skipped {
            reason: SkipReason::DependencyFailed { occurrence: 5 },
        },
    ];
    for outcome in outcomes {
        let mut bytes = Vec::new();
        let event = Event::CommandFinished {
            occurrence: 7,
            outcome,
        };
        {
            let mut writer = Writer::new(&mut bytes, 80, 24, None)?;
            writer.metadata(Duration::ZERO, &event)?;
        }
        let parsed = Reader::new(bytes.as_slice())?
            .next()
            .ok_or("missing outcome")??;
        let v3::EventPayload::Unknown { code, data } = parsed.payload else {
            return Err("metadata became visible output".into());
        };
        assert_eq!(code, EVENT_CODE);
        assert_eq!(decode(&data)?, Decoded::Event(event));
    }
    Ok(())
}

#[test]
fn unsupported_metadata_and_extra_fields_do_not_block_terminal_replay() -> TestResult {
    assert_eq!(
        decode(r#"{"version":2,"unrecognized":true}"#)?,
        Decoded::UnsupportedVersion(2)
    );
    assert_eq!(
        decode(r#"{"version":1,"event":"future","other":{}}"#)?,
        Decoded::UnsupportedEvent("future".into())
    );
    assert_eq!(
        decode(r#"{"version":1,"event":"command_started","occurrence":4,"future":true}"#)?,
        Decoded::Event(Event::CommandStarted { occurrence: 4 })
    );
    assert!(decode(r#"{"version":1,"event":"command_started"}"#).is_err());
    assert!(decode("not json").is_err());
    let input = concat!(
        "{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n",
        "[1,\"acdc.runbook\",\"{\\\"version\\\":99}\"]\n",
        "[2,\"o\",\"hello\"]\n",
    );
    let events = Reader::new(input.as_bytes())?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        events.last().map(|event| event.at),
        Some(Duration::from_secs(3))
    );
    assert!(
        matches!(events.last().map(|event| &event.payload), Some(v3::EventPayload::Output(text)) if text == "hello")
    );
    Ok(())
}

#[test]
fn truncated_final_metadata_preserves_the_complete_prefix_and_stops() -> TestResult {
    let input = concat!(
        "{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n",
        "[0,\"o\",\"first\"]\n",
        "[1,\"m\",\"second\"]\n",
        "[1,\"acdc.runbook\",\"{\\\"version\\\":1,",
    );
    let mut reader = Reader::new(input.as_bytes())?;
    assert!(
        matches!(reader.next().transpose()?.map(|event| event.payload), Some(v3::EventPayload::Output(text)) if text == "first")
    );
    assert!(
        matches!(reader.next().transpose()?.map(|event| event.payload), Some(v3::EventPayload::Marker(text)) if text == "second")
    );
    assert!(matches!(
        reader.next(),
        Some(Err(ReadError::Truncated { line: 4, .. }))
    ));
    assert!(reader.next().is_none());
    assert!(reader.next().is_none());
    Ok(())
}

#[test]
fn a_missing_finish_is_not_invented_and_a_valid_final_line_needs_no_newline() -> TestResult {
    let mut bytes = Vec::new();
    {
        let mut writer = Writer::new(&mut bytes, 80, 24, None)?;
        writer.metadata(Duration::ZERO, &Event::RunStarted { steps: steps() })?;
        writer.output(Duration::from_secs(1), "partial output")?;
    }
    assert_eq!(bytes.pop(), Some(b'\n'));
    let events = Reader::new(bytes.as_slice())?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(events.len(), 2);
    assert!(
        matches!(events.last().map(|event| &event.payload), Some(v3::EventPayload::Output(text)) if text == "partial output")
    );
    Ok(())
}

#[test]
fn malformed_middle_events_are_reported_instead_of_silently_skipped() -> TestResult {
    let input = concat!(
        "{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n",
        "[0,\"o\",\"first\"]\n",
        "not json\n",
        "[0,\"o\",\"must not appear\"]\n",
    );
    let mut reader = Reader::new(input.as_bytes())?;
    assert!(reader.next().transpose()?.is_some());
    assert!(matches!(
        reader.next(),
        Some(Err(ReadError::Event { line: 3, .. }))
    ));
    assert!(reader.next().is_none());
    Ok(())
}

#[test]
fn input_unknown_codes_and_comments_preserve_the_raw_clock() -> TestResult {
    let input = concat!(
        "{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n",
        "# comment\n\n",
        "[0.5,\"i\",\"do not execute\\r\"]\n",
        "[0.5,\"future\",\"metadata\"]\n",
        "[1,\"o\",\"last\"]\n",
        "[3,\"x\",\"7\"]\n",
    );
    let events = Reader::new(input.as_bytes())?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        events
            .iter()
            .map(|event| event.at.as_millis())
            .collect::<Vec<_>>(),
        [500, 1000, 2000, 5000]
    );
    assert!(
        matches!(events.first().map(|event| &event.payload), Some(v3::EventPayload::Input(text)) if text == "do not execute\r")
    );
    Ok(())
}

#[test]
fn invalid_times_and_sizes_are_rejected() -> TestResult {
    assert!(matches!(
        Writer::new(Vec::new(), 0, 24, None),
        Err(WriteError::InvalidSize)
    ));
    let mut writer = Writer::new(Vec::new(), 80, 24, None)?;
    writer.output(Duration::from_secs(2), "first")?;
    assert!(matches!(
        writer.output(Duration::from_secs(1), "backward"),
        Err(WriteError::TimeOrder { .. })
    ));
    assert!(matches!(
        writer.resize(Duration::from_secs(2), 80, 0),
        Err(WriteError::InvalidSize)
    ));
    for interval in ["-1", "1e300"] {
        let input = format!(
            "{{\"version\":3,\"term\":{{\"cols\":80,\"rows\":24}}}}\n[{interval},\"o\",\"x\"]\n"
        );
        assert!(
            Reader::new(input.as_bytes())?
                .next()
                .ok_or("missing event")?
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn line_limits_apply_before_output_and_before_unbounded_input_allocation() -> TestResult {
    let mut bytes = Vec::new();
    let huge = "x".repeat(MAX_LINE_BYTES);
    {
        let mut writer = Writer::new(&mut bytes, 80, 24, None)?;
        assert!(matches!(
            writer.output(Duration::from_secs(50), &huge),
            Err(WriteError::LineTooLong)
        ));
        writer.output(Duration::from_secs(1), "still usable")?;
    }
    let events = Reader::new(bytes.as_slice())?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(events.len(), 1);
    assert_eq!(
        events.first().map(|event| event.at),
        Some(Duration::from_secs(1))
    );

    let huge = "x".repeat(MAX_LINE_BYTES * 2);
    let mut cursor = Cursor::new(huge.as_bytes());
    assert!(matches!(
        Reader::new(&mut cursor),
        Err(ReadError::LineTooLong { line: 1 })
    ));
    assert_eq!(cursor.position(), MAX_LINE_BYTES as u64 + 1);
    let mut events_input =
        Vec::from(b"{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n".as_slice());
    events_input.extend_from_slice(huge.as_bytes());
    let mut reader = Reader::new(events_input.as_slice())?;
    assert!(matches!(
        reader.next(),
        Some(Err(ReadError::LineTooLong { line: 2 }))
    ));
    assert!(reader.next().is_none());
    Ok(())
}

#[test]
fn oversized_headers_and_double_encoded_metadata_do_not_change_output() -> TestResult {
    let mut bytes = Vec::new();
    let huge = "x".repeat(MAX_LINE_BYTES);
    assert!(matches!(
        Writer::new(&mut bytes, 80, 24, Some(&huge)),
        Err(WriteError::LineTooLong)
    ));
    assert_eq!(bytes.as_slice(), b"");
    {
        let mut writer = Writer::new(&mut bytes, 80, 24, None)?;
        let event = Event::RunFinished {
            result: RunResult::Aborted,
            capture: Capture::Incomplete {
                reason: "\"".repeat(MAX_LINE_BYTES / 3),
            },
        };
        assert!(matches!(
            writer.metadata(Duration::from_secs(50), &event),
            Err(WriteError::LineTooLong)
        ));
        writer.output(Duration::from_secs(1), "prefix intact")?;
    }
    let events = Reader::new(bytes.as_slice())?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(events.len(), 1);
    assert!(
        matches!(events.first().map(|event| &event.payload), Some(v3::EventPayload::Output(text)) if text == "prefix intact")
    );
    Ok(())
}

#[test]
fn submillisecond_intervals_do_not_accumulate_rounding_drift() -> TestResult {
    let mut bytes = Vec::new();
    {
        let mut writer = Writer::new(&mut bytes, 80, 24, None)?;
        for sequence in 1..=10_000 {
            writer.output(Duration::from_nanos(sequence * 333_333), "x")?;
        }
    }
    let last = v3::stream(bytes.as_slice())?
        .absolute_times()
        .last()
        .ok_or("missing output")??;
    assert!((last.0 - 3.33333).abs() < 1e-9);
    Ok(())
}

#[test]
fn fractional_nanosecond_intervals_accumulate_before_conversion() -> TestResult {
    let mut input = String::from("{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n");
    input.push_str(&"[0.0000000004,\"o\",\"x\"]\n".repeat(10));
    let last = Reader::new(input.as_bytes())?
        .last()
        .ok_or("missing event")??;
    assert_eq!(last.at, Duration::from_nanos(4));
    Ok(())
}

#[test]
fn bounded_header_reads_do_not_enable_implicit_decompression() {
    let compressed_empty_frame: &[u8] = &[0x28, 0xb5, 0x2f, 0xfd, 0x20, 0x00, 0x01, 0x00, 0x00];
    assert!(matches!(
        Reader::new(compressed_empty_frame),
        Err(ReadError::Header(asciicast_rs::Error::Json(_)))
    ));
}

#[test]
fn complete_execution_and_capture_health_independently_control_the_final_exit() -> TestResult {
    for (result, capture, expected) in [
        (RunResult::Succeeded, Capture::Complete, 0),
        (RunResult::Failed, Capture::Complete, 1),
        (RunResult::Aborted, Capture::Complete, 1),
        (
            RunResult::Succeeded,
            Capture::Incomplete {
                reason: "drain deadline reached".into(),
            },
            1,
        ),
    ] {
        let bytes = Writer::new(Vec::new(), 80, 24, None)?.finish(
            Duration::from_secs(1),
            result,
            capture.clone(),
        )?;
        let events = Reader::new(bytes.as_slice())?.collect::<Result<Vec<_>, _>>()?;
        let metadata = events.first().ok_or("missing finish")?;
        let v3::EventPayload::Unknown { data, .. } = &metadata.payload else {
            return Err("missing metadata".into());
        };
        assert_eq!(
            decode(data)?,
            Decoded::Event(Event::RunFinished { result, capture })
        );
        assert!(
            matches!(events.last().map(|event| &event.payload), Some(v3::EventPayload::Exit(status)) if status.code() == expected)
        );
    }
    Ok(())
}

#[derive(Default)]
struct SinkState {
    bytes: Vec<u8>,
    remaining: Option<usize>,
    fail_flush: bool,
    calls: usize,
}

struct Sink(Rc<RefCell<SinkState>>);

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        state.calls += 1;
        let written = state
            .remaining
            .map_or(bytes.len(), |remaining| remaining.min(bytes.len()));
        if written == 0 {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "disk full"));
        }
        let prefix = bytes
            .get(..written)
            .ok_or_else(|| io::Error::other("invalid test write"))?;
        state.bytes.extend_from_slice(prefix);
        if let Some(remaining) = state.remaining.as_mut() {
            *remaining -= written;
        }
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        state.calls += 1;
        if state.fail_flush {
            Err(io::Error::other("flush failed"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn partial_writes_poison_the_writer_and_leave_a_readable_prefix() -> TestResult {
    let state = Rc::new(RefCell::new(SinkState::default()));
    let mut writer = Writer::new(Sink(Rc::clone(&state)), 80, 24, None)?;
    writer.output(Duration::ZERO, "first")?;
    state.borrow_mut().remaining = Some(8);
    assert!(
        matches!(writer.output(Duration::from_secs(1), "partial"), Err(WriteError::Io(error)) if error.kind() == io::ErrorKind::StorageFull)
    );
    let calls = state.borrow().calls;
    assert!(matches!(
        writer.output(Duration::from_secs(2), "later"),
        Err(WriteError::Failed)
    ));
    assert!(matches!(
        writer.finish(
            Duration::from_secs(3),
            RunResult::Succeeded,
            Capture::Complete
        ),
        Err(WriteError::Failed)
    ));
    assert_eq!(state.borrow().calls, calls);
    let state = state.borrow();
    let mut reader = Reader::new(state.bytes.as_slice())?;
    assert!(
        matches!(reader.next().transpose()?.map(|event| event.payload), Some(v3::EventPayload::Output(text)) if text == "first")
    );
    assert!(matches!(
        reader.next(),
        Some(Err(ReadError::Truncated { .. }))
    ));
    Ok(())
}

#[test]
fn flush_errors_stop_future_writes_without_hiding_the_original_error() -> TestResult {
    let state = Rc::new(RefCell::new(SinkState::default()));
    let mut writer = Writer::new(Sink(Rc::clone(&state)), 80, 24, None)?;
    writer.output(Duration::ZERO, "first")?;
    state.borrow_mut().fail_flush = true;
    assert!(
        matches!(writer.flush(), Err(WriteError::Io(error)) if error.to_string() == "flush failed")
    );
    let calls = state.borrow().calls;
    assert!(matches!(
        writer.marker(Duration::ZERO, "must not write"),
        Err(WriteError::Failed)
    ));
    assert!(matches!(writer.flush(), Err(WriteError::Failed)));
    assert_eq!(state.borrow().calls, calls);
    Ok(())
}
