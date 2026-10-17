#![cfg(all(unix, feature = "execute-recording"))]

use std::{
    fs,
    io::{self, Read, Write},
    os::fd::{AsRawFd, RawFd},
    path::Path,
    time::{Duration, Instant},
};

use filedescriptor::FileDescriptor;
use osc7501::{CapabilityReply, State, Status, write_query};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::Value;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

struct BorrowedMaster<'a> {
    _master: &'a dyn MasterPty,
    fd: RawFd,
}
impl AsRawFd for BorrowedMaster<'_> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

struct ChildGuard(Box<dyn Child + Send + Sync>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

struct Recorded {
    success: bool,
    visible: Vec<u8>,
    events: Vec<Value>,
    file: Vec<u8>,
}

fn run(
    source: &str,
    directory: &Path,
    flags: &[&str],
    action: impl FnMut(&[u8], &dyn MasterPty, &mut FileDescriptor) -> Result,
) -> Result<Recorded> {
    run_with_limit(source, directory, flags, None, action)
}

fn run_with_limit(
    source: &str,
    directory: &Path,
    flags: &[&str],
    limit: Option<usize>,
    mut action: impl FnMut(&[u8], &dyn MasterPty, &mut FileDescriptor) -> Result,
) -> Result<Recorded> {
    let document = directory.join("runbook.adoc");
    let recording = directory.join("run.cast");
    fs::write(&document, source)?;
    let pair = native_pty_system().openpty(PtySize {
        cols: 80,
        rows: 24,
        ..PtySize::default()
    })?;
    let before = pair
        .master
        .get_termios()
        .ok_or("missing terminal attributes")?;
    let fd = pair.master.as_raw_fd().ok_or("missing PTY descriptor")?;
    let mut terminal = FileDescriptor::dup(&BorrowedMaster {
        _master: &*pair.master,
        fd,
    })?;
    terminal.set_non_blocking(true)?;
    let mut command = if let Some(limit) = limit {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args([
            "-c",
            &format!("ulimit -f {limit}; exec \"$@\""),
            "record-test",
            env!("CARGO_BIN_EXE_acdc"),
        ]);
        command
    } else {
        CommandBuilder::new(env!("CARGO_BIN_EXE_acdc"))
    };
    command.args(["execute", "--record", "run.cast"]);
    command.arg(&document);
    command.args(flags);
    command.cwd(directory);
    command.env("TERM", "xterm-256color");
    command.env_remove("ACDC_LOG");
    let mut child = ChildGuard(pair.slave.spawn_command(command)?);
    let mut visible = Vec::new();
    let start = Instant::now();
    let mut buffer = [0; 16384];
    let status = loop {
        if start.elapsed() > Duration::from_secs(12) {
            return Err(format!("recording hung: {}", String::from_utf8_lossy(&visible)).into());
        }
        for _ in 0..16 {
            match terminal.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => visible.extend(buffer.get(..count).ok_or("buffer count")?),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.raw_os_error() == Some(5) => break,
                Err(error) => return Err(error.into()),
            }
        }
        action(&visible, &*pair.master, &mut terminal)?;
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    for _ in 0..16 {
        match terminal.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => visible.extend(buffer.get(..count).ok_or("buffer count")?),
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock || error.raw_os_error() == Some(5) =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut after = pair
        .master
        .get_termios()
        .ok_or("missing restored attributes")?;
    let mut before = before;
    // macOS can set PENDIN when canonical mode is restored.
    before
        .local_flags
        .remove(nix::sys::termios::LocalFlags::PENDIN);
    after
        .local_flags
        .remove(nix::sys::termios::LocalFlags::PENDIN);
    assert_eq!(before, after, "outer terminal mode was not restored");
    let file = fs::read(recording)?;
    let events = parsed_events(&file, status.success())?;
    Ok(Recorded {
        success: status.success(),
        visible,
        events,
        file,
    })
}

fn parsed_events(file: &[u8], success: bool) -> Result<Vec<Value>> {
    let lines = std::str::from_utf8(file)?
        .lines()
        .skip(1)
        .collect::<Vec<_>>();
    let mut events = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        match serde_json::from_str(line) {
            Ok(event) => events.push(event),
            Err(_) if !success && index + 1 == lines.len() && !file.ends_with(b"\n") => {
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(events)
}

fn command(id: &str, deps: &str, script: &str) -> String {
    format!("[.command,id={id}{deps}]\n----\n{script}\n----\n\n")
}
fn output(recording: &Recorded) -> String {
    recording
        .events
        .iter()
        .filter(|event| event.get(1).and_then(Value::as_str) == Some("o"))
        .filter_map(|event| event.get(2).and_then(Value::as_str))
        .collect()
}
fn metadata(recording: &Recorded) -> Result<Vec<Value>> {
    recording
        .events
        .iter()
        .filter(|event| event.get(1).and_then(Value::as_str) == Some("acdc.runbook"))
        .map(|event| {
            Ok(serde_json::from_str(
                event
                    .get(2)
                    .and_then(Value::as_str)
                    .ok_or("metadata absent")?,
            )?)
        })
        .collect()
}

#[test]
fn recording_preserves_command_policy_cwd_environment_and_output() -> Result {
    let directory = tempfile::tempdir()?;
    let source = command(
        "build",
        "",
        "printf 'out:%s\\n' \"$RECORD_VALUE\"; printf 'err\\n' >&2; pwd; exit 7",
    ) + &command("dependent", ",deps=build", "printf SHOULD-NOT-RUN")
        + &command("independent", "", "printf final-output");
    let recording = run(
        &source,
        directory.path(),
        &["--env", "RECORD_VALUE=first", "--env", "RECORD_VALUE=last"],
        |_, _, _| Ok(()),
    )?;
    assert!(!recording.success);
    let text = output(&recording);
    assert!(text.contains("out:last\r\nerr\r\n"));
    let expected_directory = directory.path().canonicalize()?;
    assert!(
        text.lines().any(|line| Path::new(line)
            .canonicalize()
            .is_ok_and(|path| path == expected_directory)),
        "{text}"
    );
    assert!(text.contains("final-output"));
    assert!(!text.contains("SHOULD-NOT-RUN"));
    let metadata = metadata(&recording)?;
    assert!(
        metadata
            .iter()
            .any(|event| event.pointer("/outcome/termination/code") == Some(&Value::from(7)))
    );
    assert!(metadata.iter().any(|event| {
        event
            .pointer("/outcome/reason/kind")
            .and_then(Value::as_str)
            == Some("dependency_failed")
    }));
    assert_eq!(
        metadata
            .last()
            .and_then(|event| event.get("result"))
            .and_then(Value::as_str),
        Some("failed")
    );
    assert!(!String::from_utf8_lossy(&recording.file).contains("RECORD_VALUE"));
    assert!(
        !recording
            .events
            .iter()
            .any(|event| event.get(1).and_then(Value::as_str) == Some("i"))
    );
    Ok(())
}

#[test]
fn recording_forwards_input_and_resize_without_input_events() -> Result {
    let directory = tempfile::tempdir()?;
    let source = command(
        "input",
        "",
        "stty -echo; printf ready; read answer; until test \"$(stty size)\" = '38 120'; do sleep 0.01; done; printf '\\nanswer:%s' \"$answer\"",
    );
    let mut sent = false;
    let recording = run(&source, directory.path(), &[], |visible, master, input| {
        if !sent && String::from_utf8_lossy(visible).contains("ready") {
            master.resize(PtySize {
                cols: 120,
                rows: 38,
                ..PtySize::default()
            })?;
            input.write_all(b"accepted\r")?;
            sent = true;
        }
        Ok(())
    })?;
    assert!(
        recording.success,
        "{}",
        String::from_utf8_lossy(&recording.visible)
    );
    assert!(output(&recording).contains("answer:accepted"));
    assert!(
        recording
            .events
            .iter()
            .any(|event| event.get(1).and_then(Value::as_str) == Some("r")
                && event.get(2).and_then(Value::as_str) == Some("120x38"))
    );
    assert!(
        !recording
            .events
            .iter()
            .any(|event| event.get(1).and_then(Value::as_str) == Some("i"))
    );
    Ok(())
}

fn shell_printf(bytes: &[u8]) -> Result<String> {
    use std::fmt::Write as _;
    let mut escaped = String::new();
    for byte in bytes {
        write!(escaped, "\\{byte:03o}")?;
    }
    Ok(format!("printf '{escaped}'"))
}

#[test]
fn a_probing_program_gets_one_reply_without_forwarding_its_query_to_the_outer_terminal() -> Result {
    for outer_supports_osc in [false, true] {
        let directory = tempfile::tempdir()?;
        let mut query = Vec::new();
        write_query(&mut query)?;
        let mut reply = Vec::new();
        CapabilityReply::new().write_to(&mut reply)?;
        let mut report = Vec::new();
        Status::new(State::Done)
            .title("Probe complete")
            .write_to(&mut report)?;
        let script = format!(
            "stty -echo -icanon min 1 time 0; {}; dd bs=1 count={} of=received 2>/dev/null; {} > expected; cmp -s received expected || exit 9; stty min 0 time 2; dd bs=1 count=1 of=extra 2>/dev/null; test ! -s extra || exit 10; printf probe-ok; {}",
            shell_printf(&query)?,
            reply.len(),
            shell_printf(&reply)?,
            shell_printf(&report)?
        );
        let source = command("probe", "", &script);
        let mut queried = false;
        let recording = run(&source, directory.path(), &[], |visible, _, input| {
            if outer_supports_osc
                && !queried
                && visible.windows(query.len()).any(|bytes| bytes == query)
            {
                input.write_all(&reply)?;
                queried = true;
            }
            Ok(())
        })?;
        assert!(
            recording.success,
            "{}",
            String::from_utf8_lossy(&recording.visible)
        );
        assert!(!queried);
        assert!(
            !recording
                .visible
                .windows(query.len())
                .any(|bytes| bytes == query)
        );
        assert!(
            recording
                .visible
                .windows(report.len())
                .any(|bytes| bytes == report)
        );
        let captured = output(&recording);
        assert!(captured.contains("probe-ok"));
        assert_eq!(
            captured.matches(std::str::from_utf8(&query)?).count(),
            1,
            "the raw query is retained; terminal input is not recorded"
        );
    }
    Ok(())
}

#[test]
fn interrupt_stops_later_commands_and_restores_the_outer_terminal() -> Result {
    let directory = tempfile::tempdir()?;
    let source = command("wait", "", "printf ready; while :; do sleep 1; done")
        + &command("later", "", "printf SHOULD-NOT-RUN");
    let mut sent = false;
    let recording = run(&source, directory.path(), &[], |visible, _, input| {
        if !sent && String::from_utf8_lossy(visible).contains("ready") {
            input.write_all(b"\x03")?;
            sent = true;
        }
        Ok(())
    })?;
    assert!(!recording.success);
    assert!(!output(&recording).contains("SHOULD-NOT-RUN"));
    let metadata = metadata(&recording)?;
    assert!(metadata.iter().any(|event| {
        event
            .pointer("/outcome/termination/kind")
            .and_then(Value::as_str)
            == Some("signaled")
    }));
    assert_eq!(
        metadata
            .last()
            .and_then(|event| event.get("result"))
            .and_then(Value::as_str),
        Some("aborted")
    );
    Ok(())
}

#[test]
fn final_buffered_output_is_retained_and_existing_recordings_are_not_overwritten() -> Result {
    let directory = tempfile::tempdir()?;
    let source = command(
        "tail",
        "",
        "i=0; while test $i -lt 10000; do printf 'line-%s\\n' \"$i\"; i=$((i+1)); done; printf FINAL",
    );
    let recording = run(&source, directory.path(), &[], |_, _, _| Ok(()))?;
    assert!(
        recording.success,
        "{}",
        String::from_utf8_lossy(&recording.visible)
    );
    assert!(output(&recording).contains("line-9999\r\nFINAL"));
    let second = run(
        &command("overwrite", "", "touch UNEXPECTED"),
        directory.path(),
        &[],
        |_, _, _| Ok(()),
    )?;
    assert!(!second.success);
    assert_eq!(recording.file, second.file);
    assert!(!directory.path().join("UNEXPECTED").exists());
    Ok(())
}

#[test]
fn disk_write_failure_stops_and_reaps_the_active_child_and_retains_a_prefix() -> Result {
    let directory = tempfile::tempdir()?;
    let source = command(
        "flood",
        "",
        "echo $$ > child.pid; trap '' HUP; while :; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; done",
    ) + &command("later", "", "touch UNEXPECTED");
    let recording = run_with_limit(&source, directory.path(), &[], Some(4), |_, _, _| Ok(()))?;
    assert!(!recording.success);
    assert!(!directory.path().join("UNEXPECTED").exists());
    assert!(recording.file.starts_with(b"{\"version\":3"));
    assert!(String::from_utf8_lossy(&recording.visible).contains("recording write failed"));
    let pid: i32 = fs::read_to_string(directory.path().join("child.pid"))?
        .trim()
        .parse()?;
    let pid = rustix::process::Pid::from_raw(pid).ok_or("invalid child pid")?;
    assert!(
        rustix::process::test_kill_process(pid).is_err(),
        "active child was not reaped"
    );
    Ok(())
}

#[test]
fn failed_spawn_and_invalid_cwd_do_not_start_later_commands() -> Result {
    for cwd in [false, true] {
        let directory = tempfile::tempdir()?;
        let first = if cwd {
            command("first", "", "touch UNEXPECTED")
        } else {
            "[source,/definitely/missing/acdc-interpreter,role=command,id=first]\n----\ntouch UNEXPECTED\n----\n\n".into()
        };
        let source = first + &command("later", "", "touch UNEXPECTED");
        let recording = run(
            &source,
            directory.path(),
            if cwd {
                &["--cwd", "/definitely/missing/acdc-directory"]
            } else {
                &[]
            },
            |_, _, _| Ok(()),
        )?;
        assert!(!recording.success);
        assert!(!directory.path().join("UNEXPECTED").exists());
        let events = metadata(&recording)?;
        assert!(
            events.iter().any(
                |event| event.pointer("/outcome/kind").and_then(Value::as_str) == Some("error")
            )
        );
        assert_eq!(
            events
                .last()
                .and_then(|event| event.pointer("/capture/kind"))
                .and_then(Value::as_str),
            Some("complete")
        );
    }
    Ok(())
}

#[test]
fn recorder_cancellation_restores_terminal_and_terminates_the_active_child() -> Result {
    let directory = tempfile::tempdir()?;
    let source = command(
        "cancel",
        "",
        "echo $$ > child.pid; kill -TERM \"$PPID\"; trap '' HUP; while :; do sleep 1; done",
    ) + &command("later", "", "touch UNEXPECTED");
    let recording = run(&source, directory.path(), &[], |_, _, _| Ok(()))?;
    assert!(!recording.success);
    assert!(!directory.path().join("UNEXPECTED").exists());
    let pid: i32 = fs::read_to_string(directory.path().join("child.pid"))?
        .trim()
        .parse()?;
    let pid = rustix::process::Pid::from_raw(pid).ok_or("invalid child pid")?;
    assert!(rustix::process::test_kill_process(pid).is_err());
    let events = metadata(&recording)?;
    assert_eq!(
        events
            .last()
            .and_then(|event| event.get("result"))
            .and_then(Value::as_str),
        Some("aborted")
    );
    assert_eq!(
        events
            .last()
            .and_then(|event| event.pointer("/capture/kind"))
            .and_then(Value::as_str),
        Some("incomplete")
    );
    Ok(())
}

#[test]
fn a_descendant_holding_the_pty_does_not_block_recording_completion() -> Result {
    let directory = tempfile::tempdir()?;
    let source = command(
        "descendant",
        "",
        "(trap '' HUP; sleep 30) & echo $! > descendant.pid; sleep 0.05; printf trailer; exit 0",
    );
    let started = Instant::now();
    let recording = run(&source, directory.path(), &[], |_, _, _| Ok(()))?;
    let elapsed = started.elapsed();
    let pid: i32 = fs::read_to_string(directory.path().join("descendant.pid"))?
        .trim()
        .parse()?;
    let pid = rustix::process::Pid::from_raw(pid).ok_or("invalid descendant pid")?;
    let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
    assert!(elapsed < Duration::from_secs(5));
    assert!(output(&recording).contains("trailer"));
    let events = metadata(&recording)?;
    assert!(
        events.iter().any(
            |event| event.pointer("/outcome/kind").and_then(Value::as_str) == Some("succeeded")
        )
    );
    #[cfg(target_os = "linux")]
    {
        assert!(!recording.success);
        assert_eq!(
            events
                .last()
                .and_then(|event| event.pointer("/capture/kind"))
                .and_then(Value::as_str),
            Some("incomplete")
        );
    }
    Ok(())
}
