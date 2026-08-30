# Changelog

All notable changes to `acdc-converters-manpage` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Major revamp of the manpage converter to match asciidoctor output much more closely.

### Performance

- **Streaming output.** Rendering writes directly to the caller's `Write`
  target, keeping allocations flat on large documents.

### Added

- `%notitle` sections now omit their heading while retaining their body and
  cross-reference text.
- `hide-uri-scheme` now removes the scheme from the visible text of URL,
  `link:`, and automatic links without changing their destination.
- Built-in inline and link roles now use portable roff fallbacks: `underline`
  and `subtitle` use italics, `big` and `small` change the text size,
  `line-through` uses `[deleted: ...]`, and `overline` uses
  `[overlined: ...]`. ID-only spans and unknown or color roles keep their text
  without adding unsupported presentation.
- Unordered `[none]`, `[no-bullet]`, and `[unstyled]` lists now omit bullets;
  ordered lists also support `[unnumbered]`. The `%reversed` option counts down
  from the list length or an explicit `start`, without changing nested lists.
- `[horizontal]`, `[qanda]`, `[ordered]`, and `[unordered]` description lists
  now use distinct readable layouts.
- Bibliography entries now show their reference labels. A valid final `[index]`
  section renders an alphabetized catalog with nested terms and `see` and
  `see-also` relationships.
- Image fallbacks now show explicit or filename-derived alternative text and
  preserve configured links. Audio and video blocks render titled static links,
  including every video source and poster, and report one playback warning per
  document.
- Figure, table, example, and listing titles now use their parser-resolved
  captions, including custom or disabled captions and source-order label
  changes. Styled example, listing, and source paragraphs use the same caption
  behavior as their delimited forms.
- Title-based shorthand cross-references now resolve generated and explicit
  section IDs before selecting their displayed reference text.
- Cross-references preserve formatted explicit text through supported nested
  inline macros. Empty references to captioned blocks honor source-order
  `xrefstyle=basic`, `short`, and `full`, including custom and disabled
  captions, matching Asciidoctor.
- Visible index terms preserve inline formatting, links, and attribute
  substitutions; concealed terms remain hidden, matching Asciidoctor.
- Inline icon macros render their explicit alternative text, or a readable
  form of their target when no alternative is set, matching Asciidoctor.
- Ordered lists honor a positive `start` value, including on nested lists.
  Explicit alphabetic and Roman styles remain an acdc extension; Asciidoctor's
  manpage backend uses decimal markers for those styles.
- Manpage conversion attributes now include the `manpage` backend and
  base-backend, `man` file type, `.man` default output suffix, implied manpage
  doctype, and their conditional convenience attributes.
- An ordered list with an explicit numbering style (`[loweralpha]`, `[upperalpha]`,
  `[lowerroman]`, `[upperroman]`, `[lowergreek]`, `[arabic]`, `[decimal]`) renders
  its `.IP` tags in that style (e.g. `a.`, `IV.`, `α.`) instead of always `1.`, `2.`.
- `[subs="-replacements"]` on a paragraph now keeps typography source (`--`,
  `(C)`, `->`, `...`) literal instead of converting to roff special glyphs.
- User-facing converter warnings are now collected in `ConversionResult` for
  recoverable manpage convention issues, including NAME/SYNOPSIS section order.
- **`[listing]` and `[source]` styled paragraphs** — paragraphs with `[listing]` or
  `[source,lang]` style now render as preformatted text (same as `[literal]`).

- A `<<id>>` with no text reads as the target's reference text: an explicit
  `[[id,label]]` label, otherwise the target's title, otherwise the literal
  `[id]`. Both a label and a title keep their inline formatting (`` `code` ``
  becomes `\f(CR`, bold becomes `\fB`). A reference to a level-1 section
  upper-cases its heading the way the `.SH` line does, link and mailto text
  included; a label keeps the case it was written in. A passthrough block's
  title serves only as reference text and never appears above the block,
  matching `asciidoctor`.
- A cross-reference inside a target's own reference text renders as `[id]`
  rather than resolving again, so a title that references itself terminates.
  `asciidoctor` falls back at the same point, except where it reuses a target's
  cached converted title, which can carry one already-resolved level.
- A `.SH`/`.SS` heading, the `.TH` title comment, and a block caption keep the
  text of what their title holds. A link contributes its link text and a
  cross-reference its target's reference text, matching `asciidoctor`. Unlike
  `asciidoctor`, acdc writes the link's text rather than a `.URL` macro inside
  the quoted `.SH` argument, where a nested macro produces malformed roff.
- A callout marker in a listing or literal block is kept in the rendered
  content, reading as `<N>`, its source form. `asciidoctor` renders a bold `(N)`.
- Upper-casing a level-1 section title leaves a numeric character reference
  alone, so `&#x2019;` keeps its lowercase `x`, matching `asciidoctor`.

- **Typography replacements** — em-dashes (`--`), arrows (`->`, `<-`, `=>`), ellipsis (`...`),
  symbols (`(C)`, `(R)`, `(TM)`), and smart apostrophes now render as proper roff escapes
  via the shared `apply_replacements()` pipeline feeding into `manify()`.
- **Thin-space and zero-width-space escaping** — `\u{2009}` maps to `\|` and `\u{200B}`
  maps to `\&` in roff output, supporting em-dash typography replacements.
- **Table colspan/rowspan support** — cells with `colspan` and `rowspan` now render correctly
  using per-row tbl format lines with `s` (horizontal span) and `^` (vertical span) markers.
- **Test fixtures** for video/audio blocks, index terms, inline/block images, icon macros,
  STEM blocks, volume number variations, custom `man-linkstyle`, and embedded mode with media.
- Quote and verse block attribution rendering (`[quote, author, citation]`)
- Footnotes section (rendered as NOTES, matching asciidoctor)
- Author(s) section auto-generated from document header
- Checklist marker rendering for unordered list items
- Description list principal text rendering (inline content after `::`)
- List continuation blocks with RS/RE scoping for all list types
- Proper `.URL` and `.MTO` macro usage for links and autolinks (replacing inline angle-bracket format)
- Section level 3+ rendering as bold paragraph headings
- Table column alignment support and inline formatting in table cells
- Source file modification date fallback for `revdate`
- Arrow character escapes (right/left arrows, double arrows)
- Support for `man source`, `man manual`, `man-source`, `man-manual` attribute aliases

### Fixed

- Sections with a named `reftext` now use it for natural-reference and explicit-ID
  display text. Their titles are not retained as second natural aliases, and
  formatted labels keep their roff formatting, matching Asciidoctor.
- Plain visible shorthand cross-references now match section titles containing
  `pass:[...]` or `+...+` content. A shorthand target containing a passthrough
  remains unresolved and displays its visible text, matching the Asciidoctor
  manpage backend.
- When `:compat-mode:` is active at a title-based shorthand cross-reference,
  it displays its bracketed unresolved fallback. Source-order changes apply
  only to later references, and explicit local IDs still display the section
  heading, matching the Asciidoctor manpage backend.
- Interdocument `xref:` macros no longer display a same-named local section's
  heading. They keep the manpage converter's bracketed external-reference
  fallback, while natural shorthand references remain local.
- Description lists now preserve nested ownership, repeated continuations,
  formatted terms, titled boundaries, named styles, and trailing unanswered
  Q&A items, matching the Asciidoctor manpage backend.
- Table cells now apply strong, emphasis, literal, and monospace styles,
  matching the Asciidoctor manpage backend. acdc's span and alignment extension
  now applies column alignment by source-cell order after spans.
- A custom title on the first manpage name section now gets the same special
  spacing, embedded-output handling, and section-order validation as `NAME`,
  matching `asciidoctor`.
- Dialogue hard breaks and em dashes now match the `asciidoctor` manpage
  backend: paragraph-leading and trailing `--` are replaced, while dashes
  beside or at the edge of inline formatting stay literal. Spaced em dashes
  use normal roff word spacing instead of thin-space escapes.
- Quote and verse attribution stays literal unless single-quoted, when
  formatting and links render. A citation without an attribution is hidden,
  matching the `asciidoctor` manpage backend.
- **Em-dash inside inline formatting** — `--` inside bold, italic, monospace, highlight,
  superscript, subscript, and curved quotes is no longer converted to an em-dash at string
  boundaries, matching asciidoctor behavior.
- Non-paragraph content in table cells (lists, code blocks, admonitions) is no longer
  silently dropped
- Explicit `mailto:` macros now capture trailing punctuation in the `.MTO` macro's third
  argument (matching autolink behaviour)
- **Inline markup in `link:` text** — the `.URL` macro's display-text argument now
  reflects parsed inline markup inside the link's bracket expression.

### Changed

- Refactored grid-building logic to use shared utilities from `acdc-converters-core`.
- Skip NOTES and AUTHOR(S) sections in embedded mode to match asciidoctor behaviour
- **Attribution rendering** — uses `BlockMetadata.attribution`/`citetitle` fields instead of
  string attributes. ([#357])
- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])
- Replaced `.PP` with `.sp` throughout for consistent paragraph spacing
- Monospace text now uses Courier font (`\f(CR`) instead of bold
- Unordered list continuation blocks use `.RS 2` to align text with item content
- All list types now wrapped in RS/RE for proper indent scoping
- Ellipsis rendering uses thin-space separated dots (`.\\|.\\|.`)
- Comment header format corrected (`.\\"` instead of `.\"`)
- Menu macro renders target with arrow separators
- Document title strips trailing volume number in header comment
- Multi-author support in header comment and AUTHORS section
- Subsection headings (level 2) preserve original case instead of uppercasing

[#313]: https://github.com/nlopes/acdc/pull/313
