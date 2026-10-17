#[cfg(unix)]
fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let script = std::env::args_os()
        .nth(1)
        .ok_or("usage: runbook-demo SCRIPT")?;
    let action = std::fs::read_to_string(script)?;
    demo::run(
        action.trim(),
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        std::time::Duration::from_millis(300),
    )
    .map(std::process::ExitCode::from)
}

#[cfg(not(unix))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("the interactive runbook demo requires Unix".into())
}

#[cfg(unix)]
mod demo {
    use std::{
        io::{BufRead, Write},
        time::Duration,
    };

    use osc7501::{AppId, Kind, Progress, RecordId, State, Status};

    type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

    pub(super) fn run(
        action: &str,
        input: &mut impl BufRead,
        output: &mut impl Write,
        delay: Duration,
    ) -> Result<u8> {
        match action {
            "review" => review(input, output, delay),
            "publish" => {
                emit(
                    output,
                    Status::new(State::Done)
                        .app(AppId::new("local-release")?)
                        .title("Package assembled")
                        .message("Task completion does not determine the process exit code"),
                    delay,
                )?;
                writeln!(output, "Simulated publish failure: exiting with code 7.")?;
                Ok(7)
            }
            _ => Err("expected review or publish in the command block".into()),
        }
    }

    fn emit(output: &mut impl Write, status: Status<'_>, delay: Duration) -> Result {
        status.write_to(output)?;
        output.flush()?;
        std::thread::sleep(delay);
        Ok(())
    }

    fn review(input: &mut impl BufRead, output: &mut impl Write, delay: Duration) -> Result<u8> {
        let app = AppId::new("local-release")?;
        let child = RecordId::new("checks/cache")?;
        writeln!(output, "Checking the local release preview.")?;
        emit(
            output,
            Status::new(State::Working)
                .app(app)
                .title("Review release")
                .progress(Progress::new(20)?),
            delay,
        )?;
        for (state, message) in [
            (State::Working, "First attempt"),
            (State::Error, "Simulated cache miss"),
            (State::Working, "Retrying with local data"),
            (State::Done, "Retry succeeded"),
        ] {
            emit(
                output,
                Status::new(state)
                    .id(child)
                    .title("Cache check")
                    .message(message),
                delay,
            )?;
        }
        emit(
            output,
            Status::new(State::Blocked)
                .app(app)
                .title("Review release")
                .kind(Kind::Permission)
                .progress(Progress::new(80)?)
                .message("Approve this local preview; no release will be published"),
            delay,
        )?;
        write!(output, "Type yes to approve the local preview: ")?;
        output.flush()?;
        let mut answer = String::new();
        input.read_line(&mut answer)?;
        if answer.trim() != "yes" {
            emit(
                output,
                Status::new(State::Error)
                    .app(app)
                    .title("Review release")
                    .message("Preview approval declined"),
                delay,
            )?;
            return Ok(2);
        }
        emit(
            output,
            Status::new(State::Working)
                .app(app)
                .title("Review release")
                .progress(Progress::new(100)?),
            delay,
        )?;
        emit(
            output,
            Status::new(State::Done)
                .app(app)
                .title("Review release")
                .message("Local preview approved"),
            delay,
        )?;
        writeln!(output, "Review complete.")?;
        Ok(0)
    }

    #[cfg(test)]
    mod tests {
        use osc7501::parser::{Action, Decoder, Event, Kind, State};

        use super::*;

        #[test]
        fn review_reports_a_retry_and_permission_without_changing_the_screen() -> Result {
            let mut bytes = Vec::new();
            assert_eq!(
                run("review", &mut &b"yes\n"[..], &mut bytes, Duration::ZERO)?,
                0
            );
            let mut decoder = Decoder::new();
            let mut states = Vec::new();
            let mut permissions = 0;
            decoder.feed(&bytes, |event| {
                if let Ok(Event::Report(report)) = event {
                    states.push(report.action());
                    permissions += usize::from(report.kind() == Some(Kind::Permission));
                }
            });
            assert_eq!(permissions, 1);
            assert!(states.windows(3).any(|states| states
                == [
                    Action::Set(State::Error),
                    Action::Set(State::Working),
                    Action::Set(State::Done),
                ]));
            assert_eq!(states.last(), Some(&Action::Set(State::Done)));
            assert!(String::from_utf8(bytes)?.contains("Review complete."));
            Ok(())
        }

        #[test]
        fn declined_approval_and_publish_failure_have_distinct_exit_codes() -> Result {
            let mut output = Vec::new();
            assert_eq!(
                run("review", &mut &b"no\n"[..], &mut output, Duration::ZERO)?,
                2
            );
            output.clear();
            assert_eq!(
                run("publish", &mut &b""[..], &mut output, Duration::ZERO)?,
                7
            );
            let mut decoder = Decoder::new();
            let mut done = false;
            decoder.feed(&output, |event| {
                if let Ok(Event::Report(report)) = event {
                    done |= report.action() == Action::Set(State::Done);
                }
            });
            assert!(done);
            Ok(())
        }
    }
}
