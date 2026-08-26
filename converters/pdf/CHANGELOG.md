# Changelog

All notable changes to `acdc-converters-pdf` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Typst-backed PDF converter with broad support for AsciiDoc document
  structure, blocks, inlines, navigation, lists, tables, images, source code,
  indexes with `see` and `see-also` relationships, and books. It includes PDF
  themes, page headers and footers, portrait and landscape layouts, A3/A4/A5 and
  common US page sizes, custom page dimensions and per-document margins, document
  metadata, lower-Roman front matter with configurable Arabic page-numbering
  starts, trusted fonts, safe asset loading, strict asset checks, and optional
  Typst output for diagnostics. Print indexes keep Roman page labels separate
  and can span from the final Roman label into a contiguous Arabic range.
