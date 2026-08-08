# Changelog

All notable changes to `acdc-pdf-typst` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Generated documents apply the configured margin between document blocks.
- Generated block titles and captions use the configured PDF caption style.
- Generated documents include theme-derived abstract title and body styles.
- Generated documents can set the language and optional region of their text.
- Running-header text is configured independently from embedded document title
  metadata.
- Running headers start after the first numbered page, so the document title is
  not repeated at the top of its opening page.
- The crate now lives under `converters/pdf/crates` as a non-publishable implementation
  component of `acdc-converters-pdf`; its Cargo package name remains unchanged.
- `EmitOptions` now contains only preamble configuration; converters place tables of
  contents during their document traversal.

### Added

- Generated documents can include title, author, description, and keyword
  metadata.
- Initial shared Typst writer, escaping, page preamble, header/footer,
  watermark, image, callout, list, table, and source-code styling helpers for
  PDF converters. Every font stack ends with the bundled colour emoji face, so
  emoji render as glyphs in body text, headings, and code, under any theme.
