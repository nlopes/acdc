---
name: trace-parse
description: Debug parser with trace output showing grammar rule execution.
arguments:
  - name: adoc_file
    description: Path to .adoc file to parse with trace logging
    required: true
  - name: module
    description: "Module to trace (default: document, options: inline_preprocessor, preprocessor, all)"
    required: false
argument-hint: <file> [module]
allowed-tools:
  - Bash
  - Read
---

# Parser Trace Debugging

Run acdc with trace-level logging to debug parser behavior.

## Execution

1. Set trace level:
   - Default: `RUST_LOG=acdc_parser::grammar::document=trace`
   - `inline_preprocessor`: `RUST_LOG=acdc_parser::grammar::inline_preprocessor=trace`
   - `preprocessor`: `RUST_LOG=acdc_parser::preprocessor=trace`
   - `all`: `RUST_LOG=acdc_parser=trace`

2. Run: `RUST_LOG={level} cargo run --bin acdc -- convert --output - {adoc_file}`
   - Trace goes to stderr, HTML to stdout

3. Analyze trace output:
   - Build call hierarchy from grammar rule enter/exit events
   - Identify failure points (byte offsets, expected vs found)
   - Flag excessive backtracking
   - If failed: show exact failure point, failing rule, input context, and what was expected
