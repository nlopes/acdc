# CLI — Developer Guide

## Features

`pre-spec-subs` (default-on) is forwarded to every selected converter (`acdc-converters-html?`, `acdc-converters-manpage?`, `acdc-converters-pdf?`, `acdc-converters-terminal?`), to `acdc-lint?`, and to `acdc-parser`. For the parser contract see `acdc-parser/AGENTS.md`; for converter plumbing see `converters/AGENTS.md`.

`network` is forwarded to `acdc-converters-pdf?` as well as the parser and lint crate, so PDF builds can resolve remote images and logos under the converter's network policy.

`setext` is default-on as a build feature and exposes the runtime `--setext` compatibility flag. `highlighting` forwards to whichever of the HTML and terminal backends are selected without enabling either backend itself.

## TCK compliance

The CLI supports the AsciiDoc TCK (Test Compatibility Kit) behind a feature flag.

Build: `cargo build --package acdc-cli --no-default-features --features tck`

Manual test:
```bash
echo '{"contents":"= Hello","path":"test.adoc","type":"block"}' | cargo run -p acdc-cli --no-default-features --features tck -- tck
```
