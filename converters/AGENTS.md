# Converters — Developer Guide

## Architecture

All converters implement the `Visitor` trait from `core/src/visitor.rs`.

Shared utilities in `core/`:
- `substitutions.rs` — typography replacements, escape handling
- `section.rs` — section numbering and hierarchy
- `table.rs` — table layout calculations (colspan/rowspan)
- `code.rs` — syntax highlighting integration
- `toc.rs` — table of contents generation

## Debugging

- Use `compare-asciidoc-output` agent to diff converter output against asciidoctor
- For fixture mismatches, run `regen-fixtures` skill (ask first)

## Fixture regeneration

```bash
cargo run -p acdc-converters-html --example generate_html_fixtures --all-features
cargo run -p acdc-converters-terminal --example generate_terminal_fixtures --all-features
cargo run -p acdc-converters-manpage --example generate_manpage_fixtures --all-features

# Markdown uses a shell script that drives the CLI (no example binary):
bash converters/markdown/tests/regenerate_expected.sh
# Regenerate a single fixture:
bash converters/markdown/tests/regenerate_expected.sh <fixture_name>
```
