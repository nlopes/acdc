# Changelog

All notable changes to `acdc-pdf-theme` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The crate now lives under `converters/pdf/crates` as a non-publishable implementation
  component of `acdc-converters-pdf`; its Cargo package name remains unchanged.

### Added

- Themes can place tables at the left, centre, or right of the available page
  width. The default is left.
- Themes can control table borders, the header divider, row stripes, and header
  and footer backgrounds. The defaults match Asciidoctor PDF.
- Themes can control the vertical margin between document blocks. The 12 pt
  default matches Asciidoctor PDF.
- Themes can control the alignment, colour, size, weight, style, and spacing of
  block titles and captions. The defaults closely match Asciidoctor PDF.
- Themes can control page breaks before book parts and chapters and can avoid a
  forced break before a part's first chapter.
- This crate - the starting code.
