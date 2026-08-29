# Changelog

All notable changes to `acdc-parser` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `CrossReference` equality and debug output now depend only on its exposed
  model state. Equivalent values from standalone and full-document parsing no
  longer differ because of private parsing state, and debug output does not
  expose that state.
- A section's named `reftext` is now its natural cross-reference alias and
  catalogued reference label. The section title is not retained as a second
  alias; explicit IDs still resolve, and formatted labels retain their inline
  content, matching Asciidoctor.
- Section titles with `pass:[...]` or `+...+` content now participate in
  title-based cross-reference lookup as their visible text. A shorthand target
  that contains a passthrough remains unresolved, but `CrossReference::target`
  and warnings retain its visible text, matching Asciidoctor.
- When `:compat-mode:` is active at a title-based shorthand cross-reference,
  it retains its natural target and produces an unresolved-reference warning
  instead of resolving to a section ID. Source-order changes apply only to
  later references, and explicit local IDs still resolve. Asciidoctor also
  retains the natural target but does not warn.
- Interdocument `xref:` macros keep their file and fragment targets when a
  local section has the same title. Natural `<<Title>>` references still
  resolve to local section IDs, and `CrossReference::target` distinguishes the
  two forms, matching Asciidoctor.
- Title-based shorthand cross-references such as `<<Syntax Highlighting>>` now
  resolve to generated or explicit section IDs. Exact IDs take precedence,
  custom link text remains intact, and missing titles stay unresolved with a
  parser warning, matching Asciidoctor. `CrossReference::target` contains the
  resolved ID when a title-based reference matches.
- Explicit links, direct URL macros, and `mailto:` macros with a named `id`
  attribute now act as untitled cross-reference targets, including when the
  attribute list starts with a comma. Automatic references use `[id]`, and
  `Document::references` exposes the target for navigation. Asciidoctor
  renders these destinations but does not add them to its public reference
  catalog; acdc catalogs them so converters can resolve the references safely.
- Cross-reference labels that contain inline passthroughs no longer expose
  internal placeholder text in automatic citations. The passthrough source is
  preserved as literal reference text, matching Asciidoctor citations.
- Inline passthroughs nested inside formatted text now retain their substitution
  policy instead of becoming plain text.
- Nested sections inside bibliography sections now produce a non-fatal parser
  warning and remain in the document. Asciidoctor reports the same recoverable
  condition at error severity; acdc's recoverable parser diagnostics currently
  expose only warning severity.
- Nested Setext sections now remain under their parent section instead of being
  treated as sibling sections.
- Attributes supplied through parser options now take precedence over matching
  document entries. A document cannot replace a caller value or reverse a
  caller-requested unset. A caller-set `sectnums` remains flexible after the
  header, matching Asciidoctor; a caller-requested unset remains locked.
- Document entries can no longer set or unset the read-only and API-only names
  in the AsciiDoc document attribute reference, including backend convenience,
  safe-mode, input-path, and include-security attributes. This follows the
  documented contract and is intentionally stricter than current Asciidoctor
  for derived names that its Ruby implementation does not lock consistently.

### Added

- Index terms now parse Asciidoctor's named `see` and `see-also` attributes
  and the spaced `>>` and `&>` shorthand forms. `IndexTerm::relationship`
  exposes this data as `IndexTermRelationship`; serialized ASG output includes
  a `relationship` extension only when a relationship is present.
- Cross-reference macros preserve formatted explicit text through supported
  nested inline macros and escaped closing brackets. Empty cross-references
  also expose the `xrefstyle` and caption-label selection in effect at their
  source position; captioned targets expose the resolved label and number
  needed for `short` and `full` automatic reference text.
- Parsed tables expose their resolved `frame`, `grid`, and `stripes`
  presentation. Table-level values override `table-frame`, `table-grid`, and
  `table-stripes`, and document-attribute changes apply in source order. The
  resolved presentation, including interactive `stripes=hover`, participates
  in table equality and debug output.
- Parsed sections and their table-of-contents entries now carry the same number,
  based on the numbering attributes in effect at each heading. Numbering changes
  apply in source order and restart inside AsciiDoc table cells. `Section::new`
  creates an unnumbered section; use `Section::with_numbering(true)` and
  `Document::renumber_sections()` when adding or changing sections through the API.

- Titled example, listing, source, image, and table blocks now carry their caption —
  label and number — in `BlockMetadata::caption`, taken from the `caption`,
  `<kind>-caption`, or block-level `caption=` attribute in effect **at that block's
  source position**. Changing a caption attribute part-way through a document applies
  only from that point on, and a block nested inside another is numbered first, both
  matching Asciidoctor.
- Caption kind follows a block's effective context: `[listing]` on `====` is still an
  example, `[literal]` on `----` takes no caption, and `[example]`/`[listing]`/`[source]`
  promote an open or literal block. An untitled block takes no number, a custom or blank
  caption consumes none, and `:example-caption!:` disables both. A `%collapsible` example
  also takes no caption label or number.
- `Document::renumber_captions()` reassigns every caption number after you change the
  AST — reorder blocks, remove one, add a title — and `Document::highest_caption_number()`
  reports the highest number assigned for a kind, so a consumer numbering a title the
  parser never saw can start past it.
- `Options::builder().with_base_dir(path)` now controls entry include resolution
  for string, reader, and file input. It also defines the Safe/Server local
  boundary; otherwise string/reader input uses the current directory and file
  input uses the entry file's parent.
- Every AST node's `Location` now maps back to the **original source**: its
  `start`/`end` line numbers and `absolute_start`/`absolute_end` byte offsets are
  original-source coordinates, and each boundary (`Position`) carries a new `file`
  field naming the `include::` chain the content came through. This holds across
  `include::` directives and preprocessor edits (dropped adjacent comments, stripped
  `ifdef`/`ifndef`/`ifeval` blocks, collapsed multi-line attribute continuations) —
  content after such an edit no longer reports a shifted line. `file` is set only for
  content from an `include::`d file; content from the primary input carries `None`,
  matching the ASG convention of omitting the primary file. Because the file lives on
  each boundary, a span that starts in an included file and ends after it reports each
  endpoint's own chain. Partial includes (`include::file.adoc[lines=…]` or `[tag=…]`)
  map to the selected lines' true positions in the included file — a `lines=3..4`
  include reports line 3, not line 1 — and a non-contiguous selection locates each
  run independently. Columns are origin-relative too: an `include::file.adoc[indent=N]`
  re-indent reports each node's column in the included file (the inserted indent is
  stripped back off). For such re-indented content the line and column are exact while
  the `absolute_start`/`absolute_end` byte offsets stay in preprocessed coordinates
  (these offsets are not part of the ASG output, which carries line/column only). The
  same original-source mapping also applies to the `Document.references`,
  `Document.toc_entries`, and `Document.footnotes` locations (not serialized to the
  ASG, but consumed e.g. by LSP go-to-definition), so a cross-reference, TOC entry, or
  footnote pointing at a target in an `include::`d file resolves to that file at its
  true line.
- `Position` now serializes in the ASG `locationBoundary` format: `{ line, col }`
  plus an optional `file` — the `include::` chain as an array of the include targets
  *as written*, outermost first and the file directly containing the content last
  (e.g. `["outer.adoc", "inner.adoc"]`) — emitted only for `include::`d content, so
  the JSON shape is unchanged for single-file documents.
- `Location::byte_len()` returns the location's inclusive byte length, or `None` when its
  start and end fall in different files (where the byte offsets are in different coordinate
  spaces and can't be subtracted). Prefer it over `absolute_end - absolute_start`.
- `Position::from_line_col(line, column)` builds a `Position` from `usize` line/column,
  saturating at `u32::MAX`. Use it when constructing from `usize` indices; prefer
  `Position::new` when the values are already `u32`.
- `SectionKind` enum and a `kind` field on `Section` (and `TocEntry`) classifying
  a section as an `AsciiDoc` *special section* (`Preface`, `Glossary`, `Appendix`,
  …) or `Normal`, derived from its style. This is a structural classification only
  — converters use it, e.g. to exclude special sections and their subsections from
  `:sectnums:` numbering, matching asciidoctor. `#[non_exhaustive]`, so more kinds
  can be added later.
- A section title that skips a level (e.g. `====` under `==`) is rendered at its
  literal level with a `WarningKind::SectionLevelOutOfSequence` instead of being a
  fatal error. Nestable level-0 book special sections (`[preface]`, `[abstract]`,
  and `[appendix]`) accept a level-2 (`===`) first subsection without an
  out-of-sequence warning, matching Asciidoctor.
- The `discrete`/`float` block style marks a discrete heading and renders as its
  class; the legacy `float` spelling raises `WarningKind::LegacyFloatDiscreteHeading`.
- `[#id,…]` sets the id and treats the rest as block attributes (only
  `[[id,reftext]]` sets a reference text).
- Constrained formatting (`*`, `_`, `` ` ``, `#`) is now recognized after a
  non-ASCII punctuation boundary such as a curly quote or guillemet (e.g.
  `“*bold*”`); Unicode letters/digits still aren't boundaries.
- `Document.references`: an `id → Reference` catalog covering every cross-reference
  target, including sections, blocks, inline `[[id]]` anchors, and IDs on formatted
  inline spans, with each target's reference text and source `location`, so `<<id>>`
  can be resolved and navigated to. A target's reference text is its explicit
  `[[id,label]]` label when it has one, otherwise its title; a titled single-line
  admonition keeps its title. Both arrive as inline nodes: a label takes the
  reference-text substitutions (specialchars, quotes, replacements), so
  `[[id,*Bold* label]]` renders bold while a macro in a label stays literal,
  matching `asciidoctor`. When two elements claim the same id, the first one to
  claim it keeps the reference text, also matching `asciidoctor`.
- Bibliography sections classify each direct unordered list without an explicit
  style as `bibliography`. A valid leading `[[[id]]]` or
  `[[[id,reference text]]]` in those lists is exposed through
  `Anchor::is_bibliography()` and `Reference::is_bibliography()`; misplaced
  syntax remains an ordinary anchor with visible brackets, and invalid IDs
  remain literal text. `Reference::has_automatic_citation()` reports whether a
  bibliography target is cited without explicit link text. This matches
  Asciidoctor.
- An `<<id>>`/`xref:id[]` whose target is defined nowhere now reports a
  `WarningKind::UnresolvedReference`, matching `asciidoctor` (external/inter-document
  references aren't flagged as the parser only deals with one file at a time).
- `[subs="-post_replacements"]` now suppresses trailing-`+` hard line breaks.
- `[subs="-quotes"]` now leaves `*bold*`, `_italic_`, `` `mono` ``,
  `#highlight#`, `^super^`, `~sub~`, and curved quotes/apostrophes as literal
  text.
- `[subs="-callouts"]` on listing/literal blocks now leaves `<1>` and `<.>`
  markers as literal text.

  All three require the default-on `pre-spec-subs` feature.
- Comments record their syntactic form and keep their text for tooling: the new
  `CommentKind` on `Comment` separates `//` lines from `[comment]` paragraphs,
  and a `[comment]` `--` block is a `DelimitedComment` (distinguished from a
  `////` block by its `--` delimiter).
- Table recovery warnings for unsupported formats, incomplete final rows, row
  column-count mismatches, and cells that exceed the configured column count are
  now distinguishable warning categories with targeted advice, so callers can
  present separate diagnostics without matching warning text.

### Changed

- **Breaking:** index terms now expose substituted `InlineNode` collections
  instead of source strings through `IndexTermKind` and its accessors.
  `InlineMacro::IndexTerm` now contains a boxed `IndexTerm`, and serialized
  `term`, `secondary`, and `tertiary` values are inline-node arrays.
- Substitutions now follow their written order when attribute values introduce
  formatting or macro syntax. The public `NORMAL` order and `[subs=normal]`
  now put quotes before attributes, matching Asciidoctor.
- A trailing `^` in URL, link, and mailto display text now sets
  `window=_blank` and is omitted from the text, matching Asciidoctor.
- Parsed URL, link, and autolink nodes report whether fallback display text
  should omit the URI scheme, based on `hide-uri-scheme` at their source
  position. Serialized ASG output is unchanged.
- Keyboard, button, and menu macros are now recognized only while the
  `experimental` document attribute is set. Setting or unsetting it in the
  document body applies to the content that follows, matching Asciidoctor.
- Comparing or debugging `Anchor` and `Reference` values now includes their
  bibliography state. Serialized ASG output is unchanged.
- `toclevels` and `sectnumlevels` now remain undefined until the document sets
  them, so attribute references stay literal by default. TOC and section-numbering
  depth still default to 2 and 3, matching Asciidoctor.
- Comparing or debugging sections now includes their numbering state. Serialized
  ASG output is unchanged.

- `BlockMetadata` now carries its block's resolved caption, so parsed metadata no longer
  compares equal to otherwise-identical metadata built by a caller. Serialized output is
  unchanged — the caption is not part of the ASG.
- A document attribute assigned the literal value `false` now remains set to
  that text. Use `:name!:` or `:!name:` to unset it, matching Asciidoctor.
- Include targets beginning with a case-insensitive ASCII URI scheme are now
  classified before local paths. HTTP(S) remains the supported transport; other
  schemes use URI fallback or recovery and cannot resolve as local files. acdc keeps
  Asciidoctor's two-character Windows-path disambiguation but intentionally treats
  MRI Asciidoctor's non-ASCII scheme-like prefixes as paths.
- Include decoding now ignores unknown encoding labels before BOM/UTF-8 fallback and
  recovers malformed explicitly transcoded local files as unreadable includes.
  acdc intentionally retains `encoding_rs` labels and decode-before-selection
  behavior rather than Ruby's exact encoding aliases and line-reader quirks.
- **Breaking:** `TocEntry`'s `numbered` and `style` fields are replaced by a
  single `kind: SectionKind`. Appendix detection becomes `entry.kind ==
  SectionKind::Appendix`; the special-section classification it previously needed
  `style` for is now carried by `kind`. Serialized output is unchanged — the
  `style` key is still emitted for special sections (derived via
  `SectionKind::as_style`).
- Updated the parser grammar implementation to reduce location-tracking overhead while
  preserving the same parse output and diagnostics.
- **Breaking:** `Position::line` and `Position::column` are now `u32` instead of `usize`
  (saturating at `u32::MAX` for inputs beyond ~4 billion lines/columns), keeping the
  per-node `Location` compact now that each boundary also carries its originating `file`.
- **Breaking:** the `Positioning` enum is removed and `SourceLocation` now holds a
  single `location: Location` (a point diagnostic is a zero-width span with
  `start == end`). Read `source_location.location.start` for the line/column instead
  of matching `Positioning`. Construct diagnostics with `SourceLocation::at_position`
  / `SourceLocation::at_location`, or `Location::point` for a bare position. Rendered
  error/warning text is unchanged.

### Removed

- **Breaking:** `inlines_to_string()` is no longer exported by `acdc-parser`. Check
  `acdc-converters-core::inlines_to_string()` for converter/tooling plain-text extraction
  from inline nodes (even though we currently don't publish `acdc-converters-core`, you
  can see an example).
- **Breaking:** the unused `Location::shift`, `Location::shift_inline`, and
  `Location::shift_line_column` methods.

### Fixed

- Block image, audio, and video macros and inline image macros accept local and
  remote targets with internal spaces, as well as already percent-encoded local
  targets. Leading and trailing target whitespace remains invalid, matching
  Asciidoctor.
- Index-term inline nodes preserve their source locations inside passthroughs
  and formatted spans.
- Inline passthroughs apply `c`, `q`, `a`, `r`, `m`, and `p` substitutions in
  the written order. The `n` and `v` groups use Asciidoctor's inline
  passthrough policies. Escaped passthroughs stay literal, ordered substitutions
  can create hard line breaks across attribute references, expanded values keep
  valid source locations, and `parse_inline` accepts pass macros without a
  substitution-name list, including an empty pass body.
- Description lists now preserve repeated continuations, delimiter-based
  nesting, formatted terms, trailing unanswered Q&A items, titled-list
  boundaries, and named `style=` values, matching Asciidoctor.
- Image macros now accept an empty inline `link=` value and use the last value
  when `link=` appears more than once, matching Asciidoctor.
- Checklist markers now apply only to unordered list items. `[ ]`, `[x]`,
  `[X]`, and `[*]` remain visible text in ordered items, matching Asciidoctor.
  acdc continues to accept `[X]` in unordered checklists as an intentional
  extension.
- Unindented ordered and unordered markers now nest automatically when the
  marker type changes, while metadata after a blank line still starts a new
  list, matching Asciidoctor.
- Block attributes and anchors directly before an automatically nested list now
  apply to that child list instead of appearing in the parent item text,
  matching Asciidoctor.
- A block image macro preceded by `[listing]`, `[source]`, `[literal]`, or
  `[verse]` is parsed as that styled paragraph and kept as text instead of
  becoming an image, matching Asciidoctor.
- Path-based macros now recognize non-ASCII local targets, including cross-references,
  links, icons, images, audio, and video, matching Asciidoctor.
- Table cell specifiers now accept the Asciidoctor order with spans or repeats
  before alignment and style. Column styles after spans, and across repeated
  cells, now follow source-cell order.
- AsciiDoc-style table cells now keep local attributes and sections within the
  nested cell document. Local section-numbering changes apply in source order,
  and outer cross-references can still target cell headings.
- Literal table cells now keep attribute references, macros, formatting marks,
  spaces, and line breaks as source text. Semantic header rows ignore cell and
  column styles and continue to use normal substitutions, matching Asciidoctor.
- Source blocks record `source-linenums-option` and `prewrap` changes at their
  source position, so converters apply line numbering and HTML wrapping only
  to the blocks that follow each change.
- Verbatim blocks preserve trailing blank lines when callouts are enabled, and
  explicitly styled source, listing, and literal paragraphs preserve their
  leading indentation.
- Callout parsing now recognizes every marker at the end of one line and XML
  comment markers, while escaped markers remain literal and do not consume an
  automatic number, matching Asciidoctor. Text and callout nodes now report
  their own source spans instead of the location of the whole verbatim block.
- Manpage name attributes come only from the required first level-1 section and
  are available to substitutions throughout the body. A preamble, a wrong-level
  section, or a non-conforming first section now uses the `docname` or `command`
  fallback instead of deriving attributes from a later `NAME` section, matching
  `asciidoctor`.
- Dialogue lines that start with `--` now follow the effective replacement
  order. The documented trailing-`+` conflict stays on one line, while the
  `{empty}` hardbreaks workaround produces one line break. Disabling post
  replacements now makes `%hardbreaks` inert instead of causing a parse error.
- Paragraph `hardbreaks` options and the document attributes `hardbreaks` and
  `hardbreaks-option` now preserve source newlines. Empty ` +` lines and
  `{empty} +` insert blank or leading lines. Attribute changes affect only
  following content, and disabling post replacements disables hard breaks.

- Document subtitles now split only at a colon followed by a space, so URL
  macros in a subtitle remain intact.
- Brackets in block attribute values no longer break the metadata line. Quoted
  commas, empty and named slots, attribute references, and stacked lists now
  follow `asciidoctor`.
- Source language and line-number slots, stem notation, image alt text and
  dimensions, and video posters and dimensions now use their correct positions.
- Quote and verse attribution stays literal unless single-quoted, when inline
  formatting and macros are processed.
- With `compat-mode`, denied URI includes now omit `role=include` from their
  converter-visible fallback links, matching `asciidoctor`.
- Partial includes now select `lines`, `tag`, or `tags` from the original target
  before processing nested directives. Excluded includes and conditionals no
  longer run or emit warnings. `lines` takes precedence over `tag`, which takes
  precedence over `tags`; duplicate named selectors use their last value; selected
  line numbers are sorted and deduplicated; and a negative range end selects
  through the end of the file.
- Tag selection now handles nested and repeated tag names and emits located
  warnings for missing tags and unexpected, mismatched, or unclosed boundaries.
  Warnings point to the including directive, name the included file and marker
  line where relevant, and parsing continues.
- Mixed `tags=!**;…` include filters now decide whether to select otherwise
  unrequested tagged regions from the first remaining selector in attribute
  order, matching `asciidoctor`; changing tag names no longer changes the result.
- Include targets now accept internal spaces while leading or trailing target
  whitespace remains invalid. Local targets with spaces are read normally, and
  Secure or unauthorized URI includes preserve the complete target in a
  converter-visible link fallback. Authorized HTTP(S) targets continue to use
  `ureq`: a raw space causes the existing located `include uri not readable`
  recovery.
- Builds without the `network` feature now emit a located warning, preserve an
  authorized HTTP(S) include as literal unresolved text, and continue parsing
  instead of removing the directive. This is an acdc-specific capability
  fallback because Asciidoctor has no equivalent compile-time network setting;
  URI includes that lack caller authority continue to use the no-warning link
  fallback.
- Authorized HTTP(S) includes that encounter a connection failure, HTTP error
  status, or response-body I/O error now emit a located
  `include uri not readable` warning. We preserve the original directive as
  unresolved text and continue parsing instead of returning a top-level transport
  error. We don't process partial response bodies. Character-encoding errors and
  the remote-include size limit remain fatal parser policies.
- HTTP(S) includes without caller-supplied `allow-uri-read` now render as
  `link:<target>[role=include]`, emit no warning, and continue parsing in Unsafe,
  Safe, and Server modes instead of disappearing. The authority check happens
  before transport selection, so the fallback is identical with and without the
  `network` feature and shares Secure mode's source mapping.
- Missing non-optional local includes now emit a located warning, insert
  `Unresolved directive in <current source> - include::<target>[<attributes>]`,
  and continue parsing instead of silently removing the directive. The fallback
  also applies when an existing include cannot be read, retains the include
  attributes for converter-visible output, and is attributed to the correct
  source in nested includes. Missing `opts=optional` includes remain silent and
  are removed.
- Malformed `include::` lines with an empty target or leading/trailing target
  whitespace remain literal content instead of being removed or failing the parse.
  A leading backslash is removed only when the complete line is a valid include
  directive, matching asciidoctor.
- Tables now accept at most 100 columns and 1,000 rows (100,000 cells).
  Larger `cols` multipliers, cell duplication counts, column spans, row spans,
  or delimited table dimensions return a located parse error. These are fixed
  internal safety limits: they cannot be changed through `Options` or document
  attributes. They intentionally differ from asciidoctor, which has no equivalent
  cap.
- Block macros whose name only starts with a conditional directive name
  (`ifdefs::x[]`, `ifdef-foo::bar[]`) are ordinary content instead of failing the
  parse with `Invalid conditional directive`.
- Includes are bounded by a `max-include-depth` attribute, set through `Options` and
  not by document content. It defaults to `64`. `0` or a negative value leaves
  `include::` directives unprocessed silently; past a positive limit the parser warns
  `maximum include depth of N exceeded`, keeps the directive, and continues, so
  recursive and cyclic includes terminate. Unlike `asciidoctor`, boolean `true`
  disables includes instead of raising, and leading Unicode whitespace in the value
  counts as whitespace. Only exact `include::` directives are depth-limited; similarly
  prefixed block macros remain literal without include diagnostics. Ignored declarations
  in document content are not exposed as `Block::DocumentAttribute` nodes. See the
  README for the full value handling.
- `include::file[indent=N]` now accepts at most `4096` spaces and returns a located
  error before reading the target when the value is larger. This fixed safety limit
  prevents one directive from requesting an unbounded indentation prefix and
  intentionally diverges from `asciidoctor`, which has no equivalent limit. The cap
  is an acdc security policy, not a requirement of the AsciiDoc language.
- Local includes in `Safe` and `Server` mode now stay beneath the entry document's
  directory. For an entry document at `/workspace/docs/main.adoc`,
  `include::../shared.adoc[]` reads `/workspace/docs/shared.adoc`, and
  `include::/tmp/shared.adoc[]` reads
  `/workspace/docs/tmp/shared.adoc`; both transformations emit a warning. Nested
  includes retain the `/workspace/docs` boundary, while `Unsafe` behavior is
  unchanged. The check does not resolve symlinks, so
  `/workspace/docs/linked.adoc` may still point to and read a file outside that
  directory. These path transformations match `asciidoctor`.
- Remote HTTP(S) include responses are now limited to 10 MiB after transport
  decoding. Larger responses fail with an HTTP request error. This fixed safety
  limit intentionally diverges from `asciidoctor`, which has no equivalent
  per-response limit.
- With the `network` feature and caller-supplied `allow-uri-read`, nested includes in
  HTTP(S) content now remain URI-based instead of being interpreted as local files.
  Relative, root-relative, ancestor, and filesystem-looking targets use
  `asciidoctor`'s literal containing-directory construction, preventing remote
  content from pivoting into local file reads. Remote content is also no longer
  stored under predictable shared temporary filenames.
- URI includes now require caller-supplied `allow-uri-read`; a document header can no
  longer grant itself network-read authority or revoke a hard caller grant. Caller
  string values are presence-based, so empty and textual `false` values still enable
  URI reads, while boolean `false` and no-value caller entries do not. This matches
  `asciidoctor`. Denied-URI fallback output and diagnostics retain their existing
  behavior pending the separate compatibility work.
- `Secure` mode no longer reads local or URI `include::` targets. It preserves each
  ordinary target as a `link:...[role=include]`, matching `asciidoctor`.
- Link and URL macros containing only named attributes, such as
  `link:https://example.com[role=include]` or
  `https://example.com[role=include]`, now treat them as attributes instead of visible
  link text. With no explicit text, converters display the target. Attribute-like
  `mailto:` content remains display text, matching `asciidoctor`.
- When parsing with the `manpage` backend, `outfilesuffix` now changes from its
  initial `.man` value to the document's man volume suffix (for example `.1` or
  `.3p`) once the manpage title is known, matching `asciidoctor`.
- `ifdef` and `ifndef` conditions now accept any number of attribute names joined
  by `,` or `+`. A named `endif` can repeat the complete condition
  case-insensitively, matching `asciidoctor`. Conditional blocks also remain in
  effect across blank lines and nest correctly, so skipped content and closing
  directives no longer leak into the parsed document.
- Warnings now report the correct original file and line for content that follows a
  preprocessor edit. A dropped adjacent line comment, a stripped
  `ifdef`/`ifndef`/`ifeval` block, or a collapsed multi-line attribute continuation
  no longer shifts the reported line of everything after it, and a warning inside an
  `include::`d file is anchored to that file at its true line.
- A list continuation marker (`+`) on its own line with nothing to attach (the next
  line is blank or the document ends) is now dropped instead of being rendered as a
  literal `+` paragraph, matching `asciidoctor`. This commonly appears as a trailing
  `+` after a block attached to a list item; the list now continues uninterrupted to
  the following items rather than being split in two.
- A delimited block (example `====`, listing `----`, literal `....`, sidebar
  `****`, quote `____`, open `--`, comment `////`, passthrough `++++`, and the
  Markdown ```` ``` ```` fence) whose opening delimiter runs to end of input with no
  closing delimiter is now closed at end of input and still rendered, with a
  `WarningKind::UnterminatedDelimitedBlock` warning — matching `asciidoctor`.
  Previously this either aborted the parse with a hard error (e.g. an
  unterminated `====`) or leaked the opening delimiter into a literal paragraph.
- A document whose only content is a title (`= Title` with no body, no author
  line, and no following blank line) is now recognized as the document title
  rather than rendered as a level-0 section, matching `asciidoctor`. Applies to
  both ATX (`= Title`) and Setext (`Title` / `====`) titles.
- A trailing ` +` hard line break on the last line of a block (at end-of-input or
  immediately before a blank line) now renders as a line break instead of a literal
  `+`, matching `asciidoctor`. A nested span ending in ` +` (e.g. `` `code +` ``, a
  footnote, or a link label) still stays literal.
- An auto-generated section id from a title that starts with non-alphanumeric
  characters (e.g. `=== -- Specialized Environments`) no longer gains a doubled
  leading underscore (`__specialized_environments`); leading separators are now
  squeezed away so the id is `_specialized_environments`, matching `asciidoctor`,
  keeping xref and TOC anchors to those sections working. A title with no id-able
  characters at all (e.g. `== ---`) now yields an empty id rather than a bare
  `_`, also matching `asciidoctor`.
- A `//` line comment that sits directly against preceding block content (no blank
  line in between) is now dropped instead of being rendered as literal text, matching
  `asciidoctor`. This covers comments after a paragraph, list item, or description-list
  entry. Standalone comments (preceded by a blank line, a title, a `+` continuation
  marker, or another comment), comments inside verbatim blocks, and `tag::`/`end::`
  include directives are left untouched.
- A `//` line comment or `////` block comment inside a list or description-list
  continuation (after a `+` marker) is now treated as a comment instead of being
  rendered (line comments were emitted as literal text; a block comment preceded
  by a blank line was dropped but leaked a stray `+` paragraph), matching
  `asciidoctor`. A trailing `+` followed by a comment terminates the continuation
  cleanly.
- A block with the `[comment]` style (an open `--` block or a paragraph) is now dropped
  and produces no output, matching `asciidoctor`; previously its content was rendered.
- Document header author lines that aren't a plain `firstname [middlename] [lastname]
  [<email>]` (e.g. with an `Author:` prefix, a `(role)`, or comma-separated names) are now
  read as a single author instead of spilling the author line and the header's attribute
  entries into the document body. Comment lines between the title and the author line are
  skipped, and multiple authors are separated by `;`, matching `asciidoctor`. Such a
  non-standard author line also raises a `WarningKind::NonStandardAuthorLine` warning.
- A `//` line comment between the author line and the revision line is now
  skipped, so the revision (and any following attribute entries) is still read,
  matching `asciidoctor`.
- `{revnumber}` no longer keeps the leading `v` from a `vX.Y` revision line
  (`v2.0` now resolves to `2.0`), matching `asciidoctor`.
- `{authorcount}` resolves to `0` for a document with no author (rather than
  staying an unresolved reference), matching `asciidoctor`.
- In pipe (`|`) tables (the default
  [PSV](https://docs.asciidoctor.org/asciidoc/latest/tables/data-format/#default-table-syntax)
  format), a cell's content can span multiple lines, and a row can be written across
  several lines (each cell starting on its own line), both matching `asciidoctor`. When
  the last row has fewer cells than the table has columns, the leftover cells are dropped
  with a warning (`dropping cells from incomplete row detected end of table`). A cell
  specifier (`2+`, `.3+`, `^`, `a`, ...) is recognized when it sits immediately before the
  delimiter and is separated from the cell content by whitespace (e.g. `| name 2+| spans`
  gives the next cell a colspan of 2), so colspan/rowspan cells count toward the row width
  and are no longer mistaken for incomplete rows; a specifier flush against the opening
  delimiter (`|2+|`) stays literal, matching `asciidoctor`.
- [TSV](https://docs.asciidoctor.org/asciidoc/latest/tables/data-format/#csv-and-tsv)
  tables (`format=tsv`) now honor quoted field values that span multiple lines, matching
  `asciidoctor` (the same quoting rules already applied to CSV).

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

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-parser-v0.9.0...HEAD
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
