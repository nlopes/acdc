# acdc-runbook

Internal format support for recorded executable runbooks. It writes plain
asciicast v3 with ordinary terminal output, resize events, navigation markers,
and a final session exit. Versioned `acdc.runbook` extension events retain the
selected commands, source locations, and execution outcomes.

This crate does not run commands or interpret OSC reports. The execution library
owns command policy. The capture host owns PTYs and one monotonic clock. Replay
consumers read child OSC reports from ordinary output events.

```rust
use std::time::Duration;
use acdc_runbook::{Capture, Event, RunResult, Writer};

let mut bytes = Vec::new();
let mut recording = Writer::new(&mut bytes, 80, 24, Some("Deploy runbook"))?;
recording.metadata(Duration::ZERO, &Event::RunStarted { steps: Vec::new() })?;
recording.output(Duration::from_millis(100), "Ready\r\n")?;
recording.finish(Duration::from_millis(200), RunResult::Succeeded, Capture::Complete)?;
# Ok::<(), acdc_runbook::WriteError>(())
```

For a sample with a failed command and skipped dependent, run the `recording`
example with a new destination path. It writes deterministic sample events;
it does not execute the example commands.

## Wire contract

The [asciicast v3 specification](https://docs.asciinema.org/manual/asciicast/v3/)
defines the header and `[interval, code, data]` event lines. Every interval is
relative to the previous event, including metadata. Callers provide absolute
elapsed `Duration` values; the writer preserves equal-time call order and
rejects backward times. It writes full floating-point precision without
rounding each interval to milliseconds.

For an extension event, `code` is `acdc.runbook` and `data` is a JSON **string**:

```json
[0.0,"acdc.runbook","{\"version\":1,\"event\":\"command_started\",\"occurrence\":0}"]
```

Schema version 1 has four events:

- `run_started`: selected `steps` in plan order, including steps later skipped.
- `command_started`: the command `occurrence` key, before script preparation.
- `command_finished`: the same key and a structured `outcome`. Skips have a
  finish event and no start event.
- `run_finished`: execution `result` and capture health, as separate fields.

Each step has a recording-local numeric occurrence key. Its authored command
ID and child OSC IDs belong to separate namespaces. Dependencies refer to other
occurrence keys. Source paths are display text; readers must not open them or
execute commands. Authors should use source paths relative to the runbook.

`decode` retains unsupported versions and event names as explicit results.
Readers can report a diagnostic and continue ordinary terminal replay. Unknown
fields in known events are ignored. Consumers must validate occurrence
references and lifecycle ordering before showing a completed run.

The v3 reader delegates event syntax to `asciicast-rs`. It retains raw elapsed
times for **all** events, including unknown codes and input. Consumers must
keep input inert. The reader accepts plain v3 only; decompress first when
needed. Existing v2 replay readers can use `decode` for extension data.

## Partial recordings and limits

Create the destination with exclusive creation before executing any commands.
Do not overwrite an existing recording. Flush the writer at command boundaries.
An I/O failure makes it unusable; stop launching commands and retain the file.
`finish` writes completion, a final standard exit event, and a flush. The
standard exit is zero only when execution succeeded and capture was complete.
Dropping a writer emits no finish event. Confirm completion only after reading
valid lifecycle metadata and its matching final exit event without a read
error. A missing or damaged trailer means incomplete, even if an earlier
`run_finished` event was readable.

The reader yields events one at a time. A malformed or truncated final line
does not erase earlier events. It returns one error and then stops. A valid
final JSON event is accepted without a trailing newline. Consumers can retain
the prefix and report the read error as incomplete capture.

Each encoded line is limited to 1 MiB, including its newline. The reader bounds
storage before parsing. The writer bounds encoding before touching output, so
an oversized event leaves the existing prefix intact. Split large terminal
output into smaller events. PTY capture must retain incomplete UTF-8 sequences
between reads before passing text to the writer.

The writer records no input events, scripts, or environment overrides. Output
can still contain echoed input, secrets printed by a command, or OSC messages.
Terminal output is preserved as data; it is not safe to treat it as HTML.
