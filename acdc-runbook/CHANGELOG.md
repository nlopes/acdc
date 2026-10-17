# Changelog

All notable changes to `acdc-runbook` are documented in this file.

## [Unreleased]

### Added

- The format guide links to a real CLI capture and its report, including
  distinct task completion, command failure, and capture completeness.

- Internal asciicast v3 recording support with selected command provenance,
  lifecycle events, structured outcomes, and separate capture completeness.
- Bounded streaming reads retain complete earlier events when a recording
  ends with a damaged line. Unsupported metadata can fall back to terminal replay.
