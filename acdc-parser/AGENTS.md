# Parser developer guide

## Architecture

- **PEG grammar** in `src/grammar/document/peg.rs`, `src/grammar/inlines/peg.rs`,
  and `src/grammar/inline_preprocessor/peg.rs`.
  Keep helpers used by one grammar in named sibling modules, such as
  `document/lists.rs` or `inlines/links.rs`. Keep shared helpers at the
  `src/grammar/` level. Put grammar tests in sibling `tests.rs` modules.
- Keep each `peg::parser!` declaration in a dedicated `peg.rs` child module.
  Put auxiliary functions, types, and tests outside that file. This also applies
  to the preprocessor grammars.
- **Two-pass inline markup** processing (see SDR-5 in `../ARCHITECTURE.adoc`)
  - Phase 1: Inline preprocessor — extracts passthroughs, expands attribute references
  - Phase 2: Inline parser — parses expanded text into inline node tree
- **Preprocessor** (`src/preprocessor/`) handles includes, conditionals, and document attributes before parsing.
- `ParseResult` owns the parsed document and its backing text. Public callers borrow through `document()`.

## Block substitutions

Both compatibility modes honor explicit `subs=` settings in every build. Draft
supports these controls as an acdc extension. Resolve groups and modifiers with
`SubstitutionSpec::resolve()`; keep structural decisions in the resolved plan.

The parser's `pre-spec-subs` feature is a compatibility alias with no effect.
Do not add parser control flow or test skips behind it. Use mode fixtures for
language differences. Supported settings do not warn. Unsupported entries use
`WarningKind::ContentRecovery` and preserve source; supported entries still apply.
STEM supports attribute expansion and output escaping, not formatted formulas.

Converter-side feature plumbing lives in `converters/AGENTS.md`.

`setext` and `network` are off by default in the parser crate. Setext headings also require `OptionsBuilder::with_setext()`; remote includes require caller-supplied `allow-uri-read` and a safe mode below Secure.

## Document attribute policy

- `document_attribute.rs` owns the registry, intrinsic initialization, typed configuration validation, and assignment policy.
- `DocumentAttributes` owns explicit values, effective values, presentation values, and assignment provenance.
- `Options` classifies API-supplied values as caller input and supplies the per-parse input context.
- The CLI parses assignment syntax, and converters may add processor defaults. Consumers use the parser's value and text views; they do not duplicate parser policy.
- Keep policy inputs and decisions internal. The public parser API exposes stable semantic and presentation views, not converter-specific or test-specific controls.
- `Document::attributes` is the end-of-header snapshot. Accepted body changes are ordered `Block::DocumentAttribute` nodes; use `assignment()` to distinguish a set value from an unset.
- When changing the policy, test document entries, parser `Options` and builder input, CLI `-a` input, locked and soft `@` assignments and unsets, and header/body exceptions. Compare both the official attribute documentation and the current asciidoctor implementation.

## Documentation

- Keep the shared API, feature, security, and migration sections in `README.md` and `README.adoc` consistent. Markdown is the published crate README; AsciiDoc also holds detailed syntax examples.
- Describe supported behavior without claiming full AsciiDoc or SDR-5 conformance. Keep deliberate Asciidoctor differences and fixed resource limits explicit.
- Keep `[Unreleased]` short and useful to library users. Group related fixes, omit internal cleanup, and list breaking API or JSON changes with their migration path. Public names belong here when callers must change them.
- Compare breaking changes with the last release tag, not with intermediate unreleased designs. Keep previous release entries intact.

## Debugging

- **Grammar failures**: use `trace-parse` if available, then check `src/grammar/`.
- **Preprocessor failures**: use `trace-parse <file> preprocessor` if available, or enable the preprocessor tracing module below for the focused test.

Trace module mapping (use with a focused nextest test):
- Test contains "inline"/"markup" → `acdc_parser::grammar::inline_preprocessor=trace`
- Test contains "preprocess"/"include" → `acdc_parser::preprocessor=trace`
- Default → `acdc_parser::grammar::document=trace`

## Fixtures

Put supporting include inputs in `fixtures/tests/includes/` with the `.adoc`
extension. Fixture discovery excludes that folder; test documents include them
using relative paths.

Ask before regenerating expected output. From the workspace root, select exact paths relative to `fixtures/tests`:
```bash
cargo run -p acdc-parser --example generate_parser_fixtures --all-features -- example.adoc
```

Every fixture runs in both compatibility modes. Use a shared expectation only when both modes agree. With no filenames, the generator processes all shared fixtures supported by the current build and fails if it encounters a paired case. Use `--pair` with exact filenames to create or update both mode expectations, or `--mode asciidoctor` / `--mode draft` to update one existing partner. Use `--diagnostics` to record warnings, recovery, and diagnostic locations; adding paired diagnostics also requires `--pair`. Record `git status --short` before running the generator and inspect every changed fixture afterward. Create `.adoc` inputs directly; use the generator for expected output. Filter nextest by the stable case name, such as `-E 'test(=inline_image::asciidoctor)'`. Do not use the CLI to generate fixtures.

`fixtures/preprocessor/` contains supporting scenarios, not automatically discovered tests. Each root input must be opened by a Rust test.

## Property tests

```bash
cargo nextest run -p acdc-parser --all-features -E 'test(/proptests/)'
PROPTEST_CASES=10000 cargo nextest run -p acdc-parser --all-features -E 'test(/proptests/)'
```

Regressions are tracked in `proptest-regressions/`.
