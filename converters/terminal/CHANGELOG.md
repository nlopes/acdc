# Changelog

All notable changes to `acdc-converters-terminal` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Performance

- **Streaming output.** Rendering writes directly to the caller's `Write`
  target, keeping allocations flat on large documents.

### Added

- Figures, tables, examples, listings, and source blocks now render captions
  with the labels active at each block's source position. Custom and disabled
  captions, inner-first numbering, and changes made part-way through a document
  match Asciidoctor.
- Cross-references preserve formatted explicit text through supported nested
  inline macros. Empty references to captioned blocks honor source-order
  `xrefstyle=basic`, `short`, and `full`, including custom and disabled
  captions.
- Visible index terms and generated catalog labels preserve inline formatting,
  links, and attribute substitutions.
- Inline icon labels use their explicit alternative text, or a readable form
  of their target when no alternative is set.
- Ordered lists honor a positive `start` value, including on nested lists.
- Tables honor `frame`, `grid`, and static `stripes` values, including
  source-order `table-frame`, `table-grid`, and `table-stripes` defaults.
  `stripes=hover` has no effect in non-interactive terminal output.
- Terminal conversion attributes now include the `terminal` backend,
  base-backend and file type, `.terminal` output suffix, and their conditional
  convenience attributes.
- An ordered list with an explicit numbering style (`[loweralpha]`, `[upperalpha]`,
  `[lowerroman]`, `[upperroman]`, `[lowergreek]`, `[arabic]`, `[decimal]`) renders
  its markers in that style (e.g. `a.`, `IV.`, `α.`) instead of always `1.`, `2.`.
  `[none]`, `[no-bullet]`, and `[unstyled]` omit ordered and unordered markers,
  while `[unnumbered]` omits ordered markers. Markerless checklists keep their
  checkbox.
- Terminal replay frame capture (`replay::capture` / `capture_windowed`) turns
  recorded ANSI into ordered, deduplicated `CellGrid` frames for animated replay
  renderers; `capture_windowed` is a fast path for append-only recordings. Each
  captured cell keeps its palette index alongside the resolved colour, so a
  player can re-resolve it against a recording's own palette.
- The `asciicast` module parses asciicast v2/v3 (`.cast`) recordings into a
  `Recording` of replay frames (via `asciicast-rs`). Recorded commands and input
  are never executed; long idle gaps are compressed (the header's
  `idle_time_limit`, a caller override, or a default), and the recording's theme,
  palette, and title are exposed for faithful playback.
- `[subs="-replacements"]` on a paragraph now keeps typography source (`--`,
  `(C)`, `->`, `...`) literal instead of converting to Unicode.
- User-facing converter warnings are now collected in `ConversionResult` for
  recoverable terminal conversion issues such as image display failures and
  unsupported delimited block fallbacks.
- **`[listing]` and `[source]` styled paragraphs** — paragraphs with `[listing]` or
  `[source,lang]` style now render as preformatted text (same as `[literal]`).

- A `<<id>>` with no text reads as the target's reference text: an explicit
  `[[id,label]]` label, otherwise the target's title, otherwise the literal
  `[id]`. Both keep their inline formatting inside the link styling. A
  passthrough block's title serves only as reference text and never appears
  above the block, matching `asciidoctor`.
- A cross-reference inside a target's own reference text renders as `[id]`
  rather than resolving again, so a title that references itself terminates.
  `asciidoctor` falls back at the same point, except where it reuses a target's
  cached converted title, which can carry one already-resolved level.
- An inline span that sets a colour (monospace, highlight, a cross-reference)
  restores the colour of the span around it when it ends, instead of resetting
  the terminal. Bold, italic and background colour survive a nested span, so
  `*bold before <<id>> bold after*` stays bold throughout.
- A section heading and a block caption keep the text of what their title holds.
  A link contributes its link text, and a cross-reference contributes its
  target's reference text rather than its bare id.

- **Typography replacements** — em-dashes (`--`), arrows (`->`, `<-`, `=>`), ellipsis (`...`),
  symbols (`(C)`, `(R)`, `(TM)`), and smart apostrophes now render as Unicode characters.
- **Table colspan/rowspan support** — cells with `colspan` and `rowspan` now render correctly
  using the shared grid utilities. Content appears in the primary cell; spanned positions show
  as empty cells.
- Word wrapping for content inside box-drawn blocks (sidebars, examples, admonitions, quote blocks)
- Unicode-aware character width measurement for correct CJK and emoji wrapping
- `Processor::with_terminal_width()` for deterministic width control in tests and fixture generation.
- Section numbering support (`sectnums`, `partnums`, appendix tracking); special-style
  sections (`[preface]`, `[glossary]`, etc.) and their subsections are left unnumbered.
  Appendix subsections are numbered with the appendix letter as the top component
  (`A.1`, `A.1.1`, `B.1`); with `:!appendix-caption:` the heading shows the bare letter
  numeral (`A.`). Changes to numbering attributes apply only to later headings,
  matching Asciidoctor.
- Index term collection and alphabetized index catalog rendering (`[index]` sections).
- Table column alignment and column style support (strong, emphasis, header).
- Dynamic terminal width detection, capped at 120 columns.
- Super/subscript Unicode conversion with dim-styled fallback for unsupported characters.
- Cross-reference, callout reference, button, keyboard, menu, stem, image, and icon inline macro rendering.
- Box-drawing characters for example, sidebar, and open blocks.
- Comprehensive test fixture covering all major terminal output features.
- Index section test fixture.
- Headless terminal integration tests now verify rendered cell content and
  styling through Ghostty's virtual terminal emulator.

### Fixed

- Description lists now indent nested levels and preserve repeated
  continuations, formatted terms, titled boundaries, named styles, and
  trailing unanswered Q&A items.
- Table alignment after a row or column span now follows source-cell order,
  consistent with HTML and PDF output.
- Book abstracts now take chapter numbers, `sectnums=all` includes special
  sections, and ordinary section numbering continues after an appendix.
- In a book, `:partnums: false` now enables Roman part numbers because the
  attribute is set. Use `:partnums!:` to disable them, matching Asciidoctor.
- Numbered book chapters continue across parts instead of restarting at each
  part.
- Dialogue hard breaks and em dashes now use paragraph boundaries: leading and
  trailing `--` are replaced, while dashes beside or at the edge of inline
  formatting stay literal.
- Brackets in block attribute values no longer make the attribute line leak
  into terminal output.
- **Em-dash inside inline formatting** — `--` inside bold, italic, monospace, highlight,
  superscript, subscript, and curved quotes is no longer converted to an em-dash at string
  boundaries, matching asciidoctor behavior.
- `extract_plain_text` now preserves text content from formatted inline nodes (bold,
  italic, monospace, etc.) in literal paragraphs
- ANSI SGR state tracking now prunes cancelled codes (e.g. bold-off removes bold) instead
  of accumulating indefinitely
- `extract_title_text` now preserves inline content from `VerbatimText`, `RawText`,
  `StandaloneCurvedApostrophe`, `LineBreak`, `CalloutRef`, and all `Macro` variants in
  section titles. Previously these were silently dropped.
- Comprehensive test fixture now marked as OSC8 so it is skipped in CI environments
  without OSC8 support.
- **Inline markup in `link:` text** — bold, italic, monospace, etc. inside `link:url[...]`
  are now honoured in the OSC8 display text.

### Changed

- Terminal tables truncated to the available width now use a single ellipsis
  (`…`) as the truncation marker, leaving more space for cell content.
- `pad_to_width` returns `Cow<str>` to avoid allocation when padding is not needed
- Deduplicated ANSI escape skipping logic into shared `skip_ansi_escape` helper
- **Attribution rendering** — uses `BlockMetadata.attribution`/`citetitle` fields instead of
  string attributes. ([#357])
- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])
- `Error` type is now public (was `pub(crate)`), enabling external code to handle
  terminal converter errors explicitly.
- List rendering no longer inserts extra spaces between inline nodes.
- Enabled `custom_styling` feature on `comfy-table` for ANSI-aware column width calculations, fixing garbled table layouts with styled cell content.

[#313]: https://github.com/nlopes/acdc/pull/313
