# Changelog

All notable changes to `acdc-pdf-render` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The crate now lives under `converters/pdf/crates` as a non-publishable implementation
  component of `acdc-converters-pdf`; its Cargo package name remains unchanged.

### Added

- Initial Typst-backed PDF renderer with bundled fallback fonts, optional
  runtime font directories, on-demand resolved image embedding, and non-fatal
  Typst warning reporting.
