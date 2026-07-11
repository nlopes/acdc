# Changelog

All notable changes to `acdc-pdf-images` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The crate now lives under `converters/pdf/crates` as a non-publishable implementation
  component of `acdc-converters-pdf`; its Cargo package name remains unchanged.

### Added

- Initial release for a PDF image resolver for local, remote, `file://`, and `data:` URI
  images (scheme matching is case-insensitive). Each validated image is snapshotted into an
  explicit caller-owned spool and handed to the renderer as a path, never retained bytes.
