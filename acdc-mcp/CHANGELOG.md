# Changelog

All notable changes to `acdc-mcp` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking changes

- The `pre-spec-subs` and `setext` Cargo features are removed. Remove them from
  your feature lists. The compatibility mode now selects the language rules,
  and block `subs=` works in every build. Every build recognizes underlined
  (setext) headings in Asciidoctor mode, the default.

### Added

- Releases ship prebuilt `acdc-mcp` binaries for Linux and macOS (x86_64 and
  aarch64) and Windows (x86_64).
- `--compatibility-mode asciidoctor|draft` selects the language rules for all
  document tools. The default is `asciidoctor`. Continuation cursors can't be
  reused after a mode change.

### Changed

- `adoc_find` and fuzzy title lookup now use case-insensitive Unicode fuzzy
  matching. Every space-separated term must match.
- Metadata-only search matches now return the document with the `@document`
  selector.
- AsciiDoc responses keep character references as written. Plain-text
  responses show the character.

### Fixed

- `adoc_links` now finds `link=` targets on inline images and icons, and links
  in captions, alt text and labels. It reports each link once.
- Paragraphs and thematic breaks with an ID now appear in the tree. You can
  select them by ID or alias.
- Description-list excerpts now keep every term that shares a definition.
- Rendered text now includes formatted image alt text and UI labels.

## [0.1.0] - 2026-10-25

- Initial release

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-mcp-v0.1.0...HEAD
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-mcp-v0.1.0
