# Changelog

All notable changes to `acdc-converters-terminal` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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

- Comprehensive test fixture now marked as OSC8 so it is skipped in CI environments without OSC8 support.

### Changed

- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])
- `Error` type is now public (was `pub(crate)`), enabling external code to handle
  terminal converter errors explicitly.
- List rendering no longer inserts extra spaces between inline nodes.
- Enabled `custom_styling` feature on `comfy-table` for ANSI-aware column width calculations, fixing garbled table layouts with styled cell content.

[#313]: https://github.com/nlopes/acdc/pull/313
