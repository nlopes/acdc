use std::time::Duration;

use acdc_runbook::{
    Capture, Event, Outcome, Position, RunResult, SkipReason, Source, Step, Termination, Writer,
};
use osc7501::{Kind, Progress, RecordId, State, Status};
use serde_json::Value;

use super::tests::render;
use crate::HtmlVariant;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn step(occurrence: u32) -> Step {
    Step {
        occurrence,
        id: format!("step-{occurrence}"),
        description: Some("Build and deploy".into()),
        section: Some("Deployment".into()),
        interpreter: "sh".into(),
        dependencies: if occurrence == 2 { vec![0] } else { Vec::new() },
        source: Source {
            path: Some("javascript:alert(1)".into()),
            start: Position { line: 5, column: 1 },
            end: Position { line: 8, column: 5 },
        },
    }
}

fn status(writer: &mut Writer<&mut Vec<u8>>, at: u64, status: Status<'_>) -> Result {
    let mut output = Vec::new();
    status.write_to(&mut output)?;
    writer.output(Duration::from_secs(at), std::str::from_utf8(&output)?)?;
    Ok(())
}

fn recording() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 40, 4, Some("Recorded deployment"))?;
    writer.metadata(
        Duration::ZERO,
        &Event::RunStarted {
            steps: vec![step(0), step(1), step(2)],
        },
    )?;
    writer.metadata(Duration::ZERO, &Event::CommandStarted { occurrence: 0 })?;
    writer.output(Duration::ZERO, "Terminal screen stays the same\r\n")?;
    let id = RecordId::new("build/compile")?;
    status(
        &mut writer,
        1,
        Status::new(State::Working)
            .id(id)
            .title("Compiling")
            .progress(Progress::new(10)?),
    )?;
    status(
        &mut writer,
        2,
        Status::new(State::Error).id(id).title("Compile failed"),
    )?;
    status(
        &mut writer,
        3,
        Status::new(State::Working).id(id).title("Retrying"),
    )?;
    status(
        &mut writer,
        5,
        Status::new(State::Done).id(id).title("Compile complete"),
    )?;
    writer.metadata(
        Duration::from_secs(6),
        &Event::CommandFinished {
            occurrence: 0,
            outcome: Outcome::Failed {
                termination: Termination::Exited { code: 7 },
            },
        },
    )?;
    writer.metadata(
        Duration::from_secs(7),
        &Event::CommandStarted { occurrence: 1 },
    )?;
    for (at, kind) in [(8, Kind::Permission), (9, Kind::Auth), (10, Kind::Question)] {
        status(
            &mut writer,
            at,
            Status::new(State::Blocked)
                .id(id)
                .kind(kind)
                .title("Deploy service")
                .message("Please respond"),
        )?;
    }
    status(
        &mut writer,
        13,
        Status::new(State::Done).id(id).title("Deployed"),
    )?;
    writer.metadata(
        Duration::from_secs(14),
        &Event::CommandFinished {
            occurrence: 1,
            outcome: Outcome::Succeeded,
        },
    )?;
    writer.metadata(
        Duration::from_secs(14),
        &Event::CommandFinished {
            occurrence: 2,
            outcome: Outcome::Skipped {
                reason: SkipReason::DependencyFailed { occurrence: 0 },
            },
        },
    )?;
    writer.finish(
        Duration::from_secs(15),
        RunResult::Failed,
        Capture::Complete,
    )?;
    Ok(bytes)
}

fn document(bytes: &[u8]) -> Result<String> {
    Ok(format!(
        "= Runbook\n:!webfonts:\n\n[terminal%replay,format=asciicast,subs=none,replay-duration-ms=30000]\n----\n{}----\n",
        std::str::from_utf8(bytes)?
    ))
}

fn payload(html: &str) -> Result<Value> {
    let (_, data) = html
        .split_once("<script type=\"application/json\" class=\"terminal-view__data\">")
        .ok_or("missing player data")?;
    Ok(serde_json::from_str(
        data.split_once("</script>").ok_or("missing script end")?.0,
    )?)
}

fn at<'a>(value: &'a Value, path: &str) -> Result<&'a Value> {
    value
        .pointer(path)
        .ok_or_else(|| format!("missing player data at {path}").into())
}

#[test]
fn runbook_summary_and_program_states_are_useful_without_scripting() -> Result {
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let html = render(&document(&recording()?)?, variant)?;
        assert!(html.contains("Recorded run: Failed. Capture: complete."));
        assert!(html.contains("Exited with code 7"));
        assert!(html.contains("Compile complete"));
        assert!(html.contains("Deployed"));
        assert!(html.contains("Prerequisite step-0 did not succeed"));
        assert!(html.contains("6.0 s recorded"));
        assert!(html.contains("javascript:alert(1):5:1–8:5"));
        assert!(!html.contains("href=\"javascript:"));
        assert!(html.contains("data-task-scope=\"command-0\""));
        assert!(html.contains("data-task-scope=\"command-1\""));
        assert!(html.contains("data-replay-play disabled"));
        let data = payload(&html)?;
        assert_eq!(at(&data, "/activity/steps/0/state")?, "failed");
        assert_eq!(at(&data, "/activity/steps/1/state")?, "succeeded");
        assert_eq!(at(&data, "/activity/steps/2/state")?, "skipped");
        assert_eq!(at(&data, "/activity/steps/0/elapsedMs")?, 6_000);
        assert_eq!(at(&data, "/activity/steps/1/elapsedMs")?, 7_000);
    }
    Ok(())
}

#[test]
fn task_events_and_command_failures_have_distinct_categories_and_shared_playback_scaling() -> Result
{
    let html = render(&document(&recording()?)?, HtmlVariant::Standard)?;
    let data = payload(&html)?;
    let events = at(&data, "/activity/events")?
        .as_array()
        .ok_or("missing navigation events")?;
    for category in [
        "command-failure",
        "task-error",
        "permission",
        "auth",
        "question",
    ] {
        assert!(
            events
                .iter()
                .any(|event| event.get("category").and_then(Value::as_str) == Some(category))
        );
    }
    let permission = events
        .iter()
        .find(|event| event.get("category").and_then(Value::as_str) == Some("permission"))
        .ok_or("missing permission request")?;
    // The three-second quiet gap is compressed to two seconds before the
    // playback override scales every stream by the same factor.
    assert_eq!(at(permission, "/at")?, 8_000 * 30_000 / 14_000);
    assert_eq!(at(permission, "/rawMs")?, 8_000);
    assert_eq!(
        at(&data, "/activity/steps/1/start/at")?,
        7_000 * 30_000 / 14_000
    );
    assert_eq!(at(&data, "/activity/rawDurationMs")?, 15_000);
    assert_eq!(at(&data, "/durationMs")?, 30_000);
    assert_eq!(
        at(&data, "/frames")?
            .as_array()
            .ok_or("missing frames")?
            .len(),
        1
    );
    Ok(())
}

#[test]
fn task_and_metadata_labels_cannot_create_markup_or_hide_directional_controls() -> Result {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 40, 4, Some("title\u{202e}evil"))?;
    status(
        &mut writer,
        0,
        Status::new(State::Done)
            .title("</script><img src=x onerror=alert(1)>\u{202e}evil\u{e0001}")
            .message("https://example.invalid/path"),
    )?;
    let html = render(&document(&bytes)?, HtmlVariant::Standard)?;
    let data = payload(&html)?;
    assert!(html.contains("&lt;/script&gt;&lt;img"));
    assert!(!html.contains("<img src=x"));
    assert!(!html.contains("href=\"https://example.invalid/path"));
    assert!(!html.contains('\u{202e}'));
    assert!(!html.contains('\u{e0001}'));
    assert!(
        at(&data, "/activity/changes/0/record/label")?
            .as_str()
            .ok_or("missing task label")?
            .contains('\u{fffd}')
    );
    Ok(())
}

#[test]
fn missing_command_outcome_and_missing_trailer_are_explicit_in_the_summary() -> Result {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 40, 4, None)?;
    writer.metadata(
        Duration::ZERO,
        &Event::RunStarted {
            steps: vec![step(0)],
        },
    )?;
    writer.metadata(Duration::ZERO, &Event::CommandStarted { occurrence: 0 })?;
    status(&mut writer, 1, Status::new(State::Done))?;
    let html = render(&document(&bytes)?, HtmlVariant::Standard)?;
    assert!(html.contains("Incomplete recording — no run completion was captured."));
    assert!(html.contains("Outcome not captured"));
    assert!(!html.contains("Recorded run: Succeeded"));
    Ok(())
}

#[test]
fn exported_checkpoints_reference_complete_replacements_and_keep_root_keys_distinct() -> Result {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 40, 4, None)?;
    status(&mut writer, 0, Status::new(State::Done))?;
    for index in 0..600 {
        status(
            &mut writer,
            index,
            Status::new(if index % 2 == 0 {
                State::Error
            } else {
                State::Working
            })
            .id(RecordId::new("task")?),
        )?;
    }
    let html = render(&document(&bytes)?, HtmlVariant::Standard)?;
    let data = payload(&html)?;
    let changes = at(&data, "/activity/changes")?
        .as_array()
        .ok_or("missing changes")?;
    let points = at(&data, "/activity/checkpoints")?
        .as_array()
        .ok_or("missing checkpoints")?;
    assert_eq!(points.len(), 2);
    for point in points {
        let indices = at(point, "/indices")?.as_array().ok_or("missing indices")?;
        assert_eq!(indices.len(), 2);
        for index in indices {
            let index = usize::try_from(index.as_u64().ok_or("bad index")?)?;
            assert!(!at(changes.get(index).ok_or("missing change")?, "/record")?.is_null());
        }
    }
    assert_ne!(
        at(changes.first().ok_or("missing root")?, "/key")?,
        at(changes.get(1).ok_or("missing named task")?, "/key")?
    );
    Ok(())
}
