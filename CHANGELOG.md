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
- **BREAKING**: Renamed `Processable` trait to `Converter` with new output routing:
  - New `OutputDestination` enum for routing output (stdout, file, buffer)
  - `convert()` is now a provided method that handles output routing
  - Required methods: `convert_to_stdout()`, `convert_to_file()`
  - New helpers: `write_to()`, `derive_output_path()`, `after_write()` ([#313])

## [Unreleased acdc-converters-html]

### Added

- Table colspan and rowspan rendering (`colspan="n"` and `rowspan="n"` attributes on `<th>`/`<td>`)
- Table visual attribute support:
  - `frame` attribute - controls outer border (`all`, `ends`/`topbot`, `sides`, `none`)
  - `grid` attribute - controls inner gridlines (`all`, `rows`, `cols`, `none`)
  - `stripes` attribute - controls row striping (`even`, `odd`, `all`, `hover`)
  - `width` attribute - sets explicit table width (e.g., `width=75%`)
  - `%autowidth` option - uses `fit-content` sizing instead of `stretch`
  - Custom roles from metadata applied as CSS classes
- Cell-level alignment overrides are now respected, falling back to column-level defaults
- Initial support for `[subs=...]` attribute on verbatim blocks (listing, literal)
  - `subs=none` - disables all substitutions, outputs raw content
  - `subs=specialchars` - only escapes HTML special characters
  - `subs=+replacements` - enables typography (arrows, dashes, ellipsis) in verbatim blocks
  - `subs=+attributes` - enables attribute expansion (`{attr}` → value) in verbatim blocks
  - `subs=+quotes` - enables inline formatting (`*bold*`, `_italic_`, etc.) in verbatim blocks
  - Default behavior unchanged (escapes HTML characters, no replacements/attributes/quotes)
  - Requires parser's `pre-spec-subs` feature flag. ([#280])

### Fixed

- Superscript (`^text^`) and subscript (`~text~`) now respect the quotes substitution
  setting, matching asciidoctor behavior. Previously they always rendered as `<sup>`/`<sub>`
  even when quotes was disabled (e.g., in listing blocks or with `[subs=-quotes]`).
- Passthrough content (`pass:[]`, `+++`, `++`, `+`) no longer has attribute references
  incorrectly expanded by the converter. Attribute expansion is now handled solely by
  the parser based on each passthrough's own substitution settings. ([#291])
- Verbatim blocks (listing/literal) now correctly skip typography replacements by default,
  matching asciidoctor behavior. Previously, smart quotes were incorrectly applied.

### Changed

- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])

## [Unreleased acdc-converters-manpage]

### Changed

- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])

## [Unreleased acdc-converters-terminal]

### Changed

- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])
- `Error` type is now public (was `pub(crate)`), enabling external code to handle
  terminal converter errors explicitly.

## [Unreleased acdc-parser]

### Added

- Complete cell specifier support for tables:
  - Colspan: `2+|` spans 2 columns
  - Rowspan: `.2+|` spans 2 rows
  - Combined: `2.3+|` spans 2 columns and 3 rows
  - Cell duplication: `3*|` duplicates cell content 3 times
  - Cell-level horizontal alignment: `<|` (left), `^|` (center), `>|` (right)
  - Cell-level vertical alignment: `.<|` (top), `.^|` (middle), `.>|` (bottom)
  - Cell-level style: `s|` (strong), `e|` (emphasis), `m|` (monospace), etc.
  - All specifiers can be combined (e.g., `2.3+^.^s|` for colspan=2, rowspan=3, centered, strong)
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

- DSV tables now correctly preserve the first cell. Previously, DSV format (`cell1:cell2`)
  was incorrectly treated like PSV (`| cell1 | cell2 |`), causing the first cell to be
  dropped. Escape handling (`\:` → literal `:`) also works correctly now.
- Table cell content now has correct source position tracking for multi-line cells
- CSV tables with quoted multiline values now have accurate source positions. Previously,
  positions were approximated without accounting for quote characters, so `"Hello\nWorld"`
  would report incorrect line/column. Now positions point to actual content start (inside
  the quotes), with proper handling for RFC 4180 escaped quotes (`""`).
- Description lists with terms starting with `#` (e.g., `#issue-123:: definition`)
  are no longer incorrectly parsed as section boundaries inside sections. The
  section boundary detection now requires a space after the level marker.
- (setext feature) Description list items are no longer matched as potential
  setext section titles, preventing parse errors when description lists appear
  before delimiter lines.
- Attribute references in attribute definitions are now resolved at definition time,
  matching asciidoctor behavior. Previously, `:foo: {bar}` followed by `:bar: value`
  would incorrectly expand `{foo}` to `value`; now `{foo}` correctly outputs `{bar}`
  (the literal value stored when foo was defined, before bar existed). ([#291])
- `pass:normal[]` and `pass:n[]` passthroughs now correctly expand attribute references.
  The `normal` substitution group includes `attributes`, but this was previously not
  being checked. ([#291])
- Context-aware backslash stripping for `\^` and `\~` escapes now matches asciidoctor
  behavior. Backslashes are only stripped when they prevent actual formatting (e.g.,
  `\^super^`), preserved as literal text when at word boundaries without closing marker
  (e.g., `\^caret`). ([#278])
- Discrete headings (`[discrete]`) at the end of a document are now parsed correctly
  instead of causing a parsing error. ([#289])
- Paragraphs no longer incorrectly split when a line starts with inline passthrough
  syntax like `+>+`. The list continuation lookahead now only matches actual
  continuation markers (standalone `+` followed by whitespace/EOL/EOF).

### Changed

- **BREAKING**: `TableColumn` struct now includes `colspan`, `rowspan`, `halign`, `valign`,
  and `style` fields.
- **BREAKING**: `BlockMetadata.substitutions` changed from `Option<Vec<Substitution>>`
  to `Option<SubstitutionSpec>`. New types `SubstitutionSpec` and `SubstitutionOp` are
  now public exports. Modifier syntax (`+quotes`, `-callouts`) is now stored as operations
  and resolved by converters with the appropriate baseline (NORMAL vs VERBATIM) rather
  than being eagerly resolved by the parser.

### Removed

- **BREAKING**: Removed `Deserialize` implementation from all model types (`Document`,
  `Block`, `InlineNode`, and ~60 other AST types). Serialization to JSON remains
  supported. If you need to load previously serialized AST, parse the original
  AsciiDoc source instead.


## [Unreleased acdc-cli]

### Added

- **Automatic pager for terminal output** - When using `--backend terminal` and stdout is
  a TTY, output is automatically piped through a pager. Defaults to `less -FRX` on Unix
  and `more` on Windows. Respects the `PAGER` environment variable for custom pagers. Use
  `--no-pager` to disable, or set `PAGER=""`. On Unix, sets `LESSCHARSET=utf-8` for proper
  UTF-8 display. ([#311])
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
[#291]: https://github.com/nlopes/acdc/issues/291
[#311]: https://github.com/nlopes/acdc/issues/311
[#313]: https://github.com/nlopes/acdc/pull/313

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
