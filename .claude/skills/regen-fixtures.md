---
name: regen-fixtures
description: "Regenerate test fixtures after AST or converter changes. WARNING: modifies files. Use when parser or converters output changed intentionally."
allowed-tools:
  - Bash
  - Read
---

# Regenerate Fixtures

Run fixture regeneration and summarize what changed.

## Execution

1. Run all fixture generators:
   - `cargo run -p acdc-parser --example generate_parser_fixtures --all-features`
   - `cargo run -p acdc-converters-html --example generate_html_fixtures --all-features`
   - `cargo run -p acdc-converters-terminal --example generate_terminal_fixtures --all-features`
   - `cargo run -p acdc-converters-manpage --example generate_manpage_fixtures --all-features`

2. Show git diff summary for each fixture directory

3. Analyze changes:
   - Categorize: new fields, type changes, structural refactoring, position changes
   - Identify patterns across multiple fixtures (e.g., "all nodes now include source positions")
   - Flag breaking changes
   - Show before/after JSON snippets for key changes

4. Suggest next steps: review diffs, run `cargo nextest run --all-features --all-targets -j 8 --no-fail-fast`, commit if correct
