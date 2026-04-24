# Changelog

All notable changes to `acdc-converters-markdown` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Description list fallback rendering** — description lists now render as unordered
  lists with bold terms and indented descriptions, instead of only emitting a warning
  comment.

### Fixed

- **Inline markup in `link:` text** — link text with nested formatting is now rendered
  through the full inline pipeline instead of emitted verbatim.
