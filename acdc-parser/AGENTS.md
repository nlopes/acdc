# Parser developer guide

## Architecture

- **PEG grammar** in `src/grammar/` — `document.rs` is the main entry point
- **Two-pass inline markup** processing (see SDR-5 in `../ARCHITECTURE.adoc`)
  - Phase 1: Inline preprocessor — extracts passthroughs, expands attribute references
  - Phase 2: Inline parser — parses expanded text into inline node tree
- **Preprocessor** (`src/preprocessor/`) handles includes, conditionals, and document attributes before parsing.
- `ParseResult` owns the parsed document and its backing text. Public callers borrow through `document()`.

## `pre-spec-subs` — parser contract

The default-on `pre-spec-subs` feature governs whether `[subs="..."]` block attributes are parsed and surfaced.

**Public surface (feature-gated):**
- `SubstitutionSpec`, `SubstitutionOp`, and `BlockMetadata.substitutions` exist **only** under `pre-spec-subs`.
- `Substitution`, `substitute()`, `NORMAL`, `VERBATIM`, `HEADER` are public unconditionally — attribute reference expansion (`{attr}` → value) needs them either way.

**Diagnostics — two paths, both via `Warning` / `Diagnostics`:**
- Feature **on**: warn that custom substitutions are experimental and may change.
- Feature **off**: warn that the setting is not honored and record source recovery, available through `ParseResult::source_recovery()` even after `take_warnings()`.

Converter-side plumbing (`SubsFlags`, `effective_subs`, fixture naming) lives in `converters/AGENTS.md`.

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

Ask before regenerating expected JSON. From the workspace root, select exact top-level fixture filenames:
```bash
cargo run -p acdc-parser --example generate_parser_fixtures --all-features -- example.adoc
```

With no filenames, the generator rewrites all top-level parser JSON fixtures. Record `git status --short` before running it and inspect every changed fixture afterward. Create `.adoc` inputs directly; use the generator only for expected JSON. Use stable nextest expressions instead of generated fixture numbers when running one fixture test. Do not use the CLI to generate fixtures.

`fixtures/preprocessor/` contains supporting scenarios, not automatically discovered tests. Each root input must be opened by a Rust test.

## Property tests

```bash
cargo nextest run -p acdc-parser --all-features -E 'test(/proptests/)'
PROPTEST_CASES=10000 cargo nextest run -p acdc-parser --all-features -E 'test(/proptests/)'
```

Regressions are tracked in `proptest-regressions/`.
