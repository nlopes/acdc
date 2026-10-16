use std::{env, fs::OpenOptions, io, time::Duration};

use acdc_runbook::{
    Capture, Event, Outcome, Position, RunResult, SkipReason, Source, Step, Termination, Writer,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os()
        .nth(1)
        .ok_or_else(|| io::Error::other("usage: recording NEW.cast"))?;
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = Writer::new(file, 80, 24, Some("Runbook format sample"))?;
    writer.metadata(
        Duration::ZERO,
        &Event::RunStarted {
            steps: vec![step(0, "build", vec![], 5), step(1, "deploy", vec![0], 12)],
        },
    )?;
    writer.metadata(Duration::ZERO, &Event::CommandStarted { occurrence: 0 })?;
    writer.marker(Duration::ZERO, "Build")?;
    writer.output(Duration::from_millis(100), "Building…\r\n")?;
    writer.output(Duration::from_millis(600), "Build failed\r\n")?;
    writer.metadata(
        Duration::from_millis(600),
        &Event::CommandFinished {
            occurrence: 0,
            outcome: Outcome::Failed {
                termination: Termination::Exited { code: 7 },
            },
        },
    )?;
    writer.marker(Duration::from_millis(600), "Build failed")?;
    writer.metadata(
        Duration::from_millis(600),
        &Event::CommandFinished {
            occurrence: 1,
            outcome: Outcome::Skipped {
                reason: SkipReason::DependencyFailed { occurrence: 0 },
            },
        },
    )?;
    writer.finish(
        Duration::from_millis(700),
        RunResult::Failed,
        Capture::Complete,
    )?;
    Ok(())
}

fn step(occurrence: u32, id: &str, dependencies: Vec<u32>, line: usize) -> Step {
    Step {
        occurrence,
        id: id.into(),
        description: None,
        section: Some("Deploy".into()),
        interpreter: "sh".into(),
        dependencies,
        source: Source {
            path: Some("runbook.adoc".into()),
            start: Position { line, column: 1 },
            end: Position {
                line: line + 3,
                column: 5,
            },
        },
    }
}
