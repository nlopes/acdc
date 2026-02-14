# Changelog

All notable changes to `acdc-parser` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-02-14

### Added

- **`style` field on `TocEntry`** — TOC entries now carry the section's style (e.g.,
  `"appendix"`, `"bibliography"`), enabling converters to handle special section rendering
  like appendix letter numbering. ([#343])
- **Book doctype level-0 section (parts) support** — documents with `:doctype: book` now
  accept level 0 sections (parts) in both ATX (`= Part Title`) and setext styles. Added
  `doctype` module with `is_book_doctype()` helper (moved `is_manpage_doctype()` there too).
  ([#312])

### Changed

- **Passthrough quote processing now uses the PEG grammar** — replaced ~800 lines of
  hand-rolled pattern matching (`markup_patterns.rs`) with a dedicated `quotes_only_inlines`
  PEG rule. The new rule matches only formatting markup (bold, italic, monospace, highlight,
  superscript, subscript, curved quotes) without macros, xrefs, or autolinks, matching
  what "quotes" substitution actually means. Fixes edge cases the manual approach missed
  (nested markup, boundary detection) and makes the passthrough processing consistent with
  the main inline parser.
- **`Source::Url` now wraps `SourceUrl` instead of `url::Url`** — the new `SourceUrl` type
  preserves the original URL string for display, preventing the `url` crate from silently
  altering author URLs (e.g., stripping trailing slashes).
  `SourceUrl` implements `Deref<Target = url::Url>` so existing method calls continue to
  work. ([#335])

### Fixed

- **Section IDs now preserve underscores** — `id_from_title()` no longer strips underscores
  within words (e.g., `CHART_BOT` now generates `_chart_bot` instead of `_chartbot`). This
  also fixes broken cross-references and TOC links that depended on correct section IDs.
- **Warnings and errors now reference the correct file for included content** — warnings
  (e.g., trailing macro content, callout list validation) and errors (e.g., mismatched
  delimiters, section level mismatches) from `include::` directives previously reported
  the entry-point file with wrong line numbers. They now resolve the correct source file
  and line via `source_ranges`. ([#337])
- **Constrained markup boundary detection expanded** — `^`, `~`, `(`, `[`, `{`, and `|`
  are now recognized as valid boundary characters for constrained bold, italic, monospace,
  and highlight markup. Fixes cases where formatting wasn't properly terminated before
  these characters (e.g., `*bold*^super^`).
- **Passthrough content locations are now precise** — `RawText` nodes from non-quotes
  passthroughs now point to the content portion only, not the full macro span including
  delimiters.
- **Tables after definition list items now parse correctly** — when a definition list
  term with an empty description was immediately followed by block attributes (e.g.,
  `[cols="..."]`) and a table delimiter, the attributes line was consumed as principal
  text of the dlist item. The `principal_content` continuation loop now has a negative
  lookahead for `attributes_line()`. ([#332])
- **Bare autolinks no longer capture trailing punctuation** — URLs like
  `https://example.com.` now correctly exclude the trailing `.` from the link target.
  A new `bare_url()` rule with balanced parenthesis handling ensures sentence-level
  punctuation (`.`, `,`, `;`, `!`, `?`, `:`) and surrounding parens are not consumed.
- **URL macro display text no longer produces nested autolinks** — display text in
  `http://example.com[http://example.com]` is now parsed with autolinks suppressed,
  preventing the inner URL from being double-linked.
- **URLs are no longer altered during parsing** — trailing slashes and other author-written
  URL details are now preserved exactly as written. Previously, `http://example.com/` was
  incorrectly shortened to `http://example.com`. ([#335])

## [0.4.0] - 2026-02-07

### Fixed

- **Mixed list nesting at 3+ levels** - Mixed ordered/unordered nesting (e.g.,
  `*` → `.` → `**` → `..`) no longer breaks structure at depth 3+. The parser
  now threads ancestor marker context through cross-type nesting boundaries,
  preventing deeply nested items from consuming sibling markers that belong to a
  parent list context.
- Empty table cells (e.g., `|placeholder||`) were silently dropped, causing rows to be
  rejected with "incorrect column count". Empty cells are now preserved. Also fixed
  first-part handling for nested tables using `!` separator. ([#327])
- Markdown-style listing blocks with a language (`` ```ruby ``) now set `style: "source"`
  in metadata, matching the behavior of `[source,ruby]` blocks. This allows converters'
  `detect_language()` to work consistently regardless of syntax used.
- Inline passthroughs `+...+` and `++...++` no longer convert `...` to an ellipsis
  entity (`&#8230;&#8203;`). The root cause was that non-Quotes passthroughs were emitted
  as `PlainText` nodes, which got merged with adjacent text and lost their passthrough
  identity — the converter then applied the block's full substitutions (including
  Replacements). Passthroughs now carry their own substitution list on the `Raw` node
  (`subs: Vec<Substitution>`) instead of a boolean flag, so the converter applies exactly
  the right subs. ([#323])
- Parser warnings (counters, table column mismatches, invalid anchors, etc.) are no longer
  emitted multiple times due to PEG backtracking. Warnings are now collected during
  parsing with deduplication and emitted once after parsing completes. ([#319])

### Changed

- **BREAKING**: `Raw::escape_special_chars: bool` replaced with `Raw::subs: Vec<Substitution>`.
  The new field carries the passthrough's actual substitution list rather than a lossy
  boolean encoding. An empty vec means raw output (no subs), matching `+++` and `pass:[]`.

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
[#319]: https://github.com/nlopes/acdc/issues/319
[#320]: https://github.com/nlopes/acdc/issues/320
[#321]: https://github.com/nlopes/acdc/issues/321
[#323]: https://github.com/nlopes/acdc/issues/323
[#327]: https://github.com/nlopes/acdc/issues/327
[#332]: https://github.com/nlopes/acdc/issues/332
[#312]: https://github.com/nlopes/acdc/issues/312
[#335]: https://github.com/nlopes/acdc/issues/335
[#337]: https://github.com/nlopes/acdc/issues/337
[#343]: https://github.com/nlopes/acdc/issues/343

[0.5.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.5.0
[0.4.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.4.0
[0.3.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.3.0
[0.2.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.2.0
[0.1.4]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.4
[0.1.3]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.3
[0.1.2]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.2
[0.1.1]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.1
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.0
