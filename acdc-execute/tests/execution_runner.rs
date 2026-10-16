use std::{cell::RefCell, convert::Infallible, fs, io, path::PathBuf};

use acdc_execute::{
    CommandBlock, CommandGraph, CommandGraphBuilder, CommandId, ExecutionEvent, ExecutionOptions,
    OutcomeState, ProcessError, ProcessOptions, RunOutcome, RunReport, SkipReason, Termination,
};
use acdc_parser::{Location, SourceLocation};
use rstest::rstest;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn legacy_variants_keep_imports_and_type_inference() {
    use acdc_execute::{CommandState::Succeeded, ExecError::Process};

    let error = Process(io::Error::other("missing interpreter"));
    assert_eq!(
        error.to_string(),
        "could not run interpreter: missing interpreter"
    );
    let state = Succeeded;
    assert!(matches!(state, Succeeded));
}

fn graph(specs: &[(&str, &[&str])]) -> TestResult<CommandGraph> {
    let mut builder = CommandGraphBuilder::new();
    for (id, dependencies) in specs {
        builder.add(
            CommandBlock::new(
                CommandId::new(*id)?,
                "exit 0".to_owned(),
                None,
                SourceLocation::at_location(None, Location::default()),
            ),
            dependencies
                .iter()
                .map(|dependency| CommandId::new(*dependency))
                .collect::<Result<_, _>>()?,
        );
    }
    Ok(builder.build()?)
}

fn state_names(report: &RunReport<'_>) -> Vec<&'static str> {
    report
        .outcomes()
        .iter()
        .map(|outcome| match outcome.state {
            OutcomeState::Succeeded => "succeeded",
            OutcomeState::Failed(_) => "failed",
            OutcomeState::Skipped(SkipReason::DependencyFailed { .. }) => "dependency",
            OutcomeState::Skipped(SkipReason::StoppedAfterFailure { .. }) => "stopped",
        })
        .collect()
}

#[test]
fn runner_and_observer_follow_dependency_policy_in_order() -> TestResult {
    let graph = graph(&[
        ("first", &[]),
        ("dependent", &["first"]),
        ("independent", &[]),
    ])?;
    let events = RefCell::new(Vec::new());
    let report = graph
        .plan_all()
        .execute_with(
            &ExecutionOptions::default(),
            |command, _| {
                events
                    .borrow_mut()
                    .push(format!("run:{}", command.metadata.id));
                Ok(Termination::Exited(
                    u32::from(command.metadata.id.as_str() == "first") * 7,
                ))
            },
            |event| {
                events.borrow_mut().push(match event {
                    ExecutionEvent::CommandStarted { command } => {
                        format!("start:{}", command.metadata.id)
                    }
                    ExecutionEvent::CommandFinished { outcome } => {
                        format!("finish:{}", outcome.command.metadata.id)
                    }
                });
                Ok::<(), Infallible>(())
            },
        )
        .map_err(|error| error.source)?;
    assert_eq!(state_names(&report), ["failed", "dependency", "succeeded"]);
    assert_eq!(
        events.into_inner(),
        [
            "start:first",
            "run:first",
            "finish:first",
            "finish:dependent",
            "start:independent",
            "run:independent",
            "finish:independent",
        ]
    );
    assert!(matches!(report.outcomes(), [
        RunOutcome { state: OutcomeState::Failed(ProcessError::Failed(Termination::Exited(7))), .. },
        RunOutcome { state: OutcomeState::Skipped(SkipReason::DependencyFailed { dependency }), .. },
        RunOutcome { state: OutcomeState::Succeeded, .. },
    ] if dependency.as_str() == "first"));
    Ok(())
}

#[rstest]
#[case(Termination::Exited(7), true)]
#[case(Termination::Signaled("SIGINT".to_owned()), false)]
#[case(Termination::Unknown, false)]
fn abnormal_termination_stops_independent_commands(
    #[case] termination: Termination,
    #[case] exit_on_failure: bool,
) -> TestResult {
    let graph = graph(&[("first", &[]), ("independent", &[])])?;
    let mut calls = 0;
    let report = graph
        .plan_all()
        .execute_with(
            &ExecutionOptions {
                exit_on_failure,
                ..ExecutionOptions::default()
            },
            |_, _| {
                calls += 1;
                Ok(termination.clone())
            },
            |_| Ok::<(), Infallible>(()),
        )
        .map_err(|error| error.source)?;
    assert_eq!(calls, 1);
    assert_eq!(state_names(&report), ["failed", "stopped"]);
    assert!(matches!(report.outcomes().last(), Some(RunOutcome {
        state: OutcomeState::Skipped(SkipReason::StoppedAfterFailure { command }), ..
    }) if command.as_str() == "first"));
    Ok(())
}

#[test]
fn runner_io_errors_stop_the_plan_and_keep_the_source() -> TestResult {
    let graph = graph(&[("first", &[]), ("independent", &[])])?;
    let mut calls = 0;
    let report = graph
        .plan_all()
        .execute_with(
            &ExecutionOptions::default(),
            |_, _| {
                calls += 1;
                Err(io::Error::new(io::ErrorKind::NotFound, "missing runner"))
            },
            |_| Ok::<(), Infallible>(()),
        )
        .map_err(|error| error.source)?;
    assert_eq!(calls, 1);
    assert_eq!(state_names(&report), ["failed", "stopped"]);
    assert!(matches!(report.outcomes().first(), Some(RunOutcome {
        state: OutcomeState::Failed(ProcessError::Process(error)), ..
    }) if error.kind() == io::ErrorKind::NotFound && error.to_string() == "missing runner"));
    Ok(())
}

#[test]
fn failing_start_notification_prevents_the_next_runner_call() -> TestResult {
    let graph = graph(&[("first", &[]), ("second", &[])])?;
    let mut calls = 0;
    let result = graph.plan_all().execute_with(
        &ExecutionOptions::default(),
        |_, _| {
            calls += 1;
            Ok(Termination::Exited(0))
        },
        |event| match event {
            ExecutionEvent::CommandStarted { command }
                if command.metadata.id.as_str() == "second" =>
            {
                Err(io::Error::other("recording is full"))
            }
            ExecutionEvent::CommandStarted { .. } | ExecutionEvent::CommandFinished { .. } => {
                Ok(())
            }
        },
    );
    let error = result.err().ok_or("observer error was lost")?;
    assert_eq!(calls, 1);
    assert_eq!(error.source.to_string(), "recording is full");
    assert_eq!(state_names(&error.report), ["succeeded"]);
    Ok(())
}

#[test]
fn failing_finish_notification_keeps_the_completed_outcome() -> TestResult {
    let graph = graph(&[
        ("first", &[]),
        ("dependent", &["first"]),
        ("independent", &[]),
    ])?;
    let mut calls = 0;
    let result = graph.plan_all().execute_with(
        &ExecutionOptions::default(),
        |_, _| {
            calls += 1;
            Ok(Termination::Exited(7))
        },
        |event| match event {
            ExecutionEvent::CommandFinished { outcome }
                if matches!(outcome.state, OutcomeState::Skipped(_)) =>
            {
                Err("cannot record skip")
            }
            ExecutionEvent::CommandStarted { .. } | ExecutionEvent::CommandFinished { .. } => {
                Ok(())
            }
        },
    );
    let error = result.err().ok_or("observer error was lost")?;
    assert_eq!(error.source, "cannot record skip");
    assert_eq!(state_names(&error.report), ["failed", "dependency"]);
    assert_eq!(calls, 1);
    Ok(())
}

#[test]
fn empty_plan_never_calls_the_runner_or_observer() -> TestResult {
    let graph = graph(&[])?;
    let mut runner_calls = 0;
    let mut observer_calls = 0;
    let report = graph
        .plan_all()
        .execute_with(
            &ExecutionOptions::default(),
            |_, _| {
                runner_calls += 1;
                Ok(Termination::Exited(0))
            },
            |_| {
                observer_calls += 1;
                Ok::<(), Infallible>(())
            },
        )
        .map_err(|error| error.source)?;
    assert!(report.is_success());
    assert!(report.outcomes().is_empty());
    assert_eq!((runner_calls, observer_calls), (0, 0));
    Ok(())
}

#[rstest]
#[case(false)]
#[case(true)]
fn runner_receives_the_exact_prepared_process_and_script_lifetime(
    #[case] fail: bool,
) -> TestResult {
    let directory = tempfile::tempdir()?;
    let parent_directory = std::env::current_dir()?;
    let script = "printf 'exact'\r\n\r\n  ";
    let mut builder = CommandGraphBuilder::new();
    builder.add(
        CommandBlock::new(
            CommandId::new("prepared")?,
            script.to_owned(),
            Some(format!("bin{}interpreter", std::path::MAIN_SEPARATOR)),
            SourceLocation::at_location(None, Location::default()),
        ),
        Vec::new(),
    );
    let graph = builder.build()?;
    let mut script_path = PathBuf::new();
    let report = graph
        .plan_all()
        .execute_with(
            &ExecutionOptions {
                process: ProcessOptions {
                    current_dir: Some(directory.path().to_owned()),
                    env: vec![
                        ("ACDC_RUNNER_TEST".into(), "first".into()),
                        ("ACDC_RUNNER_TEST".into(), "last=kept".into()),
                    ],
                },
                ..ExecutionOptions::default()
            },
            |command, process| {
                assert_eq!(command.metadata.id.as_str(), "prepared");
                assert_eq!(
                    process.get_program(),
                    parent_directory.join("bin").join("interpreter")
                );
                assert_eq!(process.get_current_dir(), Some(directory.path()));
                let mut arguments = process.get_args();
                let argument = arguments
                    .next()
                    .ok_or_else(|| io::Error::other("missing script"))?;
                assert!(arguments.next().is_none());
                script_path = PathBuf::from(argument);
                assert_eq!(fs::read(&script_path)?, script.as_bytes());
                assert_eq!(
                    process.get_envs().collect::<Vec<_>>(),
                    [(
                        std::ffi::OsStr::new("ACDC_RUNNER_TEST"),
                        Some(std::ffi::OsStr::new("last=kept"))
                    )]
                );
                if fail {
                    Err(io::Error::other("runner failed"))
                } else {
                    Ok(Termination::Exited(0))
                }
            },
            |_| Ok::<(), Infallible>(()),
        )
        .map_err(|error| error.source)?;
    assert_eq!(report.is_success(), !fail);
    assert!(!script_path.as_os_str().is_empty());
    assert!(!script_path.exists());
    assert_eq!(std::env::current_dir()?, parent_directory);
    Ok(())
}

#[cfg(unix)]
#[rstest]
#[case("exit 0", ["succeeded", "succeeded", "succeeded"])]
#[case("exit 7", ["failed", "dependency", "succeeded"])]
#[case("kill -TERM $$", ["failed", "stopped", "stopped"])]
fn real_process_runner_matches_inherited_execution(
    #[case] first_script: &str,
    #[case] expected: [&str; 3],
) -> TestResult {
    use acdc_execute::{CommandOutcome, CommandState, ExecError, ExecutionReport};

    let mut builder = CommandGraphBuilder::new();
    for (id, script, dependencies) in [
        ("first", first_script, vec![]),
        ("dependent", "exit 0", vec![CommandId::new("first")?]),
        ("independent", "exit 0", vec![]),
    ] {
        builder.add(
            CommandBlock::new(
                CommandId::new(id)?,
                script.to_owned(),
                None,
                SourceLocation::at_location(None, Location::default()),
            ),
            dependencies,
        );
    }
    let graph = builder.build()?;
    let legacy: ExecutionReport<'_> = graph.plan_all().execute(&ExecutionOptions::default());
    let recorded = graph
        .plan_all()
        .execute_with(
            &ExecutionOptions::default(),
            |_, process| process.status().map(Termination::from),
            |_| Ok::<(), Infallible>(()),
        )
        .map_err(|error| error.source)?;
    let legacy_names = legacy
        .outcomes()
        .iter()
        .map(|outcome| match outcome.state {
            CommandState::Succeeded => "succeeded",
            CommandState::Failed(_) => "failed",
            CommandState::Skipped(SkipReason::DependencyFailed { .. }) => "dependency",
            CommandState::Skipped(SkipReason::StoppedAfterFailure { .. }) => "stopped",
        })
        .collect::<Vec<_>>();
    assert_eq!(state_names(&recorded), expected);
    assert_eq!(state_names(&recorded), legacy_names);
    if let Some(CommandOutcome {
        state: CommandState::Failed(ExecError::Failed(status)),
        ..
    }) = legacy.outcomes().first()
    {
        assert!(matches!(recorded.outcomes().first(), Some(RunOutcome {
            state: OutcomeState::Failed(ProcessError::Failed(termination)), ..
        }) if *termination == Termination::from(*status)));
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn termination_preserves_windows_unsigned_exit_codes() {
    use std::os::windows::process::ExitStatusExt as _;

    let status = std::process::ExitStatus::from_raw(0xc000_013a);
    assert_eq!(Termination::from(status), Termination::Exited(0xc000_013a));
}
