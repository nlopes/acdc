use std::time::Duration;

use acdc_runbook::{
    Capture, Event, Outcome, Position, RunResult, SkipReason, Source, Step, Termination, Writer,
};
use osc7501::{AppId, Kind, Progress, RecordId, State, Status};

use super::{Recording, parse};
use crate::{
    activity::{Change, Issue, Scope},
    cell_grid::TerminalSize,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn report(status: Status<'_>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    status.write_to(&mut bytes)?;
    Ok(bytes)
}

fn recording(outputs: &[(u64, Vec<u8>)]) -> Result<Recording> {
    let mut bytes = Vec::new();
    {
        let mut writer = Writer::new(&mut bytes, 20, 4, None)?;
        for (millis, output) in outputs {
            writer.output(Duration::from_millis(*millis), std::str::from_utf8(output)?)?;
        }
    }
    Ok(parse(std::str::from_utf8(&bytes)?)?)
}

#[test]
fn status_only_reports_survive_every_split_and_blank_screen_deduplication() -> Result {
    let bytes = report(
        Status::new(State::Blocked)
            .kind(Kind::Permission)
            .title("部署")
            .message("Allow access?"),
    )?;
    for split in 1..bytes.len() {
        let first = bytes.get(..split).ok_or("bad split")?.to_vec();
        let second = bytes.get(split..).ok_or("bad split")?.to_vec();
        let recording = recording(&[(0, first), (500, second)])?;
        let changes = recording.activity().changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes.first().map(|change| change.at),
            Some(Duration::from_millis(500))
        );
        let records = recording.activity().records_at(Duration::from_secs(1));
        let record = records.first().ok_or("missing task")?;
        assert_eq!(record.title.as_deref(), Some("部署"));
        assert_eq!(record.kind, Some(Kind::Permission));
        let captured = recording.capture(TerminalSize::new(20, 4))?;
        assert_eq!(captured.frames().len(), 1);
        assert_eq!(
            captured.activity().duration(),
            Some(Duration::from_millis(500))
        );
    }
    Ok(())
}

#[test]
fn complete_replacement_and_ancestor_app_changes_are_owned_in_history() -> Result {
    let root = report(Status::new(State::Idle).app(AppId::new("build")?))?;
    let child = report(
        Status::new(State::Working)
            .id(RecordId::new("task/child")?)
            .title("")
            .progress(Progress::new(42)?),
    )?;
    let root_changed = report(Status::new(State::Idle).app(AppId::new("deploy")?))?;
    let done = report(Status::new(State::Done).id(RecordId::new("task/child")?))?;
    let recording = recording(&[(0, root), (100, child), (200, root_changed), (300, done)])?;
    let history = recording.activity();
    let before = history.records_at(Duration::from_millis(100));
    let before = before
        .iter()
        .find(|record| record.key.id.as_deref() == Some("task/child"))
        .ok_or("missing child")?;
    assert_eq!(before.effective_app.as_deref(), Some("build"));
    assert_eq!(before.title.as_deref(), Some(""));
    assert_eq!(before.progress, Some(42));
    let after = history.records_at(Duration::from_millis(300));
    let after = after
        .iter()
        .find(|record| record.key.id.as_deref() == Some("task/child"))
        .ok_or("missing child")?;
    assert_eq!(after.effective_app.as_deref(), Some("deploy"));
    assert_eq!(after.state, State::Done);
    assert_eq!(after.title, None);
    assert_eq!(after.progress, None);
    Ok(())
}

#[test]
fn clear_is_subtree_aware_and_root_clear_removes_every_record() -> Result {
    let mut initial = Vec::new();
    for id in ["task", "task/child", "taskish"] {
        initial.extend(report(Status::new(State::Working).id(RecordId::new(id)?))?);
    }
    let subtree = report(Status::clear().id(RecordId::new("task")?))?;
    let root = report(Status::clear())?;
    let recording = recording(&[(0, initial), (100, subtree), (200, root)])?;
    let middle = recording.activity().records_at(Duration::from_millis(100));
    assert_eq!(middle.len(), 1);
    assert_eq!(
        middle.first().and_then(|record| record.key.id.as_deref()),
        Some("taskish")
    );
    assert_eq!(
        recording.activity().records_at(Duration::from_millis(200)),
        [] as [&crate::activity::Record; 0]
    );
    Ok(())
}

#[test]
fn prompt_exit_and_ris_use_store_lifecycle_rules() -> Result {
    let mut initial = Vec::new();
    for (id, state) in [
        ("idle", State::Idle),
        ("work", State::Working),
        ("block", State::Blocked),
        ("done", State::Done),
        ("error", State::Error),
    ] {
        initial.extend(report(Status::new(state).id(RecordId::new(id)?))?);
    }
    let recording = recording(&[
        (0, initial),
        (100, b"\x1b[!p\x1b[?1049h\x1b[?1049l".to_vec()),
        (200, b"\x1b]133;A\x1b\\".to_vec()),
        (300, b"\x1bc".to_vec()),
    ])?;
    assert_eq!(
        recording
            .activity()
            .records_at(Duration::from_millis(100))
            .len(),
        5
    );
    assert_eq!(
        recording
            .activity()
            .records_at(Duration::from_millis(200))
            .len(),
        3
    );
    assert_eq!(
        recording.activity().records_at(Duration::from_millis(300)),
        [] as [&crate::activity::Record; 0]
    );

    let output = report(Status::new(State::Working))?;
    let mut bytes = Vec::new();
    {
        let mut writer = Writer::new(&mut bytes, 20, 4, None)?;
        writer.output(Duration::ZERO, std::str::from_utf8(&output)?)?;
    }
    bytes.extend_from_slice(b"[1,\"x\",\"7\"]\n");
    let recording = parse(std::str::from_utf8(&bytes)?)?;
    assert_eq!(
        recording.activity().records_at(Duration::from_secs(1)),
        [] as [&crate::activity::Record; 0]
    );
    assert!(
        recording
            .activity()
            .changes()
            .iter()
            .any(|change| matches!(change.change, Change::Remove(_)))
    );
    Ok(())
}

#[test]
fn eviction_and_update_age_come_from_the_library_store() -> Result {
    let mut output = Vec::new();
    for index in 0..256 {
        output.extend(report(
            Status::new(State::Done).id(RecordId::new(&format!("task{index}"))?),
        )?);
    }
    output.extend(report(
        Status::new(State::Error).id(RecordId::new("task0")?),
    )?);
    output.extend(report(Status::new(State::Done).id(RecordId::new("new")?))?);
    let recording = recording(&[(0, output)])?;
    let records = recording.activity().records_at(Duration::ZERO);
    assert_eq!(records.len(), 256);
    assert!(
        records
            .iter()
            .any(|record| record.key.id.as_deref() == Some("task0"))
    );
    assert!(
        !records
            .iter()
            .any(|record| record.key.id.as_deref() == Some("task1"))
    );
    assert!(recording.activity().changes().iter().any(
        |change| matches!(&change.change, Change::Remove(key) if key.id.as_deref() == Some("task1"))
    ));
    Ok(())
}

#[test]
fn backward_seeks_restore_retries_across_checkpoints() -> Result {
    let mut outputs = Vec::new();
    for index in 0..600 {
        let state = match index % 3 {
            0 => State::Error,
            1 => State::Working,
            _ => State::Done,
        };
        outputs.push((index, report(Status::new(state))?));
    }
    let recording = recording(&outputs)?;
    for index in [599, 0, 257, 1, 598, 2] {
        let expected = match index % 3 {
            0 => State::Error,
            1 => State::Working,
            _ => State::Done,
        };
        assert_eq!(
            recording
                .activity()
                .records_at(Duration::from_millis(index))
                .first()
                .map(|record| record.state),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn cancellation_oversize_and_invalid_reports_do_not_create_partial_records() -> Result {
    let mut bad = b"\x1b]7501;state=working\x18".to_vec();
    bad.extend_from_slice(b"\x1b]7501;state=working:msg=Cg==\x07");
    bad.extend_from_slice(b"\x1b]7501;state=working:x=");
    bad.extend(std::iter::repeat_n(b'a', 4096));
    bad.push(7);
    let recording = recording(&[(0, bad), (100, report(Status::new(State::Done))?)])?;
    assert_eq!(recording.activity().changes().len(), 1);
    assert!(
        recording
            .activity()
            .issues()
            .iter()
            .any(|issue| matches!(issue, Issue::MalformedReports(2)))
    );
    Ok(())
}

#[test]
fn control_string_framing_matches_the_screen_emulator() -> Result {
    for prefix in [b"\x1bPq".as_slice(), b"\x1bX", b"\x1b^", b"\x1b_"] {
        let mut title = prefix.to_vec();
        title.extend_from_slice(b"\x1b]2;framing-test\x1b\\\x1b\\");
        let mut terminal = libghostty_vt::Terminal::new(20, 4)?;
        terminal.vt_write(&title);
        let expected = usize::from(terminal.title()? == "framing-test");
        let mut output = prefix.to_vec();
        output.extend(report(Status::new(State::Done))?);
        output.extend_from_slice(b"\x1b\\");
        let recording = recording(&[(0, output)])?;
        assert_eq!(
            recording.activity().changes().len(),
            expected,
            "prefix {prefix:?}"
        );
    }
    Ok(())
}

#[test]
fn all_events_keep_raw_time_and_trailing_exit_extends_playback() -> Result {
    let mut bytes = Vec::new();
    {
        let mut writer = Writer::new(&mut bytes, 20, 4, None)?;
        writer.output(Duration::from_secs(1), "visible")?;
        writer.marker(Duration::from_secs(100), "quiet work")?;
        writer.output(
            Duration::from_secs(101),
            std::str::from_utf8(&report(Status::new(State::Done))?)?,
        )?;
    }
    bytes.extend_from_slice(b"[49,\"x\",\"0\"]\n");
    let recording = parse(std::str::from_utf8(&bytes)?)?;
    assert_eq!(recording.duration(), Some(Duration::from_secs(6)));
    assert_eq!(
        recording.activity().raw_duration(),
        Some(Duration::from_secs(150))
    );
    assert_eq!(
        recording
            .activity()
            .markers()
            .first()
            .map(|marker| marker.at),
        Some(Duration::from_secs(3))
    );
    let captured = recording.capture(TerminalSize::new(20, 4))?;
    assert_eq!(captured.frames().len(), 1);
    assert_eq!(
        captured
            .activity()
            .changes()
            .first()
            .map(|change| (change.raw_at, change.at)),
        Some((Duration::from_secs(101), Duration::from_secs(4)))
    );
    Ok(())
}

fn step(occurrence: u32) -> Step {
    Step {
        occurrence,
        id: format!("step{occurrence}"),
        description: None,
        section: None,
        interpreter: "sh".into(),
        dependencies: Vec::new(),
        source: Source {
            path: Some("runbook.adoc".into()),
            start: Position { line: 5, column: 1 },
            end: Position { line: 8, column: 5 },
        },
    }
}

fn book() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 20, 4, None)?;
    writer.metadata(
        Duration::ZERO,
        &Event::RunStarted {
            steps: vec![step(0), step(1)],
        },
    )?;
    for occurrence in 0..2 {
        let at = Duration::from_millis(u64::from(occurrence) * 200);
        writer.metadata(at, &Event::CommandStarted { occurrence })?;
        writer.output(
            at,
            std::str::from_utf8(&report(
                Status::new(State::Done).id(RecordId::new("task")?),
            )?)?,
        )?;
        writer.metadata(
            at + Duration::from_millis(100),
            &Event::CommandFinished {
                occurrence,
                outcome: if occurrence == 0 {
                    Outcome::Failed {
                        termination: Termination::Exited { code: 7 },
                    }
                } else {
                    Outcome::Succeeded
                },
            },
        )?;
    }
    writer.finish(
        Duration::from_millis(400),
        RunResult::Failed,
        Capture::Complete,
    )?;
    Ok(bytes)
}

#[test]
fn command_scopes_preserve_repeated_ids_provenance_and_process_outcomes() -> Result {
    let bytes = book()?;
    let recording = parse(std::str::from_utf8(&bytes)?)?;
    let book = recording.activity().runbook().ok_or("missing runbook")?;
    assert!(book.is_complete());
    assert_eq!(
        book.steps()
            .first()
            .and_then(|step| step.source.path.as_deref()),
        Some("runbook.adoc")
    );
    let records = recording.activity().records_at(Duration::from_secs(1));
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .any(|record| record.key.scope == Scope::Command(0))
    );
    assert!(
        records
            .iter()
            .any(|record| record.key.scope == Scope::Command(1))
    );
    assert!(book.events().iter().any(|event| matches!(
        event.event,
        Event::CommandFinished {
            occurrence: 0,
            outcome: Outcome::Failed { .. }
        }
    )));
    Ok(())
}

#[test]
fn unsupported_metadata_falls_back_to_one_session_and_truncation_retains_history() -> Result {
    let mut bytes = book()?;
    bytes.extend_from_slice(b"[0,\"acdc.runbook\",\"{\\\"version\\\":99}\"]\n");
    let recording = parse(std::str::from_utf8(&bytes)?)?;
    assert!(recording.activity().runbook().is_none());
    assert!(
        recording
            .activity()
            .records_at(Duration::from_secs(1))
            .iter()
            .all(|record| record.key.scope == Scope::Session)
    );
    assert!(
        recording
            .activity()
            .issues()
            .iter()
            .any(|issue| matches!(issue, Issue::Metadata(_)))
    );

    let mut bytes = book()?;
    let newline = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .ok_or("missing newline")?;
    bytes.truncate(newline.saturating_sub(2));
    let recording = parse(std::str::from_utf8(&bytes)?)?;
    assert!(
        !recording
            .activity()
            .runbook()
            .ok_or("lost readable runbook")?
            .is_complete()
    );
    assert_eq!(
        recording
            .activity()
            .records_at(Duration::from_secs(1))
            .len(),
        2
    );
    assert!(
        recording
            .activity()
            .issues()
            .iter()
            .any(|issue| matches!(issue, Issue::PartialRecording(_)))
    );
    Ok(())
}

#[test]
fn task_limit_keeps_a_bounded_prefix_with_one_diagnostic() -> Result {
    let mut output = Vec::new();
    for index in 0..crate::activity::MAX_CHANGES + 2 {
        output.extend(report(Status::new(if index % 2 == 0 {
            State::Working
        } else {
            State::Done
        }))?);
    }
    let recording = recording(&[(0, output)])?;
    assert_eq!(
        recording.activity().changes().len(),
        crate::activity::MAX_CHANGES
    );
    assert_eq!(
        recording
            .activity()
            .issues()
            .iter()
            .filter(|issue| matches!(issue, Issue::TaskLimit))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn v2_input_is_inert_and_status_only_output_is_retained() -> Result {
    let input = serde_json::to_string(std::str::from_utf8(&report(Status::new(State::Error))?)?)?;
    let output = serde_json::to_string(std::str::from_utf8(&report(Status::new(State::Done))?)?)?;
    let cast = format!(
        "{{\"version\":2,\"width\":20,\"height\":4}}\n[0,\"i\",{input}]\n[1,\"o\",{output}]\n"
    );
    let recording = parse(&cast)?;
    assert_eq!(recording.activity().changes().len(), 1);
    assert_eq!(
        recording
            .activity()
            .records_at(Duration::from_secs(1))
            .first()
            .map(|record| record.state),
        Some(State::Done)
    );
    Ok(())
}

#[test]
fn full_size_frames_are_valid_but_one_extra_byte_is_rejected() -> Result {
    for terminator in [b"\x07".as_slice(), b"\x1b\\".as_slice()] {
        let mut bytes = b"\x1b]7501;state=done:future=".to_vec();
        bytes.resize(osc7501_parser::MAX_SEQUENCE - terminator.len(), b'x');
        bytes.extend_from_slice(terminator);
        let valid = recording(&[(0, bytes.clone())])?;
        assert_eq!(valid.activity().changes().len(), 1);
        assert_eq!(valid.activity().issues(), []);
        bytes.insert(30, b'x');
        let oversized = recording(&[(0, bytes)])?;
        assert_eq!(oversized.activity().changes(), []);
        assert_eq!(oversized.activity().issues(), [Issue::MalformedReports(1)]);
    }
    Ok(())
}

#[test]
fn skipped_dependencies_and_capture_health_remain_separate_from_task_state() -> Result {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 20, 4, None)?;
    let mut dependent = step(1);
    dependent.dependencies.push(0);
    writer.metadata(
        Duration::ZERO,
        &Event::RunStarted {
            steps: vec![step(0), dependent],
        },
    )?;
    writer.metadata(Duration::ZERO, &Event::CommandStarted { occurrence: 0 })?;
    writer.output(
        Duration::ZERO,
        std::str::from_utf8(&report(Status::new(State::Done))?)?,
    )?;
    writer.metadata(
        Duration::from_secs(1),
        &Event::CommandFinished {
            occurrence: 0,
            outcome: Outcome::Failed {
                termination: Termination::Exited { code: 7 },
            },
        },
    )?;
    writer.metadata(
        Duration::from_secs(1),
        &Event::CommandFinished {
            occurrence: 1,
            outcome: Outcome::Skipped {
                reason: SkipReason::DependencyFailed { occurrence: 0 },
            },
        },
    )?;
    writer.finish(
        Duration::from_secs(1),
        RunResult::Failed,
        Capture::Incomplete {
            reason: "output stream interrupted".into(),
        },
    )?;
    let recording = parse(std::str::from_utf8(&bytes)?)?;
    let book = recording.activity().runbook().ok_or("missing runbook")?;
    assert!(
        book.is_complete(),
        "a complete trailer can report incomplete capture"
    );
    assert_eq!(recording.activity().changes().len(), 1);
    assert!(matches!(
        book.events().last().map(|entry| &entry.event),
        Some(Event::RunFinished {
            result: RunResult::Failed,
            capture: Capture::Incomplete { .. },
        })
    ));
    Ok(())
}

#[test]
fn invalid_lifecycle_falls_back_and_missing_trailer_never_implies_success() -> Result {
    let bytes = book()?;
    let text = std::str::from_utf8(&bytes)?;
    let mut lines: Vec<_> = text.lines().collect();
    lines.pop();
    lines.pop();
    let incomplete = parse(&lines.join("\n"))?;
    assert!(
        !incomplete
            .activity()
            .runbook()
            .ok_or("missing runbook")?
            .is_complete()
    );

    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 20, 4, None)?;
    writer.metadata(
        Duration::ZERO,
        &Event::RunStarted {
            steps: vec![step(0)],
        },
    )?;
    writer.output(
        Duration::ZERO,
        std::str::from_utf8(&report(Status::new(State::Done))?)?,
    )?;
    writer.metadata(
        Duration::ZERO,
        &Event::CommandFinished {
            occurrence: 0,
            outcome: Outcome::Succeeded,
        },
    )?;
    writer.finish(Duration::ZERO, RunResult::Succeeded, Capture::Complete)?;
    let invalid = parse(std::str::from_utf8(&bytes)?)?;
    assert!(invalid.activity().runbook().is_none());
    assert!(
        invalid
            .activity()
            .issues()
            .iter()
            .any(|issue| matches!(issue, Issue::Metadata(_)))
    );
    assert_eq!(
        invalid
            .activity()
            .records_at(Duration::ZERO)
            .first()
            .map(|record| record.key.scope),
        Some(Scope::Session)
    );
    Ok(())
}

#[test]
fn raw_v3_times_preserve_microseconds_separately_from_idle_compression() -> Result {
    let raw_at = Duration::from_micros(9_000_000_001);
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 20, 4, None)?;
    writer.output(
        raw_at,
        std::str::from_utf8(&report(Status::new(State::Done))?)?,
    )?;
    writer.finish(
        raw_at,
        RunResult::Aborted,
        Capture::Incomplete {
            reason: "interrupted".into(),
        },
    )?;
    let recording = parse(std::str::from_utf8(&bytes)?)?;
    assert_eq!(recording.activity().raw_duration(), Some(raw_at));
    assert_eq!(
        recording
            .activity()
            .changes()
            .first()
            .map(|change| change.raw_at),
        Some(raw_at)
    );
    Ok(())
}

#[test]
fn source_event_limit_retains_the_prefix_with_one_diagnostic() -> Result {
    let mut cast =
        String::from("{\"version\":3,\"term\":{\"cols\":20,\"rows\":4}}\n[0,\"o\",\"prefix\"]\n");
    cast.push_str(&"[0,\"z\",\"\"]\n".repeat(super::MAX_RECORDING_EVENTS));
    let recording = parse(&cast)?;
    assert_eq!(recording.events.len(), 1);
    assert_eq!(recording.activity().issues().len(), 1);
    assert!(matches!(
        recording.activity().issues().first(),
        Some(Issue::PartialRecording(_))
    ));
    Ok(())
}

#[test]
fn metadata_diagnostics_bound_and_neutralize_untrusted_text() -> Result {
    let metadata =
        serde_json::json!({ "version": 1, "event": format!("\x1b]{}", "a".repeat(2000)) });
    let encoded = serde_json::to_string(&metadata.to_string())?;
    let cast = format!(
        "{{\"version\":3,\"term\":{{\"cols\":20,\"rows\":4}}}}\n[0,\"acdc.runbook\",{encoded}]\n"
    );
    let recording = parse(&cast)?;
    let issue = recording
        .activity()
        .issues()
        .first()
        .ok_or("missing metadata issue")?;
    let Issue::Metadata(message) = issue else {
        return Err("unexpected issue".into());
    };
    assert_eq!(message.chars().count(), 512);
    assert!(!message.chars().any(char::is_control));
    Ok(())
}
