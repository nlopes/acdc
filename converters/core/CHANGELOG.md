# Changelog

All notable changes to `acdc-converters-core` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `#[non_exhaustive]` attribute on `Options`, `GeneratorMetadata`, `toc::Config`,
  `Doctype`, and `IconMode` for semver-safe future additions
- Comprehensive module-level documentation
- `acdc-converters-dev` crate for test utilities (not published to crates.io)
- Visitor method `visit_callout_ref` for processing callout references
- `Backend::Html5s` variant for semantic HTML5 output

### Fixed

- Preamble wrapper now only renders when all conditions are met: document has a title,
  contains at least one section, and has content before that section. Previously,
  documents without sections incorrectly rendered preamble wrappers. ([#275])

### Changed

- **`detect_language()` now returns any source block language** — instead of matching against
  a hardcoded list of known languages, the function returns the first positional attribute
  for any `[source,LANG]` block. This means `[source,text]` and other unlisted languages
  now correctly produce `<code class="language-text">` wrappers. Removed the `LANGUAGES`
  constant.
- **BREAKING**: Renamed crate from `acdc-converters-common` to `acdc-converters-core`
- **BREAKING**: `Options` struct now uses builder pattern with private fields -
  use `Options::builder().doctype(...).build()` instead of struct construction
- **BREAKING**: `toc::Config` fields are now private - use accessor methods
  (`placement()`, `title()`, `levels()`, `toc_class()`)
- **BREAKING**: Renamed `Processable` trait to `Converter` with new output routing:
  - New `OutputDestination` enum for routing output (stdout, file, buffer)
  - `convert()` is now a provided method that handles output routing
  - Required methods: `convert_to_stdout()`, `convert_to_file()`
  - New helpers: `write_to()`, `derive_output_path()`, `after_write()` ([#313])

[#275]: https://github.com/nlopes/acdc/issues/275
[#313]: https://github.com/nlopes/acdc/pull/313
