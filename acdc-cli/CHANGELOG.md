# Changelog

All notable changes to `acdc-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Automatic pager for terminal output** - When using `--backend terminal` and stdout is
  a TTY, output is automatically piped through a pager. Defaults to `less -FRX` on Unix
  and `more` on Windows. Respects the `PAGER` environment variable for custom pagers. Use
  `--no-pager` to disable, or set `PAGER=""`. On Unix, sets `LESSCHARSET=utf-8` for proper
  UTF-8 display. ([#311])
- `--embedded` / `-e` flag to output embeddable content without document wrapper elements.
  Behaviour varies by backend: HTML skips DOCTYPE/html/head/body/content wrapper, manpage
  skips preamble and NAME section, terminal skips header/authors/ revision info. Matches
  asciidoctor's `--embedded` behaviour. ([#272])
- **Index catalog rendering for HTML** - Documents with `[index]` sections now generate a
  fully populated index catalog, organized alphabetically by first letter with
  hierarchical nesting for secondary and tertiary terms. Each entry links back to the
  source location via inline anchors. This goes beyond asciidoctor's HTML backend which
  leaves index sections empty. The index only renders when it's the last section in the
  document.
- **Semantic HTML5 backend** — `--backend html5s` produces semantic HTML5 output with proper
  elements and ARIA roles instead of div-based layout. ([#329])

### Fixed

- Horizontal description lists (`[horizontal]`) now render as `<table>` with `hdlist`
  class instead of `<dl>` with `dlist horizontal`, matching asciidoctor output ([#270])
- List titles (`.My title` syntax) now render correctly in HTML and manpage output.
  HTML uses `<div class="title">`, manpage uses bold formatting, matching asciidoctor
  behaviour. Terminal output already supported this. ([#273])

## [0.1.0] - 2026-01-02

This is tagged but unreleased in crates.io for now.

### Added

- Description lists now support roles (e.g., `[.stack]`) which are applied to the wrapper
  `<div>` element, matching asciidoctor behaviour ([#264])

### Changed

- Removed dependency on `acdc-core` (purely internal change so no need to bump minor
  version)

[#264]: https://github.com/nlopes/acdc/issues/264
[#270]: https://github.com/nlopes/acdc/issues/270
[#272]: https://github.com/nlopes/acdc/issues/272
[#273]: https://github.com/nlopes/acdc/issues/273
[#311]: https://github.com/nlopes/acdc/issues/311
[#329]: https://github.com/nlopes/acdc/issues/329

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-cli-v0.1.0...HEAD
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-cli-v0.1.0
