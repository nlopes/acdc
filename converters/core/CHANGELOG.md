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

- `list::OrderedListNumbering` — resolves an ordered list's explicit `[style]`
  attribute (`arabic`, `decimal`, `loweralpha`, `upperalpha`, `lowerroman`,
  `upperroman`, `lowergreek`) and formats a 1-based item position into its marker
  text, shared by the terminal and manpage backends.
- `section::SpecialSectionTracker` — shared, reusable tracker that decides which
  sections take part in `:sectnums:` numbering. Fed each section (by `SectionKind`)
  in document order, it returns `false` for special sections and their
  subsections, with `[appendix]` excepted (it begins its own numbered sequence).
  Used by the HTML body, HTML TOC, and terminal renderers so the rule lives in one
  place.
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
- **Section numbering utilities** — new `section` module with `SectionNumberTracker`,
  `PartNumberTracker`, `AppendixTracker`, and `to_upper_roman` moved from `acdc-converters-html`
  so they can be shared across converters. Inside an appendix, `SectionNumberTracker`
  numbers subsections with the appendix letter as the top component (`A.1`, `A.1.1`),
  and `AppendixTracker::enter_appendix` returns the heading prefix (`Appendix A: `, or the
  bare `A. ` when the caption is disabled) — both driven by the same letter.
- `#[non_exhaustive]` attribute on `Options`, `GeneratorMetadata`, `toc::Config`,
  `Doctype`, and `IconMode` for semver-safe future additions
- Comprehensive module-level documentation
- `acdc-converters-dev` crate for test utilities (not published to crates.io)
- Visitor method `visit_callout_ref` for processing callout references
- **Copyright and registered escape handling** - `\(C)` and `\(R)` are now recognized as
  escapable patterns alongside `\(TM)`, preventing accidental symbol conversion.

### Fixed

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
  `table_has_spans`) for converters that lack native colspan/rowspan support.
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
