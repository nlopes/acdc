use std::{
    collections::VecDeque,
    io::{self, IsTerminal, Read, Write},
    os::fd::{AsRawFd, RawFd},
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use acdc_execute::Termination;
use filedescriptor::{FileDescriptor, POLLIN, POLLOUT, poll, pollfd};
use osc7501::CapabilityReply;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use rustix::{
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    process::{Pid, Signal, kill_process, kill_process_group},
};
use signal_hook::{
    SigId,
    consts::{SIGHUP, SIGINT, SIGTERM, SIGXFSZ},
};

use super::{filter::QueryFilter, session::Session};

const INPUT_LIMIT: usize = 64 * 1024;
const OUTPUT_LIMIT: usize = 1024 * 1024;
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

pub(super) fn preflight() -> io::Result<(u16, u16)> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "--record requires interactive terminal input and output",
        ));
    }
    let size = crossterm::terminal::size()?;
    if size.0 == 0 || size.1 == 0 {
        return Err(io::Error::other("terminal dimensions must be nonzero"));
    }
    Ok(size)
}

struct RawMode(bool);
impl RawMode {
    fn enter() -> io::Result<Self> {
        let owns_mode = !crossterm::terminal::is_raw_mode_enabled()?;
        if owns_mode {
            crossterm::terminal::enable_raw_mode()?;
        }
        Ok(Self(owns_mode))
    }
    fn restore(&mut self) -> io::Result<()> {
        if self.0 {
            crossterm::terminal::disable_raw_mode()?;
            self.0 = false;
        }
        Ok(())
    }
}
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

struct Nonblocking {
    output: FileDescriptor,
    previous: OFlags,
}
impl Nonblocking {
    fn new() -> io::Result<Self> {
        let output = FileDescriptor::dup(&io::stdout()).map_err(io::Error::other)?;
        let previous = fcntl_getfl(&output)?;
        fcntl_setfl(&output, previous | OFlags::NONBLOCK)?;
        Ok(Self { output, previous })
    }
}
impl Drop for Nonblocking {
    fn drop(&mut self) {
        let _ = fcntl_setfl(&self.output, self.previous);
    }
}

struct Signals(Vec<SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for signal in self.0.drain(..) {
            signal_hook::low_level::unregister(signal);
        }
    }
}

pub(super) struct Terminal {
    output: Nonblocking,
    raw: RawMode,
    _signals: Signals,
    pub cancellation: Arc<AtomicUsize>,
}
impl Terminal {
    pub fn enter() -> io::Result<Self> {
        let cancellation = Arc::new(AtomicUsize::new(0));
        let mut signals = Signals(Vec::new());
        for signal in [SIGINT, SIGTERM, SIGHUP, SIGXFSZ] {
            signals.0.push(signal_hook::flag::register_usize(
                signal,
                cancellation.clone(),
                usize::try_from(signal).map_err(io::Error::other)?,
            )?);
        }
        let raw = RawMode::enter()?;
        let output = Nonblocking::new()?;
        Ok(Self {
            output,
            raw,
            _signals: signals,
            cancellation,
        })
    }
    pub fn restore(&mut self) -> io::Result<()> {
        let flags = fcntl_setfl(&self.output.output, self.output.previous).map_err(io::Error::from);
        let raw = self.raw.restore();
        flags.and(raw)
    }
}

// The borrow keeps the master descriptor valid until duplication completes.
struct MasterFd<'a> {
    _master: &'a dyn MasterPty,
    descriptor: RawFd,
}
impl AsRawFd for MasterFd<'_> {
    fn as_raw_fd(&self) -> RawFd {
        self.descriptor
    }
}

struct Process {
    child: Box<dyn Child + Send + Sync>,
    master: Option<Box<dyn MasterPty + Send>>,
    io: Option<FileDescriptor>,
    reaped: bool,
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.reaped {
            if let Some(pid) = self
                .child
                .process_id()
                .and_then(|id| i32::try_from(id).ok())
                .and_then(Pid::from_raw)
            {
                let _ = kill_process_group(pid, Signal::KILL);
                let _ = kill_process(pid, Signal::KILL);
            } else {
                let _ = self.child.kill();
            }
            // Close both master handles before waiting: tty close can wait for output to drain.
            drop(self.io.take());
            drop(self.master.take());
            let _ = self.child.wait();
        }
    }
}

pub(super) fn run_process<W: Write>(
    process: &Command,
    session: &mut Session<W>,
    terminal: &mut Terminal,
) -> io::Result<Termination> {
    let program = Path::new(process.get_program());
    let program = if program.is_relative() && program.as_os_str().as_encoded_bytes().contains(&b'/')
    {
        std::env::current_dir()?.join(program)
    } else {
        program.to_owned()
    };
    let mut command = CommandBuilder::new(program);
    command.args(process.get_args());
    // portable-pty defaults to the home directory and ignores invalid cwd values.
    let directory = process
        .get_current_dir()
        .map_or_else(std::env::current_dir, |directory| Ok(directory.to_owned()))?
        .canonicalize()?;
    if !directory.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "child working directory is not a directory",
        ));
    }
    command.cwd(directory);
    for (key, value) in process.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    let size = crossterm::terminal::size()?;
    let pair = native_pty_system()
        .openpty(pty_size(size))
        .map_err(io::Error::other)?;
    let descriptor = pair
        .master
        .as_raw_fd()
        .ok_or_else(|| io::Error::other("PTY has no Unix descriptor"))?;
    let mut io = FileDescriptor::dup(&MasterFd {
        _master: &*pair.master,
        descriptor,
    })
    .map_err(io::Error::other)?;
    io.set_non_blocking(true).map_err(io::Error::other)?;
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(io::Error::other)?;
    let mut child = Process {
        child,
        master: Some(pair.master),
        io: Some(io),
        reaped: false,
    };
    drop(pair.slave);
    if session.size != size {
        session
            .writer
            .resize(session.elapsed(), size.0, size.1)
            .map_err(io::Error::other)?;
        session.size = size;
    }
    let result = drive(&mut child, session, terminal);
    if let Err(error) = &result {
        session.problem.get_or_insert_with(|| error.to_string());
    }
    result
}

fn pty_size((cols, rows): (u16, u16)) -> PtySize {
    PtySize {
        cols,
        rows,
        ..PtySize::default()
    }
}

struct Pump {
    input: VecDeque<u8>,
    output: VecDeque<u8>,
    filter: QueryFilter,
    reply: Vec<u8>,
    eof: bool,
    progress: Instant,
}

impl Pump {
    fn new() -> io::Result<Self> {
        let mut reply = Vec::new();
        CapabilityReply::new()
            .write_to(&mut reply)
            .map_err(io::Error::other)?;
        Ok(Self {
            input: VecDeque::new(),
            output: VecDeque::new(),
            filter: QueryFilter::default(),
            reply,
            eof: false,
            progress: Instant::now(),
        })
    }

    fn poll(
        &self,
        pty: &FileDescriptor,
        input: &impl AsRawFd,
        output: &impl AsRawFd,
        active: bool,
    ) -> io::Result<(bool, bool)> {
        let mut descriptors = [
            pollfd {
                fd: pty.as_raw_fd(),
                events: if !self.eof
                    && self.output.len() < OUTPUT_LIMIT - 4096 * 16 - osc7501::parser::MAX_SEQUENCE
                {
                    POLLIN
                } else {
                    0
                } | if !self.input.is_empty() && active {
                    POLLOUT
                } else {
                    0
                },
                revents: 0,
            },
            pollfd {
                fd: input.as_raw_fd(),
                events: if active && self.input.len() < INPUT_LIMIT - 4096 {
                    POLLIN
                } else {
                    0
                },
                revents: 0,
            },
            pollfd {
                fd: output.as_raw_fd(),
                events: if self.output.is_empty() { 0 } else { POLLOUT },
                revents: 0,
            },
        ];
        if let Err(error) = poll(&mut descriptors, Some(Duration::from_millis(10))) {
            if matches!(&error, filedescriptor::Error::Poll(source) if source.kind() == io::ErrorKind::Interrupted)
            {
                return Ok((false, false));
            }
            return Err(io::Error::other(error));
        }
        Ok((
            descriptors
                .first()
                .is_some_and(|fd| fd.revents != 0 && fd.events & POLLIN != 0),
            descriptors
                .get(1)
                .is_some_and(|fd| fd.revents & POLLIN != 0),
        ))
    }

    fn read<W: Write>(
        &mut self,
        pty: &mut FileDescriptor,
        session: &mut Session<W>,
    ) -> io::Result<()> {
        let mut buffer = [0; 4096];
        // Bound each turn so continuous output cannot starve cancellation checks.
        for _ in 0..16 {
            match pty.read(&mut buffer) {
                Ok(0) => {
                    self.eof = true;
                    break;
                }
                Ok(count) => {
                    let bytes = buffer
                        .get(..count)
                        .ok_or_else(|| io::Error::other("PTY read exceeded its buffer"))?;
                    session.output(bytes, false)?;
                    let mut forwarded = Vec::new();
                    let queries = self.filter.push(bytes, &mut forwarded);
                    self.output.extend(forwarded);
                    for _ in 0..queries {
                        if self.input.len() + self.reply.len() > INPUT_LIMIT {
                            return Err(io::Error::other(
                                "OSC query replies exceeded the PTY input limit",
                            ));
                        }
                        self.input.extend(&self.reply);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.raw_os_error() == Some(5) => {
                    self.eof = true;
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => break,
                Err(error) => return Err(error),
            }
        }
        if self.eof {
            let mut tail = Vec::new();
            self.filter.finish(&mut tail);
            self.output.extend(tail);
            session.output(&[], true)?;
        }
        Ok(())
    }

    fn forward_input(
        &mut self,
        stdin: &mut impl Read,
        pty: &mut FileDescriptor,
        ready: bool,
        active: bool,
    ) -> io::Result<()> {
        let mut buffer = [0; 4096];
        if ready {
            match stdin.read(&mut buffer) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "terminal input closed",
                    ));
                }
                Ok(count) => {
                    if self.input.len() + count > INPUT_LIMIT {
                        return Err(io::Error::other("PTY input exceeded its queue limit"));
                    }
                    self.input.extend(
                        buffer
                            .get(..count)
                            .ok_or_else(|| io::Error::other("input read exceeded its buffer"))?,
                    );
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        if !self.input.is_empty() && active {
            match pty.write(self.input.as_slices().0) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "PTY input closed")),
                Ok(count) => {
                    self.input.drain(..count);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn flush(&mut self, terminal: &mut impl Write) -> io::Result<()> {
        for _ in 0..16 {
            if self.output.is_empty() {
                break;
            }
            match terminal.write(self.output.as_slices().0) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "terminal output closed",
                    ));
                }
                Ok(count) => {
                    self.output.drain(..count);
                    self.progress = Instant::now();
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

fn drive<W: Write>(
    process: &mut Process,
    session: &mut Session<W>,
    terminal: &mut Terminal,
) -> io::Result<Termination> {
    let Process {
        child,
        master,
        io,
        reaped,
    } = process;
    let master = master
        .as_deref()
        .ok_or_else(|| io::Error::other("PTY master was closed"))?;
    let pty = io
        .as_mut()
        .ok_or_else(|| io::Error::other("PTY I/O was closed"))?;
    let mut stdin = io::stdin();
    let mut pump = Pump::new()?;
    let mut status = None;
    let mut deadline = None;
    loop {
        if terminal.cancellation.load(Ordering::SeqCst) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "recording cancelled",
            ));
        }
        if status.is_none()
            && let Some(exited) = child.try_wait()?
        {
            *reaped = true;
            status = Some(exited);
            deadline = Some(Instant::now() + DRAIN_TIMEOUT);
        }
        if status.is_some() && pump.eof && pump.output.is_empty() {
            break;
        }
        if deadline.is_some_and(|at| Instant::now() >= at)
            && (!pump.eof || pump.progress.elapsed() >= DRAIN_TIMEOUT)
        {
            session.problem.get_or_insert_with(|| {
                if pump.eof {
                    "outer terminal stopped accepting output"
                } else {
                    "PTY output did not reach EOF after child exit"
                }
                .into()
            });
            session.output(&[], true)?;
            break;
        }
        let resized = crossterm::terminal::size()?;
        if session.size != resized && status.is_none() {
            master.resize(pty_size(resized)).map_err(io::Error::other)?;
            session
                .writer
                .resize(session.elapsed(), resized.0, resized.1)
                .map_err(io::Error::other)?;
            session.size = resized;
        }
        let (readable, input_ready) =
            pump.poll(pty, &stdin, &terminal.output.output, status.is_none())?;
        if readable && !pump.eof {
            pump.read(pty, session)?;
        }
        pump.forward_input(&mut stdin, pty, input_ready, status.is_none())?;
        pump.flush(&mut terminal.output.output)?;
    }
    let status = status.ok_or_else(|| io::Error::other("child status unavailable"))?;
    Ok(status.signal().map_or_else(
        || Termination::Exited(status.exit_code()),
        |signal| Termination::Signaled(signal.to_owned()),
    ))
}
