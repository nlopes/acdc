# Changelog

All notable changes to `acdc-editor-wasm` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial WASM package for the AsciiDoc live editor
- AST-based syntax highlighting with `<span class="adoc-*">` CSS classes
- Live preview using the same parser and HTML converter as the CLI
- DOM orchestration: debounced parsing, scroll sync, Tab key insertion,
  clipboard copy, and pre-filled GitHub issue links
- GitHub Actions release workflow (`release-editor-wasm.yml`) for building
  with wasm-pack and publishing as GitHub Release assets

[Unreleased]: https://github.com/nlopes/acdc/commits/HEAD
