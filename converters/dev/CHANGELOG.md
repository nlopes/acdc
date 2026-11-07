# Changelog

## [Unreleased]

### Fixed

- Fixture generation now exits with an error when parsing or conversion fails, so scripts cannot report success with missing output. Failed conversions preserve existing expectations.

### Changed

- Fixture-generator callbacks now receive `ast::DocumentRef`. Read attributes with `document.attributes()` and pass the handle directly to converters. Generated output and fixture selection are unchanged.

- Fixture generation uses Unsafe mode to match the Asciidoctor CLI.
  Includes still expand when the parser default changes to Secure.
