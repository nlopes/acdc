# Changelog

All notable changes to `acdc-mcp` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `adoc_find` and fuzzy title lookup now use case-insensitive Unicode fuzzy matching. Use short terms or abbreviations; all space-separated terms must match, with no English stopword or shared-prefix rules.
- Metadata-only search matches now return the document with selector `@document`.

## [0.1.0] - 2026-10-25

- Initial release

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-mcp-v0.1.0...HEAD
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-mcp-v0.1.0
