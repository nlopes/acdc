# Parser — Developer Guide

## Architecture

- **PEG grammar** in `src/grammar/` — `document.rs` is the main entry point
- **Two-pass inline markup** processing (see SDR-005 in docs)
  - Phase 1: Inline preprocessor — extracts passthroughs, expands attribute references
  - Phase 2: Inline parser — parses expanded text into inline node tree
- **Preprocessor** (`src/preprocessor/`) handles includes before parsing
- Some features are inherently difficult with PEG (list continuations, table spanning)

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

## Property tests

```bash
cargo test --package acdc-parser --lib proptests
PROPTEST_CASES=10000 cargo test --package acdc-parser --lib proptests
```

Regressions are tracked in `proptest-regressions/`.
