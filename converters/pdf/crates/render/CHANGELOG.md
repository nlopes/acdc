# Changelog

All notable changes to `acdc-pdf-render` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Typst-based PDF renderer. It bundles fallback fonts, and also uses
  font directories given at runtime with `--font-dir`. It embeds resolved
  images as needed. Typst warnings are reported and don't stop the conversion.
- PDF output warns once for each text cluster that has no glyph after font
  fallback. The warning lists the Unicode code points that an extra font must
  cover.

### Changed

- PDF output keeps Typst's semantic structure tags, with the document language
  and supported content roles. This tagged output doesn't claim PDF/UA-1
  conformance.
