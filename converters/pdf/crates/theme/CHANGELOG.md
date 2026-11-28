# Changelog

All notable changes to `acdc-pdf-theme` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial PDF theme support, including page dimensions, margins and
  orientation under `page`.
- `--emit-theme-config` writes every resolved theme setting as acdc YAML.
- `--theme-format asciidoctor` converts Asciidoctor PDF 2.3.15 themes. Warnings
  name each unsupported setting. External parent themes and font catalogs
  aren't loaded.
- Bundled fonts add bold italic text, italic code and symbols such as `⇒`. CJK
  fonts still need `--font-dir`.
- Themes can style page headers and footers, code block sizes, index columns,
  tables, block margins, captions and book page breaks.
- `page_numbering_start_at` sets where Arabic page numbers start. Earlier pages
  use lower-Roman numbers.
