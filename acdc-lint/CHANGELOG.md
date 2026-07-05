# Changelog

All notable changes to `acdc-lint` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added location-scoped lint level overrides for individual lint IDs. Overrides
  can now target `lint@line`, `lint@start-end`, `lint@line:column`, or
  `lint@start-line:start-column-end-line:end-column`, with comma-separated
  scopes such as `lint@line,line`; stale scopes that no longer match a
  diagnostic now report a warning so obsolete local suppressions can be removed
  or moved.
- Added the opt-in `section-title-capitalization-monospace` lint for projects
  that want lowercase leading monospace title text to be reported. The existing
  `section-title-capitalization` lint now ignores titles that start with
  monospace text, so case-sensitive tool and command names can keep their exact
  casing by default.
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
- Added a linked `RULES.adoc` reference that documents every lint ID, default level,
  group membership, detection intent, drawbacks, and bad/good examples. Lint
  references inside the page link to the corresponding lint sections.
