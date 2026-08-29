# Converters — Developer Guide

## Architecture

All converters implement the `Visitor` trait from `core/src/visitor.rs`.

Shared utilities in `core/`:
- `substitutions.rs` — typography replacements, escape handling
- `section.rs` — section numbering and hierarchy
- `table.rs` — table layout calculations (colspan/rowspan)
- `code.rs` — syntax highlighting integration
- `toc.rs` — table of contents generation

## `subs=` plumbing (the `pre-spec-subs` feature)

- `acdc-converters-core::substitutions::SubsFlags`, `effective_subs`,
  `effective_subs_flags`, and `apply_replacements` only exist when the
  `pre-spec-subs` feature is enabled (default-on). When the feature is off,
  `[subs="…"]` is silently ignored at the parser layer and converters fall
  back to the asciidoctor default (all substitutions on).
- The markdown converter does **not** honour `subs=` and intentionally has
  no `pre-spec-subs` feature in its `Cargo.toml`. Don't treat that as a
  regression to fix — markdown's output model doesn't need it.
- **HTML** uses a separate `Vec<Substitution>` path returned by
  `effective_subs(...)` at shallow block sites (`html/src/html_visitor.rs`).
- **Terminal and manpage** carry an `Rc<Cell<SubsFlags>>` on their `Processor`
  for the deep inline hot path. The `current_subs` field, its initialisers,
  and the paragraph snapshot/restore are all `#[cfg(feature = "pre-spec-subs")]`.
  The PlainText arm calls a small `transform_plain` helper that has two
  definitions (feature on/off) so the call site stays clean.
- **When adding a new converter that honours `subs=`**: forward the feature in
  `Cargo.toml` (`pre-spec-subs = ["acdc-parser/pre-spec-subs", "acdc-converters-core/pre-spec-subs"]`),
  follow the manpage shape (snapshot/restore in `render_paragraph`, query
  `current_subs.get()` in the PlainText arm), and gate both sides behind the
  feature.
- **Fixtures whose expected output depends on `subs=`** must include `subs`
  in their stem (convention: `subs_…`). Under `--no-default-features`:
  - The **html** and **manpage** harnesses use rstest glob discovery and
    early-return in their `run_*_fixture` helper when
    `file_name.contains("subs")`.
  - The **terminal** harness uses an explicit `generate_tests!` macro list,
    not glob discovery. Subs fixtures must be registered in that list
    explicitly, and `test_fixture` has the matching `.contains("subs")`
    early-return.

## Test placement

- Use source and expected-output fixtures for rendered HTML, text, Typst, and other snapshot-like converter output.
- Use integration tests for properties that snapshots cannot prove, including structured diagnostics, PDF annotations, PDF objects and metadata, warnings, and end-to-end behavior.

## Debugging

- Compare the same source with the built `acdc` CLI and the matching asciidoctor backend before using the `compare-asciidoc-output` agent. Compare observable behavior rather than byte-identical output.
- For fixture mismatches, run `regen-fixtures` skill (ask first)

## Fixture regeneration

- Get approval before regenerating fixtures.
- Record `git status --short` before regeneration.
- Run one generator at a time. Each generator may rewrite its full fixture corpus.
- Inspect status and the diff immediately afterward. Keep only intended changes or explicitly approved new baselines, and reject environment-only changes.

```bash
cargo run -p acdc-converters-html --example generate_html_fixtures --all-features
cargo run -p acdc-converters-terminal --example generate_terminal_fixtures --all-features
cargo run -p acdc-converters-manpage --example generate_manpage_fixtures --all-features
cargo run -p acdc-converters-pdf --example generate_typst_fixtures --all-features

# Markdown uses a shell script that drives the CLI (no example binary):
bash converters/markdown/tests/regenerate_expected.sh
# Regenerate a single fixture:
bash converters/markdown/tests/regenerate_expected.sh <fixture_name>
```

### Terminal capability fixtures

- The terminal fixture macro's boolean argument controls whether the test reads `.osc8.txt`. When it is `true`, the generic harness skips the fixture for terminals without OSC 8 support.
- A plain `.txt` file for that same fixture is unused unless a separate no-OSC8 assertion reads it.
- If both `.txt` and `.osc8.txt` are retained, add explicit coverage for both outputs.
- Test terminal capability-dependent behavior with both `TERM=dumb` and `TERM=xterm-ghostty`.
