# acdc Development Workflow

## Project rules

- **Use nextest**: `cargo nextest run` for tests, `cargo test --doc` for doctests
- **Product spelling**: always write the product name as `acdc`, lowercase. Preserve other casing only in exact external quotations or case-sensitive identifiers and test inputs.
- **Language**: use American English in documentation, comments, and diagnostics. Preserve identifiers, test data, and quoted external text.
- **Worktree ownership**: inspect `git status --short` before editing. Preserve unrelated and staged work, and do not stage or commit unless the user explicitly asks.
- **Feature coverage**: use `--all-features` for standard test/build/clippy commands. When a task changes supported feature-off behavior, also run the applicable documented `--no-default-features` configuration.
- **Clippy pedantic**: `cargo clippy --all-targets --all-features -- --deny clippy::pedantic --deny clippy::todo`
- **Format before committing**: `cargo fmt --all`
- **Compact imports**: merge imports from the same crate/module into one `use` with braces, e.g. `use std::{borrow::Cow, io::Write};` — not separate `use std::borrow::Cow;` / `use std::io::Write;` lines
- **Update changelogs**: each crate has its own `CHANGELOG.md`; update `[Unreleased]` for affected crates. Entries describe what a user sees or is affected by — the new behavior, the attribute/option to reach it, and any divergence from `asciidoctor`. Never regurgitate internal mechanics (function/field names, struct changes, control flow); those belong in code/commits, not the changelog.
- **Surface converter warnings structurally**: user-relevant converter warnings should use `Warning` / `Diagnostics`, not `tracing::warn!`
- **Fixture expectations**: generate new expectations without asking. Ask before regenerating or changing existing expectations. Use scoped generators and review every generated file.
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

Language behavior follows `CompatibilityMode`, not Cargo features. Both modes
and explicit `subs=` settings are available in every build. The former
`pre-spec-subs` and `setext` features are removed; do not add them to commands or
dependency declarations. Setext headings are supported in Asciidoctor mode.

Cargo features select optional capabilities. Defaults below refer to the owning
crate; check the CLI manifest for backend and tool selection.

| Feature | Default | Crate | Notes |
|---------|---------|-------|-------|
| `asg` | off | parser, CLI | Checked upstream and named acdc ASG export; independent of compatibility mode |
| `tck` | off | CLI | Fixed Draft-mode TCK adapter; enables `asg` |
| `network` | off | parser, html, pdf | Remote includes and HTML/PDF image access; caller permission and safe mode still apply |
| `highlighting` | off | html, terminal | giallo source highlighting |
| `terminal` | off | html | Renders terminal previews into HTML; the cli exposes it as `html-terminal` |
| `emulator` | off | terminal | Runs terminal output through a libghostty-vt terminal emulator and captures the rendered screen grid (static previews + session replays); the cli exposes it as `terminal-emulator` |
| `images` | off | terminal | Inline terminal image rendering (viuer) |

Converters render resolved parser semantics and apply output escaping or retained
raw-text substitutions. Do not reintroduce feature gates for language behavior.

## Debugging

When tests fail, identify the category and follow the appropriate path:

- **Fixture mismatches** → get approval before changing existing expectations, then use the `regen-fixtures` skill or the documented scoped generator.
- **Parser / grammar / preprocessor failures** → `acdc-parser/AGENTS.md`
- **Converter failures** → `converters/AGENTS.md`

## Benchmarks

`acdc-parser` has three Criterion benches. `parser_bench` (string-parse hot paths) runs
under a bare `cargo bench`. `f1_include_bench` (include / partial-include parsing) is
**disabled by default** (`bench`/`test` = false) so CI and `cargo bench`/`cargo test`
skip it — it writes temp fixtures and takes minutes. Run it on demand when touching the
include/preprocessor/remap paths: `cargo bench --bench f1_include_bench`.
`conditional_bench` is also disabled by default. Run it on demand with
`cargo bench -p acdc-parser --bench conditional_bench --all-features`.

Beware machine drift: a single before/after run on this hardware shows a uniform few-percent
shift (confirmed via a before-vs-before run) that swamps small real deltas. For trustworthy
numbers use a **paired/alternating** run (alternate the old and new binaries back-to-back
several times and compare adjacent pairs), with an untouched benchmark as a codegen-bias control.

## Versioning

All crates have **independent versions** — bump only crates that changed.

### Publish status

The only crate we publish in crates.io is `acdc-parser`. All other crates in this repository are not published.

`acdc-lsp` and `acdc-cli` have separate native release workflows that test five targets and create draft GitHub releases for matching `acdc-lsp-vX.Y.Z` and `acdc-cli-vX.Y.Z` tags. CLI binaries include all features. Manual runs upload artifacts only. Review and publish each draft separately. `acdc-editor-wasm` ships via GitHub Release; the converters and `acdc-converters-dev` are internal workspace members only.

### Releasing acdc-editor-wasm

Released via GitHub Actions. Bump version in `Cargo.toml`, update changelog, commit, tag `acdc-editor-wasm-vX.Y.Z`, push.
