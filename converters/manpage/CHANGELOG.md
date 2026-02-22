# Changelog

All notable changes to `acdc-converters-manpage` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Major revamp of the manpage converter to match asciidoctor output much more closely.

### Changed

- **Attribution rendering** — uses `BlockMetadata.attribution`/`citetitle` fields instead of
  string attributes. ([#357])

### Added

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

### Changed

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
