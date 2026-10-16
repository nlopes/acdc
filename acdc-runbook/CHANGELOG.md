# Changelog

All notable changes to `acdc-runbook` are documented in this file.

## [Unreleased]

### Added

- Internal asciicast v3 recording support with selected command provenance,
  lifecycle events, structured outcomes, and separate capture completeness.
- Bounded streaming reads retain complete earlier events when a recording
  ends with a damaged line. Unsupported metadata can fall back to terminal replay.
