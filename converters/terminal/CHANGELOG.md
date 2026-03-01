# Changelog

All notable changes to `acdc-converters-terminal` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Word wrapping for content inside box-drawn blocks (sidebars, examples, admonitions, quote blocks)
- Unicode-aware character width measurement for correct CJK and emoji wrapping
- `Processor::with_terminal_width()` for deterministic width control in tests and fixture generation.
- Section numbering support (`sectnums`, `partnums`, appendix tracking).
- Index term collection and alphabetized index catalog rendering (`[index]` sections).
- Table column alignment and column style support (strong, emphasis, header).
- Alternating row shading in tables for readability.
- Dynamic terminal width detection, capped at 120 columns.
- Super/subscript Unicode conversion with dim-styled fallback for unsupported characters.
- Cross-reference, callout reference, button, keyboard, menu, stem, image, and icon inline macro rendering.
- Box-drawing characters for example, sidebar, and open blocks.
- Comprehensive test fixture covering all major terminal output features.
- Index section test fixture.

### Fixed

- `extract_plain_text` now preserves text content from formatted inline nodes (bold, italic, monospace, etc.) in literal paragraphs
- ANSI SGR state tracking now prunes cancelled codes (e.g. bold-off removes bold) instead of accumulating indefinitely
- `extract_title_text` now preserves inline content from `VerbatimText`, `RawText`,
  `StandaloneCurvedApostrophe`, `LineBreak`, `CalloutRef`, and all `Macro` variants
  in section titles. Previously these were silently dropped.
- Comprehensive test fixture now marked as OSC8 so it is skipped in CI environments without OSC8 support.

### Changed

- `pad_to_width` returns `Cow<str>` to avoid allocation when padding is not needed
- Deduplicated ANSI escape skipping logic into shared `skip_ansi_escape` helper
- **Attribution rendering** — uses `BlockMetadata.attribution`/`citetitle` fields instead of
  string attributes. ([#357])
- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])
- `Error` type is now public (was `pub(crate)`), enabling external code to handle
  terminal converter errors explicitly.
- List rendering no longer inserts extra spaces between inline nodes.
- Enabled `custom_styling` feature on `comfy-table` for ANSI-aware column width calculations, fixing garbled table layouts with styled cell content.

[#313]: https://github.com/nlopes/acdc/pull/313
