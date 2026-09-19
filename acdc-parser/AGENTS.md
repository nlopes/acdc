# Parser — Developer Guide

## Architecture

- **PEG grammar** in `src/grammar/` — `document.rs` is the main entry point
- **Two-pass inline markup** processing (see SDR-005 in docs)
  - Phase 1: Inline preprocessor — extracts passthroughs, expands attribute references
  - Phase 2: Inline parser — parses expanded text into inline node tree
- **Preprocessor** (`src/preprocessor/`) handles includes before parsing
- Some features are inherently difficult with PEG (list continuations, table spanning)

## `pre-spec-subs` — parser contract

The default-on `pre-spec-subs` feature governs whether `[subs="..."]` block attributes are parsed and surfaced.

**Public surface (feature-gated):**
- `SubstitutionSpec`, `SubstitutionOp`, and `BlockMetadata.substitutions` exist **only** under `pre-spec-subs`.
- `Substitution`, `substitute()`, `NORMAL`, `VERBATIM`, `HEADER` are public unconditionally — attribute reference expansion (`{attr}` → value) needs them either way.

**Diagnostics — two paths, both via `Warning` / `Diagnostics`:**
- Feature **on**: "may change when spec finalises" (the draft AsciiDoc spec drops `subs=` entirely, so the experimental warning hedges).
- Feature **off**: "not honoured in this build" so users notice their attribute is being dropped silently.

Converter-side plumbing (`SubsFlags`, `effective_subs`, fixture naming) lives in `converters/AGENTS.md`.

## Document attribute policy

- `constants.rs` owns the static built-in read-only and API-only attribute protection.
- `Options` combines built-in protection with locks supplied by the caller.
- The CLI parses attribute assignment syntax, and converters may add unlocked defaults. Neither duplicates the parser's lock policy.
- Keep the policy internal. The public parser API should expose only stable caller intent, never converter-specific or test-specific fields and functions.
- When changing the policy, test document entries, parser `Options` and builder input, CLI `-a` input, locked and soft `@` assignments and unsets, and header/body exceptions. Compare both the official attribute documentation and the current asciidoctor implementation.

## Debugging

- **Grammar failures** → use `trace-parse` skill, then check `src/grammar/`
- **Preprocessor failures** → `trace-parse <file> preprocessor`

Trace module mapping (use with `rust-test-one`):
- Test contains "inline"/"markup" → `acdc_parser::grammar::inline_preprocessor=trace`
- Test contains "preprocess"/"include" → `acdc_parser::preprocessor=trace`
- Default → `acdc_parser::grammar::document=trace`

## Fixtures

Regenerate parser fixtures:
```bash
cargo run -p acdc-parser --example generate_parser_fixtures --all-features
```

The generator rewrites all parser JSON fixtures. Record `git status --short` before running it and inspect every changed fixture afterward. Treat the `.adoc` file as the test input and the generated `.json` file as expected output. Use stable nextest expressions instead of generated fixture numbers when running one fixture test.

## Property tests

```bash
cargo test --package acdc-parser --lib proptests
PROPTEST_CASES=10000 cargo test --package acdc-parser --lib proptests
```

Regressions are tracked in `proptest-regressions/`.
