# acdc Development Workflow

## Project rules

- **Use nextest**: `cargo nextest run` for tests, `cargo test --doc` for doctests
- **Product spelling**: always write the product name as `acdc`, lowercase. Preserve other casing only in exact external quotations or case-sensitive identifiers and test inputs.
- **Worktree ownership**: inspect `git status --short` before editing. Preserve unrelated and staged work, and do not stage or commit unless the user explicitly asks.
- **Feature coverage**: use `--all-features` for standard test/build/clippy commands. When a task changes supported feature-off behavior, also run the applicable documented `--no-default-features` configuration.
- **Clippy pedantic**: `cargo clippy --all-targets --all-features -- --deny clippy::pedantic --deny clippy::todo`
- **Format before committing**: `cargo fmt --all`
- **Compact imports**: merge imports from the same crate/module into one `use` with braces, e.g. `use std::{borrow::Cow, io::Write};` — not separate `use std::borrow::Cow;` / `use std::io::Write;` lines
- **Update changelogs**: each crate has its own `CHANGELOG.md`; update `[Unreleased]` for affected crates. Entries describe what a user sees or is affected by — the new behavior, the attribute/option to reach it, and any divergence from `asciidoctor`. Never regurgitate internal mechanics (function/field names, struct changes, control flow); those belong in code/commits, not the changelog.
- **Surface converter warnings structurally**: user-relevant converter warnings should use `Warning` / `Diagnostics`, not `tracing::warn!`
- **Never use CLI for fixtures**: use the examples directly (CLI adds `last_updated` timestamps)
- **asciidoctor is reference**: compare one source using the built `acdc` CLI and the matching asciidoctor backend first. Compare observable behavior, not byte-identical output, and avoid temporary Rust harnesses. Use the `compare-asciidoc-output` agent only after a direct comparison confirms a divergence or deeper research is needed. For PDF comparisons, keep clearly named `*-acdc.pdf` and `*-asciidoctor.pdf` outputs; do not create raster previews unless the user asks.

## Validation workflow

- Never run Cargo commands concurrently against the same target directory.
- During implementation, run the smallest relevant package, test, or stable nextest expression, such as `-E 'test(/name/)'` rather than a generated fixture number.
- Use `--all-features` for standard validation. When changing feature-off behavior, also run the applicable documented `--no-default-features` checks.
- At a cross-crate, public-API, checklist, or commit boundary, run:

  ```console
  cargo fmt --all -- --check
  cargo nextest run --workspace --all-features
  cargo test --doc --workspace --all-features
  cargo clippy --all-targets --all-features -- --deny clippy::pedantic --deny clippy::todo
  git diff --check
  ```

- Report whether a broad command failed during compilation or after tests began.
- Do not rerun Rust checks after a documentation-only wording change when the relevant code checks have already passed.

## Workspace features

`pre-spec-subs`, `setext`, and `network` are declared in `acdc-parser` and forwarded by every crate that consumes them, so a workspace `--no-default-features` build turns them off consistently. The rest are converter-local.

| Feature | Default | Crate | Notes |
|---------|---------|-------|-------|
| `pre-spec-subs` | on | parser (+ all converters and lint) | `acdc-parser/AGENTS.md` (parser contract) + `converters/AGENTS.md` (converter plumbing & fixtures) |
| `setext` | on | parser | Setext (two-line underlined) headers |
| `network` | off | parser | Remote `include::https://...[]` (pulls in `ureq`) |
| `highlighting` | off | html, terminal | syntect source highlighting |
| `terminal` | off | html | Renders terminal previews into HTML; the cli exposes it as `html-terminal` |
| `emulator` | off | terminal | Runs terminal output through a libghostty-vt terminal emulator and captures the rendered screen grid (static previews + session replays); the cli exposes it as `terminal-emulator` |
| `images` | off | terminal | Inline terminal image rendering (viuer) |

New code that gates parsing or rendering on a specific substitution belongs behind `pre-spec-subs`, not an ad-hoc cfg.

## Debugging

When tests fail, identify the category and follow the appropriate path:

- **Fixture mismatches** → run the `regen-fixtures` skill (ask first). If the skill is unavailable, ask before using the documented scoped generator.
- **Parser / grammar / preprocessor failures** → `acdc-parser/AGENTS.md`
- **Converter failures** → `converters/AGENTS.md`

## Benchmarks

`acdc-parser` has two Criterion benches. `parser_bench` (string-parse hot paths) runs
under a bare `cargo bench`. `f1_include_bench` (include / partial-include parsing) is
**disabled by default** (`bench`/`test` = false) so CI and `cargo bench`/`cargo test`
skip it — it writes temp fixtures and takes minutes. Run it on demand when touching the
include/preprocessor/remap paths: `cargo bench --bench f1_include_bench`.

Beware machine drift: a single before/after run on this hardware shows a uniform few-percent
shift (confirmed via a before-vs-before run) that swamps small real deltas. For trustworthy
numbers use a **paired/alternating** run (alternate the old and new binaries back-to-back
several times and compare adjacent pairs), with an untouched benchmark as a codegen-bias control.

## Versioning

All crates have **independent versions** — bump only crates that changed.

### Publish status

- **Published to crates.io**: `acdc-parser`
- **Not published**: `acdc-cli`, `acdc-lint`, `acdc-lsp`, `acdc-converters-core`, `acdc-converters-html`, `acdc-converters-manpage`, `acdc-converters-markdown`, `acdc-converters-terminal`, `acdc-converters-dev`, `acdc-editor-wasm`

`acdc-cli` and `acdc-lsp` are distributed as binaries but we haven't built a pipeline to produce these as GitHub releases yet; `acdc-editor-wasm` ships via GitHub Release; the converters and `acdc-converters-dev` are internal workspace members only.

### Releasing acdc-editor-wasm

Released via GitHub Actions. Bump version in `Cargo.toml`, update changelog, commit, tag `acdc-editor-wasm-vX.Y.Z`, push.
