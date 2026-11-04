# Changelog

## [Unreleased]

### Fixed

- Fixture generation now exits with an error when parsing or conversion fails, so scripts cannot report success with missing output. Failed conversions preserve existing expectations.

### Changed

- Fixture generation uses Unsafe mode to match the Asciidoctor CLI.
  Includes still expand when the parser default changes to Secure.
