# acdc Development Workflow

## Project rules

- **Use nextest**: `cargo nextest run` for tests, `cargo test --doc` for doctests
- **Always `--all-features`**: all test/build/clippy commands
- **Clippy pedantic**: `cargo clippy --all-targets --all-features -- --deny clippy::pedantic`
- **Format before committing**: `cargo fmt --all`
- **Update changelogs**: each crate has its own `CHANGELOG.md`; update `[Unreleased]` for affected crates
- **Never use CLI for fixtures**: use the examples directly (CLI adds `last_updated` timestamps)
- **asciidoctor is reference**: when output differs, use `compare-asciidoc-output` agent

## Debugging

When tests fail, identify the category and follow the appropriate path:

- **Fixture mismatches** → run `regen-fixtures` skill (ask first)
- **Parser/grammar failures** → see `acdc-parser/AGENTS.md`
- **Converter failures** → see `converters/AGENTS.md`
- **Preprocessor failures** → see `acdc-parser/AGENTS.md`

## Versioning

All crates have **independent versions** — bump only crates that changed.

Crates: `acdc-parser`, `acdc-cli`, `acdc-lsp`, `acdc-converters-core`, `acdc-converters-html`, `acdc-converters-manpage`, `acdc-converters-terminal`, `acdc-converters-dev` (not published), `acdc-editor-wasm` (GitHub Release, not crates.io).

### Releasing acdc-editor-wasm

Released via GitHub Actions. Bump version in `Cargo.toml`, update changelog, commit, tag `acdc-editor-wasm-vX.Y.Z`, push.
