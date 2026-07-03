# CLI — Developer Guide

## Features

`pre-spec-subs` (default-on) is forwarded to every selected converter (`acdc-converters-html?`, `acdc-converters-manpage?`, `acdc-converters-terminal?`), to `acdc-lint?`, and to `acdc-parser`. For the parser contract see `acdc-parser/AGENTS.md`; for converter plumbing see `converters/AGENTS.md`.

## TCK compliance

The CLI supports the AsciiDoc TCK (Test Compatibility Kit) behind a feature flag.

Build: `cargo build --package acdc-cli --features tck`

Manual test:
```bash
echo '{"contents":"= Hello","path":"test.adoc","type":"block"}' | cargo run -p acdc-cli --features tck -- tck
```
