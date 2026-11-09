# Converters — Developer Guide

## Architecture

Shared traversal uses the typed `DocumentVisitor`, `BlockVisitor` and
`InlineVisitor` traits exported by `core/src/visitor.rs`.

Shared utilities in `core/`:
- `substitutions.rs` — typography replacements, escape handling
- `section.rs` — section numbering and hierarchy
- `table.rs` — table layout calculations (colspan/rowspan)
- `code.rs` — syntax highlighting integration
- `toc.rs` — table of contents generation

## Resolved substitutions

- The parser applies structural substitutions in every build. Converters read
  resolved content and apply output escaping or the substitutions retained on raw
  text. Do not reparse ordinary text or repeat resolved typography replacements.
- `acdc-converters-core::substitutions::effective_subs` resolves block settings
  against normal or verbatim defaults. Raw fragments retain their own ordered
  settings; keep their treatment separate from ordinary body text.
- The former `pre-spec-subs` feature, `SubsFlags` and feature-dependent converter
  paths are removed. Do not restore them or add a Markdown substitution feature.
- Substitution fixtures run with and without default features. Use `subs_…`
  stems for these cases, but never skip them based on the removed feature.
  Terminal fixtures still need entries in `terminal/tests/fixtures/catalog.rs`.
- Markdown uses `.md` expectations in every build. Existing `.no-subs.md` files
  are historical migration baselines; do not generate new variants.

## Test placement

- Put supporting include inputs in an `includes/` folder beside the source fixtures, using `.adoc`. Fixture discovery and generation exclude that folder; test documents include the inputs using relative paths.
- Share identical, location-independent source fixtures through relative symbolic
  links, following the existing fixture convention. Edit the link target to update
  shared content. Keep inputs that use includes, local resources, or document-path
  attributes beside their own supporting files. Each backend and variant keeps its
  own case selection and expected output.
- Use source and expected-output fixtures for rendered HTML, text, Typst, and other snapshot-like converter output.
- Use integration tests for properties that snapshots cannot prove, including structured diagnostics, PDF annotations, PDF objects and metadata, warnings, and end-to-end behavior.

## Debugging

- Compare the same source with the built `acdc` CLI and the matching asciidoctor backend before using the `compare-asciidoc-output` agent. Compare observable behavior rather than byte-identical output.
- For fixture mismatches, get approval before changing existing expectations, then use the `regen-fixtures` skill or the documented scoped generator.

## Fixture regeneration

- Each backend's `tests/support/mod.rs` owns fixture parsing and render configuration.
  Both its integration harness and generator example use this adapter. Keep safe
  mode, processor defaults, source paths, variant selection, and diagnostics there;
  keep snapshot comparisons and semantic assertions in the tests.
- Shared discovery and ordinary fixture parsing live in
  `acdc_converters_dev::fixtures`. `FixtureGenerator` passes each source path to
  the backend adapter, so backend defaults are applied before parsing when needed.
  CommonMark and GFM fixtures both use Unsafe mode, matching their generator.
- Generate new expectations without asking. Get approval before regenerating or changing existing expectations.
- Record `git status --short` before regeneration.
- Run one generator at a time. Each generator may rewrite its full fixture corpus.
- Inspect status and the diff immediately afterward. Keep only intended changes or explicitly approved new baselines, and reject environment-only changes.

```bash
cargo run -p acdc-converters-html --example generate_html_fixtures --all-features
cargo run -p acdc-converters-terminal --example generate_terminal_fixtures --all-features
cargo run -p acdc-converters-manpage --example generate_manpage_fixtures --all-features
cargo run -p acdc-converters-markdown --example generate_markdown_fixtures --all-features
cargo run -p acdc-converters-pdf --example generate_typst_fixtures --all-features

# Regenerate a single Markdown fixture:
cargo run -p acdc-converters-markdown --example generate_markdown_fixtures --all-features -- <fixture_name>
```

### Terminal capability fixtures

- Register terminal fixtures and their OSC 8 variants in
  `terminal/tests/fixtures/catalog.rs`.
- The terminal generator always writes `.txt` fixtures with OSC 8 disabled,
  then writes `.osc8.txt` only for fixtures marked with an OSC 8 variant.
- The terminal fixture harness pins capabilities instead of reading `TERM`. It
  checks the plain variant for every fixture and both variants for each fixture
  marked with an OSC 8 variant.
- Do not copy or edit OSC 8 expected files by hand; use the terminal fixture
  generator. Get approval before changing existing expectations.
