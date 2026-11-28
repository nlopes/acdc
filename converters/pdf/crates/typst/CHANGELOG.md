# Changelog

All notable changes to `acdc-pdf-typst` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Typst writer for PDF output, with color emoji in any theme.
- PDF output supports A3, A4, A5, Executive, Legal, Letter, Tabloid and custom
  page sizes.
- PDF metadata can include title, author, description and keywords.
- Block images can be links with themed captions.

### Changed

- Headers, footers, block margins, captions and abstracts use theme styles.
  Page headers start after the first numbered page.
- Admonitions show text labels and a vertical divider.

### Fixed

- Backticks and spacing acute accents no longer attach to the previous letter
  in IBM Plex Serif.
