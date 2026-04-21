# Changelog

All notable changes to `acdc-converters-manpage` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Major revamp of the manpage converter to match asciidoctor output much more closely.

### Performance

- **Streaming output.** Rendering writes directly to the caller's `Write`
  target, keeping allocations flat on large documents.

### Added

- **`[listing]` and `[source]` styled paragraphs** — paragraphs with `[listing]` or
  `[source,lang]` style now render as preformatted text (same as `[literal]`).

- **Typography replacements** — em-dashes (`--`), arrows (`->`, `<-`, `=>`), ellipsis (`...`),
  symbols (`(C)`, `(R)`, `(TM)`), and smart apostrophes now render as proper roff escapes
  via the shared `apply_replacements()` pipeline feeding into `manify()`.
- **Thin-space and zero-width-space escaping** — `\u{2009}` maps to `\|` and `\u{200B}`
  maps to `\&` in roff output, supporting em-dash typography replacements.
- **Table colspan/rowspan support** — cells with `colspan` and `rowspan` now render correctly
  using per-row tbl format lines with `s` (horizontal span) and `^` (vertical span) markers.
- **Test fixtures** for video/audio blocks, index terms, inline/block images, icon macros,
  STEM blocks, volume number variations, custom `man-linkstyle`, and embedded mode with media.
- Quote and verse block attribution rendering (`[quote, author, citation]`)
- Footnotes section (rendered as NOTES, matching asciidoctor)
- Author(s) section auto-generated from document header
- Checklist marker rendering for unordered list items
- Description list principal text rendering (inline content after `::`)
- List continuation blocks with RS/RE scoping for all list types
- Proper `.URL` and `.MTO` macro usage for links and autolinks (replacing inline angle-bracket format)
- Section level 3+ rendering as bold paragraph headings
- Table column alignment support and inline formatting in table cells
- Source file modification date fallback for `revdate`
- Arrow character escapes (right/left arrows, double arrows)
- Support for `man source`, `man manual`, `man-source`, `man-manual` attribute aliases

### Fixed

- **Em-dash inside inline formatting** — `--` inside bold, italic, monospace, highlight,
  superscript, subscript, and curved quotes is no longer converted to an em-dash at string
  boundaries, matching asciidoctor behavior.
- Non-paragraph content in table cells (lists, code blocks, admonitions) is no longer
  silently dropped
- Explicit `mailto:` macros now capture trailing punctuation in the `.MTO` macro's third
  argument (matching autolink behaviour)
- **Inline markup in `link:` text** — the `.URL` macro's display-text argument now
  reflects parsed inline markup inside the link's bracket expression.

### Changed

- Refactored grid-building logic to use shared utilities from `acdc-converters-core`.
- Skip NOTES and AUTHOR(S) sections in embedded mode to match asciidoctor behaviour
- **Attribution rendering** — uses `BlockMetadata.attribution`/`citetitle` fields instead of
  string attributes. ([#357])
- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])
- Replaced `.PP` with `.sp` throughout for consistent paragraph spacing
- Monospace text now uses Courier font (`\f(CR`) instead of bold
- Unordered list continuation blocks use `.RS 2` to align text with item content
- All list types now wrapped in RS/RE for proper indent scoping
- Ellipsis rendering uses thin-space separated dots (`.\\|.\\|.`)
- Comment header format corrected (`.\\"` instead of `.\"`)
- Menu macro renders target with arrow separators
- Document title strips trailing volume number in header comment
- Multi-author support in header comment and AUTHORS section
- Subsection headings (level 2) preserve original case instead of uppercasing

[#313]: https://github.com/nlopes/acdc/pull/313
