# Changelog

All notable changes to `acdc-editor-wasm` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-02-05

### Added

- Comprehensive README with API reference, CSS classes, and usage examples
- Syntax highlighting support via `acdc-converters-html` `highlighting` feature
- wasm-bindgen release profile settings for optimized builds

### Changed

- Reduced input debounce from 250ms to 25ms for more responsive editing

### Fixed

- GitHub issues link now points to correct repository (`nlopes/acdc`)

## [0.1.0] - 2026-02-04

### Added

- Initial WASM package for the AsciiDoc live editor
- AST-based syntax highlighting with `<span class="adoc-*">` CSS classes
- Live preview using the same parser and HTML converter as the CLI
- DOM orchestration: debounced parsing, scroll sync, Tab key insertion,
  clipboard copy, and pre-filled GitHub issue links
- GitHub Actions release workflow (`release-editor-wasm.yml`) for building
  with wasm-pack and publishing as GitHub Release assets

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-editor-wasm-v0.2.0...HEAD
[0.2.0]: https://github.com/nlopes/acdc/compare/acdc-editor-wasm-v0.1.0...acdc-editor-wasm-v0.2.0
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-editor-wasm-v0.1.0
