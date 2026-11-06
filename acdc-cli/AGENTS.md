# CLI — Developer Guide

## Features

`--compatibility-mode asciidoctor|draft` selects language behavior. Both modes
and explicit `subs=` settings work in every build. The former `pre-spec-subs`
and `setext` Cargo features and `--setext` flag are removed. Asciidoctor mode
supports Setext headings. See `acdc-parser/AGENTS.md` and `converters/AGENTS.md`.

`network` enables parser remote includes and forwards to selected execute, lint,
HTML and PDF crates. HTML/PDF image access still follows the converter's network
policy, caller permission and safe mode.

`highlighting` forwards to selected HTML and terminal backends without enabling
either backend itself. `asg` enables checked upstream and named acdc exports.
`tck` enables `asg` and the fixed Draft-mode TCK adapter. Native JSON is available
without either export feature.

## Attribute overrides

- The CLI owns parsing of `-a` syntax only. Parser `Options` owns protected and caller-locked attribute decisions.
- Preserve the distinction between locked defaults and soft `@` assignments and unsets.
- Do not add CLI-only lock exceptions.
- Changes require CLI tests plus parser policy fixture and API tests.

## TCK compliance

The CLI supports the AsciiDoc TCK (Test Compatibility Kit) behind a feature flag.

Build: `cargo build --package acdc-cli --no-default-features --features tck`

Manual test:
```bash
echo '{"contents":"= Hello","path":"test.adoc","type":"block"}' | cargo run -p acdc-cli --no-default-features --features tck -- tck
```
