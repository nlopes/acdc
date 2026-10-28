# Changelog

All notable changes to `acdc-parser` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking changes

- Inline images expose identity and roles through `InlineMetadata`, with
  `ElementId` preserving the ID's source location. The owning image's
  `attributes()` map contains scalar controls such as width and link; read alt,
  tooltip, identity, roles, and options separately. Use `set_attribute()` /
  `remove_attribute()` for scalar controls and `options()` / `options_mut()`
  for macro options. Reserved names and aliases return `ReservedAttribute`
  instead of shadowing a dedicated value.

- Reference labels now expose parsed content. Read named `reftext=` through
  `BlockMetadata::reftext` instead of `metadata.attributes["reftext"]`.
  `Anchor::xreflabel`, `TocEntry::xreflabel`, and `Reference::xreflabel` use
  `Option<ContentAttribute>`; native JSON stores inline arrays.

- The parser AST now has the same substitution fields in every build.
  `SubstitutionSpec`, `SubstitutionOp`, and `BlockMetadata::substitutions` no
  longer require `pre-spec-subs`; remove feature guards used only to access
  them. Absent settings stay absent in JSON, and explicit empty settings stay
  `[]`. Block `subs=` settings now work in both compatibility modes and without
  default features. `pre-spec-subs` is a compatibility alias in the parser.
- `InlineMacro::Image` holds `InlineImage`; `Image` represents block images.
  Read `alt` and inline `tooltip` as optional `ContentAttribute` values, including
  explicitly empty content. Icons and links also expose tooltips. Button labels,
  menu labels, and keyboard keys now contain inline content. Native JSON uses
  inline arrays and omits consumed metadata attributes. `InlineMacro::Link`, `Url`, and `Mailto`
  hold boxed structs to keep ordinary inline nodes compact.
- Authored `caption=` content uses `Caption::CustomContent`; read it through
  `BlockMetadata::caption_content()`. Generated labels and counters remain
  separate from serialized content.
- `Video::source` replaces `Video::sources`, including in JSON and `Video::new`.
  It preserves commas and empty playlist slots. Read the provider from `poster`
  instead of synthetic boolean `youtube`/`vimeo` attributes.
- `Title::default()` is absent; `Title::new(Vec::new())` is explicitly empty.
  Use `is_present()` for caption presence and `is_empty()` for inline content.
  Empty titles serialize as `[]` without a synthetic empty text node.
- Named titles, quote attributions, and citation titles serialize only their
  parsed inline content. Read named titles through `title`, rather than
  `metadata.attributes["title"]`. Replacing or removing content cannot leave
  an old attribute value. `Attribution` and `CiteTitle` now share
  `ContentAttribute`; their constructors and slice access remain.

### Added

- `ParseResult::document_ref()` provides typed reads of documents, sections,
  paragraphs, tables, and images, with lazy child and inline iterators. Read table
  declarations separately from resolved presentation, and preserve absent versus
  empty image alt and tooltips. Match block and inline kinds without depending on
  collection storage. Their `attributes()` reads expose titles, image content,
  IDs, styles, role/option tokens, and scalar controls without copying values.
  Lookup and iteration preserve missing, empty, false, and valueless attributes;
  `roles` and `options` aliases share their canonical entries. Existing owner
  APIs and JSON output remain unchanged.

- Parser fixtures support shared expectations or explicit Asciidoctor/Draft
  pairs, optional diagnostics, and stable test names. The runner rejects missing
  or conflicting expectations. The generator shares its parsing rules and fails
  before writing if a selected case has an unexpected outcome. Every case runs
  in both modes; a shared expectation requires identical results.

- Select `CompatibilityMode::Draft` through `OptionsBuilder::with_compatibility_mode()`
  to keep unquoted and double-quoted named titles literal, parse single-quoted
  titles, and accumulate distinct roles/options. `Asciidoctor` remains the default;
  this API change leaves default parsing behavior unchanged. Both modes are
  available in every build and use the same AST. The selected mode applies to
  includes and nested content without affecting other parses. Image alt/tooltip behavior,
  custom substitutions, and Setext controls are unchanged. Draft supports these
  reviewed rules, not the entire unfinished specification.

- `OptionsBuilder::with_force_local_file_xrefs()` resolves file-qualified
  references by their local anchors in combined or partially included documents.
  It preserves URLs and custom text and requires a nonempty fragment. This
  opt-in override can redirect genuine external file references. It does not
  scope duplicate IDs, resolve Antora resource IDs, or change page-only
  references. It deliberately differs from Asciidoctor for files that are not
  fully included.

- Explicit `toc2` settings now produce one deprecation warning per parse.
  Use `:toc: left` or supply `toc=left` through the API or CLI. TOC behavior is
  unchanged. Unsets, ignored assignments, and the `toc2` CSS class do not warn.
  Asciidoctor accepts the deprecated alias without this warning.

### Fixed

- Reference labels retain their authored locations through attribute expansion,
  escaping, includes, and nested blocks. Block anchors use attribute values from
  their declaration point. Asciidoctor mode keeps restricted label formatting;
  Draft treats unquoted and double-quoted named labels as literal text and
  interprets single-quoted content. Empty labels still select title fallback.

- Asciidoctor mode now keeps `[discrete]` and `[float]` headings discrete when a
  later attribute list supplies `style=normal`. Named `style=discrete` alone
  remains a section, matching Asciidoctor. Draft uses the last selected style.
  Section nesting and legacy-float warnings follow the selected heading style.

- Positional block attributes now use the matched block form. Quote delimiters
  accept positional attribution without a `quote` style, and `[,language]` on a
  listing selects that source language. Unrelated block forms no longer acquire
  source or quote attributes from their style alone.
- Draft resolves mapped positional and named attributes in document order,
  including preceding image/video positions. Asciidoctor mode keeps the
  reference's block precedence and limits media dimensions/posters to the macro.
  Later positional macro dimensions now override earlier named values.
- Markdown fence languages no longer shift preceding positions or turn them
  into line-number settings. Only selected attributions register inline macros.

- Stacked block attributes retain earlier positional values across named gaps.
  Draft also retains them across empty positions; Asciidoctor mode clears an
  explicitly empty position. Values keep the location of the selected entry.
- Asciidoctor mode now replaces earlier roles when a later list supplies `role=`,
  including an empty value, and adds same-list shorthand roles after named roles.
  Block media macros retain options from preceding attribute lists. Option
  splitting follows the reference comma/space rules in Asciidoctor mode.
- Ignored duplicate caption attributes no longer replace the selected caption
  or suppress its footnotes when the list contains an attribute reference.
- Draft mode keeps quoted and malformed shorthand literal and supports combined
  anchor/style shorthand. A hash ID overrides an anchor-prefix ID. Block macros
  continue to treat shorthand punctuation as text.
- Draft treats unquoted named `None` as text, including inline-image attributes.
  Asciidoctor mode still ignores it without erasing an earlier value. Quoted and
  positional `None` remain text in both modes.
- Shorthand IDs retain their authored ranges after attribute expansion and no
  longer include the following delimiter in their source location.
- Draft mode uses odd/even backslash rules for attribute references and quotes in
  block lists, and for brackets in block anchor labels. Other backslashes stay
  literal. Asciidoctor mode keeps its compatible escape rules and now recognizes
  block anchor labels containing right brackets, including escaped brackets.
- Raw block attribute locations exclude closing quotes and following commas. Expanded
  values point to their authored references, including Unicode, selected includes,
  and nested table cells.

- Draft mode rejects outer spaces and tabs in block attribute lists and requires
  a comma, space, tab, or list end after a closing quote. Invalid quote enclosures
  stay literal. Block macros use the same rules; inline macro lists are unchanged.
  Both modes now preserve leading-space block attribute lines as content, matching
  Asciidoctor. Includes and source-paragraph comment handling use the selected mode.

- Supported block `subs=` settings no longer produce a specification warning.
  Unknown stages report source recovery and remain in the substitution metadata.
  STEM blocks warn for stages other than attributes and output escaping, while
  preserving formula text and applying the supported stages. Unlike Asciidoctor,
  acdc does not insert formatting tags into formulas.

- Paragraph admonition styles such as `[NOTE,caption='Care']` retain their label and admonition layout.

- Image and icon alternative content, tooltips, custom captions, and UI labels
  retain supported inline formatting, escapes, substitutions, and source locations.
  Only selected values register footnotes. Literal quote attributions and citation
  titles retain dashes; single-quoted values support normal substitutions even
  when the body uses `subs=none`. Named and positional quote values retain their
  declaration-time attributes. Asciidoctor mode expands introduced references;
  Draft leaves them literal unless explicit passthrough substitutions request it.
  Explicitly empty attribution and citation content serializes as `[]`.
- Single-quoted `caption=` prefixes parse formatting introduced by attribute
  expansion and retain the declaration's attribute values. Asciidoctor mode
  expands introduced references; Draft keeps them literal unless explicitly
  requested. Literal and media-macro captions stay literal, and caption source
  locations cover the authored references and escapes.
- Icon positional values always select size; named alternative text remains
  independent. Repeated icon attributes use the last value and retain link options.

- Named titles parse formatting introduced by attribute expansion, such as
  `[title='{word}']` with `:word: *Bold*`. Asciidoctor mode also expands references
  introduced by that first expansion; Draft does not do so implicitly and parses
  only single-quoted titles. Both modes retain declaration-time values, even
  where Asciidoctor uses a later value for an introduced reference.
- Inline source spans cover complete attribute references and escaped characters.
  Single-character content no longer includes a following delimiter in its byte range.

- Named `title=` attributes now supply formatted titles on paragraphs, lists,
  delimited blocks, tables, audio, and video. Media macro titles override preceding
  titles. Repeated named attributes use the last value; repeated `opts` accumulate.
- Images honor named `alt` over positional alt text and retain literal quotes
  and empty positional slots. Inline `title=` remains a tooltip and no longer supplies alt text.
  Video `poster=` overrides its positional value, including platform names.

- Block images accept `title=` captions in the macro or a preceding attribute
  list. Macro titles override preceding titles, retain inline formatting, and
  supply cross-reference text, matching Asciidoctor. Inline-image `title=`
  tooltips remain independent of caption titles.

- Errors and warnings in reindented includes report columns in the original
  source, including nested and selected includes.

- STEM notation now uses AsciiMath when `:stem:` is absent, empty, or unknown.
  The `latex` and `tex` aliases select LaTeX. Explicit `[latexmath]` and
  `[asciimath]` blocks and positional notation override the document value,
  matching Asciidoctor.
- Inline STEM keeps attribute references as formula text by default.
  LaTeX removes one enclosing dollar pair.
  `[stem,subs="+attributes"]` expands block attributes while retaining the
  original source text and locations.

## [0.11.0] - 2026-10-10

### Breaking changes

- The parser API now defaults to `SafeMode::Secure`, matching the Asciidoctor API.
  Includes become links without reading their targets. Select Safe or Unsafe
  to enable includes. `parse_file` still reads the entry document.
  Custom providers do not change the safe mode.

### Added

- `OptionsBuilder::with_include_loader()` selects custom include sources or
  disables loading with `IncludeLoader::Disabled`. Custom providers can supply
  unsaved editor buffers, virtual files, or authorized HTTP(S) content. Safe modes,
  caller attribute policy, and source recovery also apply to custom providers.

### Changed

- All includes in one parse now share a budget, with a default of 64 MiB. Each include counts
  its selected UTF-8 text after `indent=`, plus a separating newline for nonempty
  text. Nested and repeated includes use the same budget. Exceeding it stops
  parsing with `Error::IncludeExpansionTooLarge` at the include directive.
  Set `OptionsBuilder::with_max_total_include_bytes()` to change the budget in
  bytes. Document attributes cannot change it. The entry document does not count
  toward this budget; safe modes and the per-include limit still apply.
  This limit deliberately differs from Asciidoctor.
- Each include can select up to 10 MiB of normalized UTF-8 text, before and after `indent=`.
  Use `lines=`, `tag=`, or `tags=` to select a small part of a larger source.
  Larger selections fail with `Error::IncludeSourceTooLarge`.
  Excess tag metadata causes `Error::IncludeSelectionTooComplex`.
  The built-in HTTP(S) loader also limits bytes read after decompression to 10 MiB.
  These fixed limits differ from Asciidoctor and do not apply to the entry document.
- Finite `lines=` selections stop at the highest requested line, as Asciidoctor does for local files.
  Errors after that line do not affect the result, and HTTP responses close early.
  Each line loses its trailing whitespace independently, so later text cannot change the selected text or its source locations.
  This includes Unicode whitespace, which Asciidoctor preserves.
  Selected blank final lines count the same whether more lines follow or not.
  Full includes, open-ended ranges, and tag selections still read to the end of the source.

### Fixed

- Parser tracing no longer dumps document text, options, AST nodes, paths, or URLs.
  Warning logs contain categories and positions. Read full warnings from the
  parse result and full errors from the returned `Error`.
- The parser now builds for `wasm32-unknown-unknown` with `network` or `--all-features`.
  This target has no built-in HTTP client.
  Attempts to load HTTP(S) includes produce a warning and keep the unresolved directive.
  To use remote includes, fetch content before parsing and supply it through a custom `IncludeSourceProvider`.

## [0.10.0] - 2026-10-08

### Breaking changes

- The minimum supported Rust version is now 1.88, up from 1.85.
- Attribute configuration now uses iterators. `OptionsBuilder::build()` and
  `Options::with_attributes()` return `Result`; add `?` to handle invalid values.
  Read `Options::document_attributes()` instead of its former public field.
- `DocumentAttributes` is read-only. Its `get()` and `iter()` methods borrow
  `DocumentAttributeValue`, with `as_str()`, `as_integer()`, `is_presence()`, and
  `text()` for reads. Configure values through `Options::builder()` instead of
  `insert()`, `set()`, `remove()`, or `merge()`; `get_string()` is removed.
- `Document::attributes` contains the end-of-header values. Later changes are
  ordered `Block::DocumentAttribute` events. Read each event with `assignment()`
  instead of the removed `value` field, and handle both set and unset assignments.
- `Position::line` and `column` are `u32`. Use `Position::new()` or
  `Position::from_line_col()` to construct positions. Each boundary can carry its
  include chain. `SourceLocation::location` replaces `positioning`, and
  `Positioning` is removed. The three `Location::shift*()` methods are removed.
- `TocEntry::kind` replaces `numbered` and `style`. Use `SectionKind` for special
  sections and `TocEntry::number()` for the assigned number.
- Index terms contain inline nodes instead of strings. Update matches on
  `IndexTermKind` and calls to `term()`, `secondary()`, and `tertiary()`.
  `InlineMacro::IndexTerm` is boxed. JSON index labels are now inline-node arrays.
- `SubstitutionSpec`, `SubstitutionOp`, and `BlockMetadata::substitutions` require
  the default-on `pre-spec-subs` feature. Parsed lists use the new
  `SubstitutionSpec::Source` variant and retain their entries in JSON.
  Replace `macros_disabled()` and `attributes_disabled()` with
  `BlockMetadata::uses_substitution()` or `SubstitutionSpec::resolve()`.
- `inlines_to_string()` is removed. Applications that need plain text must choose
  how to handle formatting and macros when walking inline nodes.

See the [migration guide](README.md#migrating-from-09) for examples.

### Added

- A reference catalog for sections, blocks, inline anchors, and bibliography
  entries. Title-based cross-references and references to fully included files
  resolve within the document without loading more files.
- Section kinds and numbers, block captions, table presentation, and `xrefstyle`
  metadata for custom converters. Sections and captions can be renumbered after
  editing the document.
- Index `see` and `see also` relationships, `anchor:id[text]` macros, and
  positional `mailto:` subject and body values.
- Block metadata access, retained positional attributes, and block source text
  before inline substitutions. Source locations resolve to the original included
  files, including partial includes.
- `ParseResult::source_recovery()` reports content that was omitted or recovered,
  even after `take_warnings()`. Duplicate IDs and conflicting named footnotes
  report both source locations; the first definition wins.
- `OptionsBuilder::with_base_dir()` controls include resolution.
  `skip-front-matter` removes YAML front matter and exposes it as an attribute.
  `SOURCE_DATE_EPOCH` gives reproducible UTC timestamps.

### Changed

- Application attributes take precedence over document entries. Use
  `with_defaults()` for values the document can replace. Read-only and API-only
  attributes are protected; `outdir` and `outfile` are unavailable during parsing.
  Literal `true` and `false` values remain text; use `:name!:` to unset a value.
- Substitutions follow their written order. Normal content processes quotes before
  attributes. `subs="quotes,+attributes"` replaces block defaults;
  `subs="+quotes,attributes"` keeps them. Keyboard, button, and menu macros require
  the `experimental` attribute.
- Includes default to a depth limit of 64. Set `max-include-depth` through parser
  options using a non-negative decimal string. Invalid values fail configuration;
  `0` leaves directives literal and reports source recovery.
- Secure mode does not read includes. Remote reads require the `network` feature
  and caller-supplied `allow-uri-read`; a document cannot grant this permission.
  Remote content cannot include local files. Safe and Server modes keep local
  paths beneath the include base, but do not resolve symlinks.
- Fixed limits protect against excessive allocation: 10 MiB per decoded remote
  include, 4,096 include-indent spaces, and 100 columns by 1,000 rows per table.
  These limits intentionally differ from Asciidoctor and cannot be raised.
- Parsing uses less memory for attributes and repeated cross-references, and is
  faster for large verbatim blocks.

### Fixed

- Include selection, nested tags, conditionals, encodings, and source locations.
  Missing or unreadable includes retain an unresolved directive and a warning;
  optional missing files are skipped. Disabled includes report source recovery.
- Attribute precedence, header author and revision parsing, comment handling,
  source-order body attributes, and attribute isolation in AsciiDoc table cells.
- Inline formatting, passthroughs, Unicode boundaries, escaped delimiters, links,
  footnotes, and index terms. Nested labels retain their text and source locations.
- List nesting and continuations, callouts, styled code and verse paragraphs,
  multiline table cells, spans, and repeated cells. Unterminated blocks and
  incomplete tables now retain usable content with located warnings.
- Generated section IDs, reference labels, bibliography entries, captions, and
  section numbering. Repeated headings get unique IDs, and explicit duplicate
  IDs keep the first target.

Most fixes follow Asciidoctor. Deliberate differences, including nested inline
markup, stricter include settings, and recovery warnings, are documented in the
[README](README.md#deliberate-divergences-from-asciidoctor).

## [0.9.0] - 2026-04-26

### Packaging

- Shrunk the published tarball by excluding developer-only files: `AGENTS.md`,
  `README.adoc` (duplicate of `README.md`), `benches/`, `examples/`, `fixtures/`,
  `proptest-regressions/`, and `tests/`.

### Added

- **`preprocess` and `grammar_parse` tracing spans** in all parse entry points (`parse`,
  `parse_file`, `parse_from_reader`) for phase-level timing visibility.
- **First-section level validation** - emit a warning when a titled document's first
  section skips level 1 (e.g. starts with `===` instead of `==`), matching `asciidoctor`'s
  "section title out of sequence" check. Title-less documents still accept any
  first-section level.
- **Unterminated table recovery** - when a table's opening delimiter (`|===`,
  `!===`, `,===`, `:===`) runs to end of input without a matching close, emit
  a `WarningKind::UnterminatedTable { delimiter }` warning carrying the
  literal opening token and still produce a table block from the content
  (matching `asciidoctor`'s "unterminated table block" warning and recovery).
- **`ParseResult` and `ParseInlineResult`** — new return types from `parse_*`. Each
  bundles the AST, source text, and any non-fatal warnings. Access via `.document()` /
  `.inlines()`, `.source()`, `.warnings()`, and `.take_warnings()`. Marked `#[must_use]`
  so warnings aren't silently dropped.
- **`Warning` and `WarningKind`** — non-fatal parser diagnostics are now returned as
  typed values with `.source_location()` and `.advice()` accessors, mirroring `Error`.

### Changed

- **BREAKING**: `parse*()` now returns `Result<ParseResult, Error>` (and
  `Result<ParseInlineResult, Error>` for `parse_inline`) instead of
  `Result<Document, Error>` / `Result<Vec<InlineNode>, Error>`. Use `.document()`
  / `.inlines()` to borrow the AST.
- **Warnings are exposed on `ParseResult::warnings()`** instead of flowing only
  through `tracing`. Every warning — grammar (unknown table format, bad row column
  count, callout gaps, anchor whitespace, first-section level, trailing macro
  content, counter references, experimental `subs=`) and preprocessor (missing
  include files, URL includes blocked by safe-mode, missing `allow-uri-read`,
  `network` feature disabled for remote includes, invalid include line numbers,
  `ifdef`/`endif` attribute mismatches) — now carries a `SourceLocation` with an
  optional file path. `tracing::warn!` emission is kept as a fallback.
- **Long-running consumers no longer leak memory per parse.** Embedders like
  `acdc-lsp` and `acdc-editor-wasm` can parse repeatedly with a flat memory
  footprint.
- **BREAKING**: `Link.text` changed from `Option<&'a str>` to `Vec<InlineNode<'a>>` so
  inline markup inside `link:` macros is preserved as structured nodes. `Link::new`
  initialises it as `Vec::new()`; `Link::with_text` now takes `Vec<InlineNode<'a>>`.
- `Link`, `Url`, and `Mailto` now serialize their text under an `inlines` key on the ASG
  (consistent with `xref`/`footnote`), only when non-empty. Previously the text was not
  serialized at all.

### Fixed

- **Whitespace-only text in `link:`, URL, `mailto:`, and `xref:` macros is
  preserved literally** instead of falling back to the target.
  `link:https://example.com[ ]` now renders as `<a href="https://example.com"> </a>`
  and `xref:section[ ]` renders as `<a href="#section"> </a>`, matching
  asciidoctor; previously acdc dropped the whitespace and emitted the URL or
  section title as the anchor's visible text. The shorthand form `<<section, >>`
  still falls back to the section title, also matching asciidoctor.
- **Inline markup inside `link:` macro text** — `link:url[*bold*]` now parses nested
  formatting (bold, italic, monospace, passthroughs, etc.) through the full inline
  grammar, matching `url:` / `mailto:` behaviour.
- **Diagnostics inside `a`-style table cells now resolve to the offending
  line, not the cell's `a|` style prefix.** The recursive parse over an
  AsciiDoc cell anchored at the cell-token offset rather than the
  content-byte offset, so warnings (e.g. `UnterminatedTable` from a nested
  `!===`) and inline locations were reported one or more lines off and
  one column off. Inner-cell positions are now correct, which also fixes
  off-by-one columns in nested-table fixtures.
- **Trailing text after a completed table row is now a continuation
  paragraph of the last cell**, matching asciidoctor. Previously the text
  was collapsed into the preceding line (top-level tables) or dropped
  entirely (nested tables inside `a`-cells). The non-`a` cell block parser
  also now consumes blank lines between iterations, so multiple
  paragraphs in a cell render as separate `<p class="tableblock">` blocks.

### Removed

- **BREAKING**: `impl FromStr for Source<'static>` is removed. It leaked a boxed
  `String` to `'static` on every call, accumulating to multi-GB RSS in long-running
  LSP sessions. If you need an owned copy, convert to your own type.

### Performance

- **Parsing large documents is dramatically faster.** Hot paths in inline parsing and
  attribute handling skip work entirely when the input has no relevant syntax.
- **Macro-heavy documents parse much faster.** Share `FootnoteTracker` across inline
  sub-parses via `Rc<RefCell<_>>` to avoid quadratic deep-clone cost on every nested
  `process_inlines`.
- **Broad parser speedup (~15–20% on large docs)** from a four-part sweep of the
  `position()` / `LineMap` hot path: a byte-only fast path in
  `section_level_at_line_start` (runs as a negative lookahead on every paragraph
  continuation line), a monotonic last-line cache in `LineMap` (consecutive offsets skip
  the binary search), a signature change on `process_inlines` /
  `preprocess_inline_content` to take a raw `usize` offset instead of a
  `&PositionWithOffset`, and byte-lookahead guards on every alternative of the inline
  macro alternation in `non_plain_text()` to prune speculative dispatch.
- **Macro-heavy documents parse ~14% faster**; prose-heavy documents 3–6% faster.

## [0.8.0] - 2026-03-28

### Added

- **Fragment support in `xref:` and `link:` macros** — targets like `xref:file.adoc#anchor[text]`
  and `link:page.html#section[text]` now parse correctly. The `#fragment` is optional and only
  applies to `xref:` and `link:` macros (not `image::`, `video::`, etc.).
- **Public `InlineNode::location()` method** — provides direct access to the source location of
  any inline node without requiring the `Locateable` trait.
- **`Locateable` trait for `InlineNode`** — `InlineNode` now implements `Locateable`, providing
  direct access to location information without pattern matching on variants.
- **Rich inline markup in document titles and subtitles** — document titles now support bold,
  italic, monospace, links, macros, and other inline markup, matching section title behavior.
  Previously, titles were always rendered as plain text.
- **`subs=macros` substitution type** — `[subs=-macros]` and explicit lists without `macros`
  now gate macro grammar rules at parse time. When macros are disabled, inline macros
  (links, xrefs, images, footnotes, index terms, etc.) are treated as plain text.
  Requires the `pre-spec-subs` feature flag.
- **Include `indent` attribute** — `include::file.rb[indent=2]` now re-indents included content
  to the specified level, matching asciidoctor behavior. Strips existing leading whitespace and
  prepends the specified number of spaces. `indent=0` removes all leading whitespace.
- **`strip_quotes` utility function** — centralised helper to strip matching single or double
  quotes from attribute values, replacing scattered `trim_matches('"')` calls throughout the
  codebase.
- **Single-quoted attribute values** — attribute values can now use single quotes (`'value'`)
  interchangeably with double quotes (`"value"`), matching asciidoctor behavior. Applies to
  block attributes, macro positional/named values, link titles, and table column specs.
- **Compound author names** — author lines now support multi-word first, middle, and
  last names (e.g., `Jan de Groot`), matching asciidoctor behavior.
- Bidirectional sync between `Header.authors` and document attributes: the `:author:`
  document attribute now populates `Header.authors`, and parsed author lines now set
  `author`, `authors`, `firstname`, `lastname`, `middlename`, `authorinitials`, `email`,
  and `authorcount` document attributes

### Performance

- **Inline parsing up to 39% faster** — added character-class pre-filter and lookahead guards
  to `plain_text` and `quotes_plain_text` rules. Characters that cannot start any inline
  construct are now consumed in bulk without running 28+ negative lookahead checks per
  character. Remaining trigger characters use grouped character-class guards to skip
  irrelevant rule evaluations.

### Changed

- **Roles are now space-separated** — `role='a b'` produces two roles (`a`, `b`) instead of
  one, matching asciidoctor's space-separated role semantics.
- **`parse_comma_separated_values` simplified** — no longer handles quote stripping internally
  since quotes are now stripped upstream by `strip_quotes`.

### Fixed

- **`specialcharacters` not recognized as a substitution name** — `[subs="specialcharacters"]`
  now works as an alias for `specialchars`, matching asciidoctor behavior.
- **Stacked block attributes overwrite previous values** — when block attributes are spread
  across multiple lines (e.g., `[source,ruby]` followed by `[subs="+attributes"]`), the second
  line no longer overwrites the style, positional attributes, id, substitutions, attribution,
  or citetitle from the first line. Only explicitly provided values are merged.
- **Incorrect locations for inline text inside `xref:`, `url:`, and `mailto:` macros** — text
  nodes inside these macros (e.g., "Section Title" in `xref:file#id[Section Title]`) had wrong
  line and column numbers. The line was always reported as 1 regardless of actual position, and
  the column was relative to the start of the macro instead of the text content. Both issues are
  now fixed: grammar rules capture the correct content start position, and the location mapper
  remaps nested text nodes to document-absolute coordinates.
- **Roles with spaces were not split** — `image::foo.jpg[role="thumb bordered"]` now correctly
  produces two separate roles (`thumb`, `bordered`) instead of one combined string.
- Boolean/valueless attributes (e.g., `:set-attr:`) now expand to an empty string when
  referenced as `{set-attr}`, matching asciidoctor behavior
- Constrained formatting (bold, monospace, highlight) no longer incorrectly expands
  inside constrained italic `_..._` delimiters, matching asciidoctor behavior. The
  underscore is a word character, so it prevents nested constrained marks at the boundary.
- Backslash escaping of character replacements (`\--`, `\...`, `\->`, `\<-`, `\=>`, `\<=`,
  `\(C)`, `\(R)`, `\(TM)`) now correctly suppresses typography substitutions, matching
  asciidoctor behavior
- Expand attributes inside `pass:a[]` content when macros disabled via `subs=-macros`,
  matching asciidoctor behavior
- Fixed passthrough preprocessor bypassing `subs=-macros` gating — `pass:[]` macros and
  inline passthrough syntax (`+...+`, `++...++`, `+++...+++`) are now treated as literal
  text when macros are disabled, matching asciidoctor behavior
- Fixed non-monotonic inline positions for subscript/superscript text preceded by short plain text

## [0.7.0] - 2026-02-25

### Added

- **`latexmath:[]` and `asciimath:[]` inline macros** — explicit notation overrides that
  set the stem notation directly instead of resolving from the `:stem:` document attribute.
- **`BlockMetadata.location`** — metadata blocks now carry an `Option<Location>` tracking
  their source position (attribute lines, anchors, titles).
- **`DelimitedBlock.open_delimiter_location` / `close_delimiter_location`** — delimited
  blocks now carry precise locations for both the opening and closing delimiter lines.
- **`DescriptionListItem.delimiter_location`** — description list items now carry the
  source location of their delimiter (`::`, `:::`, etc.).

## [0.6.0] - 2026-02-23

### Added

- **Attribution and CiteTitle types** — blockquote attributions and citation titles are now
  stored as `Attribution` and `CiteTitle` types (containing `Vec<InlineNode>`) on `BlockMetadata`,
  instead of plain strings in the attributes map. This enables inline content (links, formatting)
  in attributions. ([#357])
- **Quoted paragraphs produce DelimitedQuote** — the `"text" -- Author` shorthand now produces
  a `DelimitedBlock` with `DelimitedQuote` content, matching the markdown blockquote and
  `[quote]` block output structure.

### Fixed

- **`stem:[]` backslash escaping for brackets** — `stem:[\]` and `stem:[\[]` now correctly
  strip the backslash and preserve the bracket. Previously, `balanced_bracket_content` treated
  `\]` as a closing bracket, breaking expressions like `stem:[[[a,b\],[c,d\]\]((n),(k))]`.
  Stem now uses a dedicated `escaped_bracket_content` rule.
- **Block delimiters in source block content** — lines inside a delimited block containing
  a longer sequence of the same delimiter character (e.g., `--------------------` inside a
  `----` block) are no longer incorrectly treated as closing delimiters. The parser now
  requires an exact match of the opening delimiter length. ([#349])

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
[#349]: https://github.com/nlopes/acdc/issues/349
[#357]: https://github.com/nlopes/acdc/issues/357

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.11.0...HEAD
[0.11.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.10.0...acdc-parser-v0.11.0
[0.10.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.9.0...acdc-parser-v0.10.0
[0.9.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.8.0...acdc-parser-v0.9.0
[0.8.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.7.0...acdc-parser-v0.8.0
[0.7.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.6.0...acdc-parser-v0.7.0
[0.6.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.5.0...acdc-parser-v0.6.0
[0.5.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.4.0...acdc-parser-v0.5.0
[0.4.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.3.0...acdc-parser-v0.4.0
[0.3.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.2.0...acdc-parser-v0.3.0
[0.2.0]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.4...acdc-parser-v0.2.0
[0.1.4]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.3...acdc-parser-v0.1.4
[0.1.3]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.2...acdc-parser-v0.1.3
[0.1.2]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.1...acdc-parser-v0.1.2
[0.1.1]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.1.0...acdc-parser-v0.1.1
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-parser-v0.1.0
