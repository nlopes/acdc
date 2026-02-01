# Changelog

All notable changes to `acdc-parser` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-02-01

### Added

- **Nested tables** - Tables can now be nested but if you're reading this, be warned, a
  lot of this bit was "vibe coded". I did check the code and it's _reasonable_ but I
  wouldn't be surprised if something was missed.
- **Support for 'numbered'** - If we find `:numbered:`, support it in the same way we
  support `:sectnums` (they behave in the same way). I think it's reasonable to support
  and it's a couple of lines of code, therefore I think the tradeoff is more than
  reasonable.

### Fixed

- Description list entries now capture implicit text continuations — non-blank lines
  immediately following a `term:: description` line are included in the same description
  paragraph, matching asciidoctor behavior. Previously these continuation lines were
  parsed as separate paragraph blocks, which also broke multi-entry description lists
  into multiple `<dl>` elements.
- Improved attribute substitutions:
  - authors
  - revision
  - xrefs
- Macro attributes (image, audio, video, icon) now parse `.`, `#`, `%` as literal
  characters instead of interpreting them as shorthand syntax. This matches asciidoctor
  behavior where `image::photo.jpg[Diablo 4 picture of Lilith.]` preserves the trailing
  period in alt text, and `image::photo.jpg[.role]` treats `.role` as literal text, not a
  CSS class. Shorthand syntax remains supported in block-level attribute lines (e.g.,
  `[.class]` before a block).
- Handle multi-line content correctly in table cells
- Handle cross-row continuation lines in tables - when a continuation line (no separator)
  appears after a blank line, append it to the previous row's last cell instead of
  dropping it.
- Detect row boundaries for cell specifier lines in tables. When parsing multi-line table
  rows, lines starting with only a cell specifier (like `a|`, `s|`, etc) now correctly
  start a new row instead of being collected in to the previous row. ([#277])

## [0.2.0] - 2026-01-24

### Added

- **`:leveloffset:` attribute support** - Tracks leveloffset ranges during preprocessing
  and applies them when parsing section headings. Included files can have their heading
  levels adjusted relative to the including document.
- **Bibliography anchor parsing** - `[[[anchor]]]` and `[[[anchor,label]]]` syntax in
  bibliography sections is now parsed as a distinct anchor type.
- **Validation for `:secnumlevels:` and `:toclevels:`** - Parser now warns when these
  attributes are set to values outside the valid range (0-5 for secnumlevels, 1-5 for
  toclevels).
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
- Default table cells now treat list markers, delimited blocks, toc macros, and page
  breaks as literal text instead of parsing them as blocks. Only cells with the `a`
  (AsciiDoc) style get full block parsing, matching asciidoctor behavior.
- Nested include paths are now resolved relative to the parent file's directory instead
  of the root document's directory. ([#317])
- Anchors before section headers (e.g., `[#myid]\n= Section`) are now correctly associated
  with the section. Previously, list continuation and paragraph parsing consumed the
  anchor as content, causing explicit IDs to be lost when using `include::[]` with
  `leveloffset`. ([#321])

### Changed

- **BREAKING**: `TableColumn` struct now includes `colspan`, `rowspan`, `halign`, `valign`,
  and `style` fields.
- **BREAKING**: `BlockMetadata.substitutions` changed from `Option<Vec<Substitution>>`
  to `Option<SubstitutionSpec>`. New types `SubstitutionSpec` and `SubstitutionOp` are
  now public exports. Modifier syntax (`+quotes`, `-callouts`) is now stored as operations
  and resolved by converters with the appropriate baseline (NORMAL vs VERBATIM) rather
  than being eagerly resolved by the parser.
- **BREAKING**: `CrossReference.text` changed from `Option<String>` to `Vec<InlineNode>`,
  enabling rich inline markup in cross-reference text (e.g., `<<id,*bold* text>>`). The
  `with_text()` method now accepts `Vec<InlineNode>`. Serialization outputs `"inlines"`
  array instead of `"text"` string. ([#320])

### Removed

- **BREAKING**: Removed `Deserialize` implementation from all model types (`Document`,
  `Block`, `InlineNode`, and ~60 other AST types). Serialization to JSON remains
  supported. If you need to load previously serialized AST, parse the original
  AsciiDoc source instead.

## [0.1.4] - 2026-01-04

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

## [0.1.3] - 2026-01-02

### Fixed

- Description lists now correctly break when a `[[id]]` anchor is present between items
  ([#269])

## [0.1.2] - 2026-01-02

### Added

- Support for `{blank}`, `{cxx}`, and `{pp}` character replacement attributes
- `SafeMode` enum now exported from `acdc_parser` (migrated from removed `acdc-core`)
- `FromStr` and `Display` implementations for `SafeMode`

### Fixed

- `{empty}` attribute now works in description lists and inline contexts ([#262])
- `{lt}`, `{gt}`, `{amp}` character replacement attributes now produce raw HTML characters
  that bypass escaping, matching asciidoctor behaviour ([#266])

### Changed

- Removed dependency on `acdc-core`

## [0.1.1] - 2025-12-31

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

## [0.1.0] - 2025-12-28

Initial release of acdc-parser, a PEG-based AsciiDoc parser with source location tracking.

[#262]: https://github.com/nlopes/acdc/issues/262
[#265]: https://github.com/nlopes/acdc/issues/265
[#266]: https://github.com/nlopes/acdc/issues/266
[#269]: https://github.com/nlopes/acdc/issues/269
[#274]: https://github.com/nlopes/acdc/issues/274
[#277]: https://github.com/nlopes/acdc/issues/277
[#278]: https://github.com/nlopes/acdc/issues/278
[#279]: https://github.com/nlopes/acdc/issues/279
[#280]: https://github.com/nlopes/acdc/issues/280
[#289]: https://github.com/nlopes/acdc/issues/289
[#291]: https://github.com/nlopes/acdc/issues/291
[#317]: https://github.com/nlopes/acdc/issues/317
[#320]: https://github.com/nlopes/acdc/issues/320
[#321]: https://github.com/nlopes/acdc/issues/321

[0.3.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.3.0
[0.2.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.2.0
[0.1.4]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.4
[0.1.3]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.3
[0.1.2]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.2
[0.1.1]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.1
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.0
