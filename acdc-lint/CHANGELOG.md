# Changelog

All notable changes to `acdc-lint` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `acdc lint` checks documents against AsciiDoc recommended practices. The
  groups are `recommended-practices`, `document-structure`, `source-format`,
  `semantic-asciidoc` and `resources`. Stricter header checks are opt-in by
  lint name.
- Lints cover parser warnings, tables, headings, block spacing, whitespace,
  lists, images and Markdown syntax.
- Three `document-structure` lints warn by default about attribute entries.
  `attribute-name-not-normalized` reports names such as `:TOC:`.
  `header-only-attribute` reports header-only attributes in the body.
  `unset-attribute-value` reports text after an unset, such as `:!toc: left`.
- The opt-in `section-title-capitalization-monospace` lint reports titles
  that start with lowercase monospace text. `section-title-capitalization` now
  skips titles that start with monospace text.
- Lint level overrides can target locations: `lint@line`, `lint@start-end`,
  `lint@line:column` or `lint@start-line:start-column-end-line:end-column`.
  Separate several scopes with commas. A scope that no longer matches reports
  a warning.
- `--compatibility-mode asciidoctor|draft` selects the parsing rules. The
  default is `asciidoctor`.
- `--max-total-include-bytes` limits include text per parse. The default is
  64 MiB.

### Changed

- Attribute lints now check only accepted text values.

### Fixed

- `one-sentence-per-line` now handles description lists, inline formatting
  and colon-terminated lead-ins.
- Image checks now use the `imagesdir` value at each image's position, and
  read the parsed alt text.
- `counter-syntax`, `attribute-url-prefix`,
  `delimited-block-minimal-delimiter` and `excessive-blank-lines` report fewer
  false positives.
