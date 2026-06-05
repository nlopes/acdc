# acdc Development Workflow

## Project rules

- **Use nextest**: `cargo nextest run` for tests, `cargo test --doc` for doctests
- **Always `--all-features`**: all test/build/clippy commands
- **Clippy pedantic**: `cargo clippy --all-targets --all-features -- --deny clippy::pedantic`
- **Format before committing**: `cargo fmt --all`
- **Compact imports**: merge imports from the same crate/module into one `use` with braces, e.g. `use std::{borrow::Cow, io::Write};` — not separate `use std::borrow::Cow;` / `use std::io::Write;` lines
- **Update changelogs**: each crate has its own `CHANGELOG.md`; update `[Unreleased]` for affected crates
- **Surface converter warnings structurally**: user-relevant converter warnings should use `Warning` / `Diagnostics`, not `tracing::warn!`
- **Never use CLI for fixtures**: use the examples directly (CLI adds `last_updated` timestamps)
- **asciidoctor is reference**: when output differs, use `compare-asciidoc-output` agent

## Workspace features

`pre-spec-subs`, `setext`, and `network` are declared in `acdc-parser` and forwarded by every crate that consumes them, so a workspace `--no-default-features` build turns them off consistently. The rest are converter-local.

| Feature | Default | Crate | Notes |
|---------|---------|-------|-------|
| `pre-spec-subs` | on | parser (+ all converters) | `acdc-parser/AGENTS.md` (parser contract) + `converters/AGENTS.md` (plumbing & fixtures) |
| `setext` | on | parser | Setext (two-line underlined) headers |
| `network` | off | parser | Remote `include::https://...[]` (pulls in `ureq`) |
| `highlighting` | off | html, terminal | syntect source highlighting |
| `terminal` | off | html | Renders terminal previews into HTML; the cli exposes it as `html-terminal` |
| `render-state` | off | terminal | libghostty-vt grid rendering |
| `images` | off | terminal | Inline terminal image rendering (viuer) |

New code that gates parsing or rendering on a specific substitution belongs behind `pre-spec-subs`, not an ad-hoc cfg.

## Debugging

When tests fail, identify the category and follow the appropriate path:

- **Fixture mismatches** → run `regen-fixtures` skill (ask first)
- **Parser / grammar / preprocessor failures** → `acdc-parser/AGENTS.md`
- **Converter failures** → `converters/AGENTS.md`

## Versioning

All crates have **independent versions** — bump only crates that changed.

### Publish status

- **Published to crates.io**: `acdc-parser`
- **Not published**: `acdc-cli`, `acdc-lsp`, `acdc-converters-core`, `acdc-converters-html`, `acdc-converters-manpage`, `acdc-converters-markdown`, `acdc-converters-terminal`, `acdc-converters-dev`, `acdc-editor-wasm`

`acdc-cli` and `acdc-lsp` are distributed as binaries but we haven't built a pipeline to produce these as GitHub releases yet; `acdc-editor-wasm` ships via GitHub Release; the converters and `acdc-converters-dev` are internal workspace members only.

### Releasing acdc-editor-wasm

Released via GitHub Actions. Bump version in `Cargo.toml`, update changelog, commit, tag `acdc-editor-wasm-vX.Y.Z`, push.
