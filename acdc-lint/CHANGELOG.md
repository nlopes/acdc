# Changelog

All notable changes to `acdc-lint` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial lint command support crate with lint names, lint groups, severity
  overrides, and report types for future AsciiDoc recommended-practice checks.
  The `recommended-practices` group starts with low-noise style checks, while
  stricter document-header checks remain opt-in by lint name.
