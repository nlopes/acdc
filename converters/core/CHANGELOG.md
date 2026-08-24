# Changelog

All notable changes to `acdc-converters-core` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Performance

- **Typography replacements skip the replace chain on plain prose.** Text
  without special characters (arrows, ellipses, escapes) costs nothing per
  paragraph.

### Added

- Automatic cross-references to captioned blocks support source-order
  `xrefstyle=short` and `xrefstyle=full`, including custom and disabled
  captions. Explicit reference labels still take precedence.
- Plain-text output uses the visible, substituted text of index terms without
  formatting markers.
- Link fallback labels can omit Asciidoctor-compatible URI prefixes while
  preserving the complete destination.
- Source blocks have shared handling for line numbers, custom starting numbers,
  and highlighted-line selectors.
- `xref::resolve_xref` resolves an automatic cross-reference's display content
  with Asciidoctor's precedence — explicit reference label, target title, then
  `[id]` — and distinguishes an untitled target, a missing local target, an
  inter-document target, and a reference nested inside another one's text. Its
  `xref::XrefGuard` keeps a target whose reference text holds a reference of its
  own from recursing. Asciidoctor falls back to `[refid]` at the same point,
  except where it reuses a target's cached converted title, which can carry one
  already-resolved level and makes its result depend on document order.
- `shows_block_title` answers whether a delimited block shows its title as a
  visible caption, so every backend applies the passthrough rule (title is
  reference text only) the same way.
- `inline_text::InlineTextTransform` now carries the text of every inline node,
  so a heading or caption loses nothing it cannot render as markup: a link
  contributes its link text, an image its alt text, an icon its alternative
  text (or a readable form of its target), a
  footnote its marker, and a stem its content; asciidoctor brackets the last
  four, and so does this. A reference to an unknown target reads as its stylized
  id, which drops a file extension so `other.adoc#part` reads as `[other#part]`. Two options extend it — `decode_char_refs` turns
  `&#39;` into a character for non-HTML backends, and `references` resolves a
  cross-reference with no text of its own to its target's reference text,
  falling back to `[id]` for an unknown target or for a reference inside another
  target's reference text. `xref::reference_text` exposes that precedence on its
  own. A standalone curved apostrophe extracts as the typographic character
  rather than an ASCII quote.
- Converter backends can declare their Asciidoctor-compatible backend,
  base-backend, file-type, output-suffix, and HTML-syntax traits and apply the
  corresponding intrinsic and doctype convenience attributes consistently.
- `inline_text::InlineTextTransform` and `inlines_to_string()` provide shared
  plain-text extraction from inline nodes for converters and tooling, including
  configurable hard-line-break rendering.
- **`#` callout guards in shell-session and PowerShell blocks** — `[source,console]`,
  `[source,terminal]`, and `[source,powershell]`/`[source,ps1]` source blocks use `#`
  as their line-comment prefix, so a guarding `# <1>` is stripped from the rendered
  line just like it is for `bash`/`sh`/`zsh`/`fish`.
- `list::OrderedListNumbering` — resolves an ordered list's explicit `[style]`
  attribute (`arabic`, `decimal`, `loweralpha`, `upperalpha`, `lowerroman`,
  `upperroman`, `lowergreek`) and formats a 1-based item position into its marker
  text, shared by the terminal and manpage backends.
- `substitutions::effective_subs(spec, is_verbatim)` — shared resolver for
  per-block `[subs="…"]` lists against the `NORMAL` / `VERBATIM` baselines.
  Previously lived in the HTML converter; promoted so terminal, manpage, and
  future backends can honour `subs=` uniformly.
- **Structured converter warnings** — `Warning`, `WarningSource`,
  and `Diagnostics` let converters return non-fatal user-facing warnings
  alongside `ConversionResult` without baking backend-specific warning categories
  into the core crate.
- Converter conversion methods now return output metadata containing the written
  file path when applicable.
- **Typography replacements API** — `Replacements` struct, `apply()`, and
  `replace_apostrophes()` for shared AsciiDoc `Replacements` substitution across
  converters. Includes `Replacements::unicode()` for terminal/manpage output.
- `replace_em_dashes()` — standalone function for em-dash pattern matching, shared
  by converters that need format-specific em-dash output (e.g. HTML entities).
- `#[non_exhaustive]` attribute on `Options`, `GeneratorMetadata`, `toc::Config`,
  `Doctype`, and `IconMode` for semver-safe future additions
- Comprehensive module-level documentation
- `acdc-converters-dev` crate for test utilities (not published to crates.io)
- Visitor method `visit_callout_ref` for processing callout references
- **Copyright and registered escape handling** - `\(C)` and `\(R)` are now recognized as
  escapable patterns alongside `\(TM)`, preventing accidental symbol conversion.

### Fixed

- Media targets used as URIs resolve relative paths against `imagesdir`, use
  forward slashes, normalize path segments, and encode spaces as `%20`.
- Icon mode selection now treats any set `icons` value other than `font` as
  image mode, matching Asciidoctor.
- Link fallback text distinguishes `link:`, `mailto:`, and automatic links, so
  the HTML and PDF backends match Asciidoctor mail targets and angle brackets.
- Inter-document cross-references preserve their external target and let each
  backend use its own output suffix and fallback filename.
- Universal AsciiDoc defaults passed to parsers no longer act like caller-set
  attributes, so nested documents can change them locally.
- Built-in converters now use the parser's section numbers for both headings and
  table-of-contents entries. Source-order changes and nested documents therefore
  use one sequence in every backend.
- Special-section numbering now treats book abstracts as chapters, honors
  `sectnums=all`, and keeps the ordinary section sequence across appendices.
- Book table-of-contents numbering now keeps chapter numbers continuous across
  part boundaries.
- Book chapter numbers continue across part boundaries instead of restarting,
  matching Asciidoctor.
- Spaced em dashes now consume one adjacent space or newline on each side,
  preserve the rest of a whitespace run, ignore tabs, and distinguish true
  paragraph boundaries from inline formatting boundaries.
- `--no-default-features` builds no longer re-enable parser default features
  through the shared parser dependency.
- Video URL generation now reports missing video sources with a dedicated error.
- **Em-dash patterns now match asciidoctor** — spaced (`word -- word`) emits
  thin-space + em-dash + thin-space; word-bounded (`word--word`) emits em-dash +
  zero-width-space. Patterns like `word --word`, `word-- word`, `test--`, `--test`,
  and `---` are correctly left unchanged.
- **Em-dash boundary replacement inside inline spans** — `replace_em_dashes` and
  `Replacements::apply` now accept a `string_boundaries_are_space` parameter. When
  `false`, string start/end are not treated as whitespace, preventing `--` inside
  inline formatting (bold, italic, monospace, etc.) from being incorrectly converted
  to an em-dash.
- Shared table grid utilities (`build_grid`, `CellKind`, `GridRow`, `determine_column_count`,
  `table_has_spans`) provide normalized cell placement and span tracking for converters.
- Output file creation now creates parent directories if they don't exist, so
  `-o path/to/nonexistent/dir/file.html` works without pre-creating the directory
  tree. ([#358])
- Preamble wrapper now only renders when all conditions are met: document has a title,
  contains at least one section, and has content before that section. Previously,
  documents without sections incorrectly rendered preamble wrappers. ([#275])

### Changed

- **BREAKING**: `Converter::write_to`, `derive_output_path`, and the
  provided `convert*` methods now accept `&Document<'_>` of any lifetime
  instead of `&Document<'a>` tied to the converter's stored-attribute
  lifetime. Stored attributes still use `'a`; the per-call doc lifetime is
  independent, so a `Converter<'static>` can convert short-lived parsed
  documents without leaking or `to_static`-ing them.
- Source blocks accept any language name from position 2. Empty or named slots
  no longer cause a later value to be treated as the language.
- **BREAKING**: Renamed crate from `acdc-converters-common` to `acdc-converters-core`
- **BREAKING**: `Options` struct now uses builder pattern with private fields -
  use `Options::builder().doctype(...).build()` instead of struct construction
- **BREAKING**: `toc::Config` fields are now private - use accessor methods
  (`placement()`, `title()`, `levels()`, `toc_class()`)
- **BREAKING**: Removed `Backend`, `Options::backend`, and
  `OptionsBuilder::backend(...)`. Variant choice lives in each converter crate
  (`HtmlVariant`, `MarkdownVariant`) and is set via `Processor::with_variant` /
  `Processor::new_with_variant`. Use `Converter::name(&self) -> &'static str`
  instead of `Converter::backend(&self) -> Backend`.
- **BREAKING**: Renamed `Processable` trait to `Converter` with new output routing:
  - New `OutputDestination` enum for routing output (stdout, file, buffer)
  - `convert()` is now a provided method that handles output routing
  - Required methods: `convert_to_stdout()`, `convert_to_file()`
  - New helpers: `write_to()`, `derive_output_path()`, `after_write()` ([#313])

[#275]: https://github.com/nlopes/acdc/issues/275
[#313]: https://github.com/nlopes/acdc/pull/313
[#358]: https://github.com/nlopes/acdc/issues/358
