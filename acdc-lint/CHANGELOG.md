# Changelog

All notable changes to `acdc-lint` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial lint command support crate with lint names, lint groups, severity overrides,
  report types, and implemented AsciiDoc recommended-practice checks. The
  `recommended-practices` group starts with low-noise style checks, while stricter
  document-header checks remain opt-in by lint name. Counter naming is intentionally
  omitted because acdc warns on unsupported counter syntax and removes it from output.
  `document-structure`, `source-format`, `semantic-asciidoc`, and `resources` groups
  organize lints by authoring intent. Added diagnostics for parser recovery warnings,
  table format/row issues, unsupported counter syntax, repeated document titles, heading
  marker spacing and capitalization, delimited block blank-line layout, trailing
  whitespace, hard tabs, repeated blank lines, list marker spacing, explicit numbered list
  markers, bold-term paragraphs, image alt text, missing local image files, and Markdown
  heading/link/image/code fence/table syntax. Lint metadata now includes stable
  long-form explanations for each lint ID, suitable for generated rule docs and future
  `acdc lint --explain` output.
