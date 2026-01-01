# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Support for `{blank}`, `{cxx}`, and `{pp}` character replacement attributes

### Fixed

- `{empty}` attribute now works in description lists and inline contexts ([#262])

### Changed

- HTML converter now outputs Unicode characters as numeric entities (e.g., `&#160;`
  instead of raw `\u{00A0}`) to match asciidoctor output

[#262]: https://github.com/nlopes/acdc/issues/262

## [0.1.1] - 2025-12-31

### Added

- README.md instead of README.adoc for `acdc-parser` to make sure it gets picked up by
  cargo publish.
- This Changelog 🥳

### Fixed

- Handle backslash-escaped patterns in URLs and text
  Fixes URLs like `{repo}/compare/v1.0\...v2.0` producing correct output instead of broken
  links with forward slashes or ellipsis entities.
- Documentation of a few models in the parser is now closer to what they actually are, and
  do, basically their intent.

## [0.1.0] - 2025-12-28

Initial release of acdc, an AsciiDoc parser and converter toolchain written in Rust.

### Added

- `acdc-parser`: PEG-based AsciiDoc parser with source location tracking
- `acdc-cli`: Command-line tool for parsing and converting AsciiDoc documents
- `acdc-lsp`: Language Server Protocol implementation with go-to-definition, hover,
  completion, diagnostics, and semantic tokens
- Converters for HTML, manpage (roff), and terminal output
