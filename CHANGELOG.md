# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased acdc-converters-core]

### Added

- `#[non_exhaustive]` attribute on `Options`, `GeneratorMetadata`, `toc::Config`,
  `Doctype`, and `IconMode` for semver-safe future additions
- Comprehensive module-level documentation
- `acdc-converters-dev` crate for test utilities (not published to crates.io)
- Visitor method `visit_callout_ref` for processing callout references

### Fixed

- Preamble wrapper now only renders when all conditions are met: document has a title,
  contains at least one section, and has content before that section. Previously,
  documents without sections incorrectly rendered preamble wrappers. ([#275])

### Changed

- **BREAKING**: Renamed crate from `acdc-converters-common` to `acdc-converters-core`
- **BREAKING**: `Options` struct now uses builder pattern with private fields -
  use `Options::builder().doctype(...).build()` instead of struct construction
- **BREAKING**: `toc::Config` fields are now private - use accessor methods
  (`placement()`, `title()`, `levels()`, `toc_class()`)

## [Unreleased acdc-converters-html]

### Added

- Initial support for `[subs=...]` attribute on verbatim blocks (listing, literal)
  - `subs=none` - disables all substitutions, outputs raw content
  - `subs=specialchars` - only escapes HTML special characters
  - `subs=+replacements` - enables typography (arrows, dashes, ellipsis) in verbatim blocks
  - `subs=+attributes` - enables attribute expansion (`{attr}` → value) in verbatim blocks
  - `subs=+quotes` - enables inline formatting (`*bold*`, `_italic_`, etc.) in verbatim blocks
  - Default behavior unchanged (escapes HTML characters, no replacements/attributes/quotes)
  - Requires parser's `pre-spec-subs` feature flag. ([#280])

### Fixed

- Verbatim blocks (listing/literal) now correctly skip typography replacements by default,
  matching asciidoctor behavior. Previously, smart quotes were incorrectly applied.

## [Unreleased acdc-parser]

### Added

- Tag filtering for include directives ([#279])
  - `tag=name` - include a specific tagged region
  - `tags=a;b;c` - include multiple tags (semicolon or comma delimited)
  - `tags=*` - include all tagged regions
  - `tags=**` - include all content except tag directive lines
  - `tags=!name` - exclude a specific tag
  - `tags=*;!debug` - include all tags except debug
  - `tags=!*` - include only untagged content
  - Tag directives (`// tag::name[]` and `// end::name[]`) are automatically stripped
  - Nested tags supported; combining `tag=` with `lines=` applies both filters
- `substitute()` function for applying substitutions to text. Currently only
  `Attributes` substitution is implemented (expands `{attr}` references). ([#280])
- `parse_text_for_quotes()` function for parsing inline formatting (`*bold*`,
  `_italic_`, etc.) in arbitrary text. Used for quotes substitution in verbatim
  blocks. ([#280])

### Fixed

- Context-aware backslash stripping for `\^` and `\~` escapes now matches asciidoctor
  behavior. Backslashes are only stripped when they prevent actual formatting (e.g.,
  `\^super^`), preserved as literal text when at word boundaries without closing marker
  (e.g., `\^caret`). ([#278])
- Discrete headings (`[discrete]`) at the end of a document are now parsed correctly
  instead of causing a parsing error. ([#289])
- Paragraphs no longer incorrectly split when a line starts with inline passthrough
  syntax like `+>+`. The list continuation lookahead now only matches actual
  continuation markers (standalone `+` followed by whitespace/EOL/EOF).

### Removed

- **BREAKING**: Removed `Deserialize` implementation from all model types (`Document`,
  `Block`, `InlineNode`, and ~60 other AST types). Serialization to JSON remains
  supported. If you need to load previously serialized AST, parse the original
  AsciiDoc source instead.


## [Unreleased acdc-cli]

### Added

- `--embedded` / `-e` flag to output embeddable content without document wrapper elements.
  Behaviour varies by backend: HTML skips DOCTYPE/html/head/body/content wrapper, manpage
  skips preamble and NAME section, terminal skips header/authors/ revision info. Matches
  asciidoctor's `--embedded` behaviour. ([#272])
- **Index catalog rendering for HTML** - Documents with `[index]` sections now generate a
  fully populated index catalog, organized alphabetically by first letter with
  hierarchical nesting for secondary and tertiary terms. Each entry links back to the
  source location via inline anchors. This goes beyond asciidoctor's HTML backend which
  leaves index sections empty. The index only renders when it's the last section in the
  document.

### Fixed

- Horizontal description lists (`[horizontal]`) now render as `<table>` with `hdlist`
  class instead of `<dl>` with `dlist horizontal`, matching asciidoctor output ([#270])
- List titles (`.My title` syntax) now render correctly in HTML and manpage output.
  HTML uses `<div class="title">`, manpage uses bold formatting, matching asciidoctor
  behaviour. Terminal output already supported this. ([#273])

[#270]: https://github.com/nlopes/acdc/issues/270
[#272]: https://github.com/nlopes/acdc/issues/272
[#273]: https://github.com/nlopes/acdc/issues/273
[#275]: https://github.com/nlopes/acdc/issues/275
[#278]: https://github.com/nlopes/acdc/issues/278
[#279]: https://github.com/nlopes/acdc/issues/279
[#280]: https://github.com/nlopes/acdc/issues/280
[#289]: https://github.com/nlopes/acdc/issues/289

## [acdc-parser-v0.1.4] - 2026-01-04

### Added

- Index term support with type-safe `IndexTermKind` enum ([#274])
  - Flow terms (visible): `((term))` or `indexterm2:[term]`
  - Concealed terms (hidden): `(((term,secondary,tertiary)))` or `indexterm:[...]`
  - `IndexTermKind::Flow` can only hold a single term (no hierarchy)
  - `IndexTermKind::Concealed` supports primary/secondary/tertiary terms
- Callout references (`<1>`, `<.>`) in source/listing blocks now have source locations in
  the AST
- New `CalloutRef` inline node type for programmatic access to callout markers

### Changed

- Auto-numbered callouts (`<.>`) are now resolved during parsing, not rendering
- JSON serialization for Callout references uses `"name": "callout_reference"`

### Fixed

- List continuations with blank line before `+` now correctly attach to ancestor list
  items instead of the last nested item, matching asciidoctor behaviour ([#265])

[#265]: https://github.com/nlopes/acdc/issues/265
[#274]: https://github.com/nlopes/acdc/issues/274

## [acdc-parser-v0.1.3] - 2026-01-02

### Fixed

- Description lists now correctly break when a `[[id]]` anchor is present between items
  ([#269])

[#269]: https://github.com/nlopes/acdc/issues/269

## [acdc-cli-v0.1.0] - 2026-01-02

IMPORTANT: This is tagged but unreleased in crates.io for now.

### Added

- Description lists now support roles (e.g., `[.stack]`) which are applied to the wrapper
  `<div>` element, matching asciidoctor behaviour ([#264])

### Changed
- Removed dependency on `acdc-core` (purely internal change so no need to bump minor
  version)

[#264]: https://github.com/nlopes/acdc/issues/264

## [acdc-parser-v0.1.2] - 2026-01-02

### Added

- Support for `{blank}`, `{cxx}`, and `{pp}` character replacement attributes
- `SafeMode` enum now exported from `acdc_parser` (migrated from removed `acdc-core`)
- `FromStr` and `Display` implementations for `SafeMode`

### Fixed

- `{empty}` attribute now works in description lists and inline contexts ([#262])
- `{lt}`, `{gt}`, `{amp}` character replacement attributes now produce raw HTML characters
  that bypass escaping, matching asciidoctor behaviour ([#266])

### Changed

- HTML converter now outputs Unicode characters as numeric entities (e.g., `&#160;`
  instead of raw `\u{00A0}`) to match asciidoctor output
- Removed dependency on `acdc-core`

[#262]: https://github.com/nlopes/acdc/issues/262
[#266]: https://github.com/nlopes/acdc/issues/266

## [acdc-parser-v0.1.1] - 2025-12-31

### Added

- README.md instead of README.adoc for `acdc-parser` to make sure it gets picked up by
  cargo publish.
- This Changelog 🥳

### Fixed

- Handle backslash-escaped patterns in URLs and text
  Fixes URLs like `{repo}/compare/v1.0\...v2.0` producing correct output instead of broken
  links with forward slashes or ellipsis entities.
- Documentation of a few models in the parser is now closer to what they actually are, and
  do, basically their intent.

## [acdc-parser-v0.1.0] - 2025-12-28

Initial release of acdc, an AsciiDoc parser and converter toolchain written in Rust.

### Added

- `acdc-parser`: PEG-based AsciiDoc parser with source location tracking
- `acdc-cli`: Command-line tool for parsing and converting AsciiDoc documents
- `acdc-lsp`: Language Server Protocol implementation with go-to-definition, hover,
  completion, diagnostics, and semantic tokens
- Converters for HTML, manpage (roff), and terminal output
