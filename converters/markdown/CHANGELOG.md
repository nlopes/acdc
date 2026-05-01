# Changelog

All notable changes to `acdc-converters-markdown` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- User-facing converter warnings are now collected in `ConversionResult` for
  recoverable Markdown conversion fallbacks such as skipped unsupported blocks,
  unsupported inline constructs, and capped heading levels.
- **`MarkdownVariant` enum** (`CommonMark` / `GitHubFlavored`) with `FromStr`
  and `Display`. `Processor::new` defaults to `GitHubFlavored`; use
  `Processor::with_variant` for another flavour.
- `Converter::name(&self)` returns `"markdown"` (replaces `Converter::backend()`).
- **Collapsible example blocks** — example blocks with `[%collapsible]` (and the
  `%open` modifier) now render as embedded `<details>/<summary>` HTML, which
  GitHub, GitLab, and most other Markdown renderers display as expandable
  sections. Applies to both delimited (`====`) and paragraph-style
  (`[example%collapsible]`) forms. When no title is given, the summary defaults
  to "Details", matching the HTML converter.
- **Description list fallback rendering** — description lists now render as unordered
  lists with bold terms and indented descriptions, instead of only emitting a warning
  comment.

### Fixed

- **Inline markup in `link:` text** — link text with nested formatting is now rendered
  through the full inline pipeline instead of emitted verbatim.
