# Changelog

All notable changes to `acdc-converters-markdown` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Inline UI macros, passthroughs, STEM expressions, and roles now retain
  readable content. GFM uses native strikethrough for `line-through`; other
  roles use portable HTML fallbacks when Markdown has no equivalent.
- Link and autolink fallback text now honors `hide-uri-scheme`, bracketed email
  autolinks remain visible, and CommonMark footnotes keep their complete bodies
  in a linked endnote list.
- Inline code, source escapes, link text, and link destinations now remain
  valid when their content contains Markdown delimiters, backticks, brackets,
  whitespace, or parentheses.
- Quote, verse, literal, listing, source, example, and abstract paragraph
  styles now retain distinct Markdown treatments. Verse, literal, listing,
  and source whitespace remains visible.
- Passthrough blocks now preserve their raw content for Markdown renderers that
  accept embedded HTML.
- Source callout markers now remain visible beside the code and their complete
  explanation lists, including attached blocks, render below it.
- Source line numbering, selected-line highlighting, and PHP `%mixed`
  highlighting now preserve the code and emit one structured fallback warning
  per option per document.
- Document headers now preserve explicit IDs, subtitles, authors and email
  addresses, and revision numbers, dates, and remarks as readable Markdown.
- Paragraph, list, delimited-block, admonition, image, audio, video, and table
  titles now remain visible as strong title lines. Figure, table, example, and
  listing titles retain their numbered, custom, or disabled caption prefixes.
- Quote and verse paragraphs and blocks now preserve their attribution and
  citation titles, including supported inline formatting.
- Sections now render parser-assigned numbers for ordinary sections, book
  parts, appendices, and numbered special sections. The `%notitle` option hides
  only the Markdown heading while keeping its destination and body; unlike
  Asciidoctor HTML, Markdown applies this option to section headings.
- Tables of contents now render as nested Markdown links at the configured
  automatic, preamble, or macro position. `toc-title`, `toclevels`, macro
  `levels`, section numbers, and formatted section titles are preserved.
- Standalone inline anchors and IDs on bold, italic, monospace, highlight,
  subscript, superscript, and curved-quote spans now emit stable destinations
  in GFM and CommonMark, so local cross-references reach the inline content.
- Sections, discrete headings, and blocks with `[#id]` or `[[id]]` now emit
  stable HTML destinations in GFM and CommonMark, so generated local
  cross-references reach the referenced content.
- Title-based shorthand cross-references now link to the matching generated or
  explicit section ID, including when the reference supplies custom text.
- Cross-references preserve formatted explicit text through supported nested
  inline macros. Empty references to captioned blocks honor source-order
  `xrefstyle=basic`, `short`, and `full`, including custom and disabled
  captions.
- Visible index terms preserve inline formatting, links, and attribute
  substitutions; concealed terms remain hidden.
- Ordered lists honor a positive `start` value, including on nested lists.
  Every line of a nested list remains indented. Unsupported alphabetic and
  Roman styles continue to produce a warning and use numeric markers.
- Markdown conversion attributes now include the `markdown` backend and
  base-backend, `md` file type, `.md` output suffix, and their conditional
  convenience attributes.
- User-facing converter warnings are now collected in `ConversionResult` for
  recoverable Markdown conversion fallbacks such as skipped unsupported blocks,
  unsupported inline constructs, and capped heading levels.
- **`MarkdownVariant` enum** (`CommonMark` / `GitHubFlavored`) with `FromStr`
  and `Display`. `Processor::new` defaults to `GitHubFlavored`; use
  `Processor::with_variant` for another flavour.
- `Converter::name(&self)` returns `"markdown"` (replaces `Converter::backend()`).
- **Collapsible example blocks** — example blocks with `[%collapsible]` (and the
  `%open` modifier) now render as embedded `<details>/<summary>` HTML, which
  GitHub, GitLab, and most other Markdown renderers display as expandable
  sections. Applies to both delimited (`====`) and paragraph-style
  (`[example%collapsible]`) forms. When no title is given, the summary defaults
  to "Details", matching the HTML converter.
- **Description list fallback rendering** — description lists now render as unordered
  lists with bold terms and indented descriptions, instead of only emitting a warning
  comment.
- **Cross-references** — `<<id>>` and `xref:id[]` render as a link to the `#id`
  fragment. Its text is the reference's own text when it has one, otherwise the
  target's reference text (an explicit `[[id,label]]` label or its title),
  falling back to `[id]` as `asciidoctor` does. A reference inside another one's
  text renders as `[id]` text alone, since Markdown links do not nest.
- **Ordered list numbering-style warning** — ordered lists with an explicit
  non-numeric numbering style (`upperalpha`, `loweralpha`, `lowerroman`,
  `upperroman`, `lowergreek`, ...) now emit a warning that Markdown cannot
  represent the style and render with numeric markers. Numeric styles
  (`arabic`, `decimal`) and unstyled lists render numerically without a warning.

### Fixed

- `mailto:` macros no longer produce destinations with a duplicate `mailto:`
  scheme.
- Sections with a named `reftext` now use it as the link text and destination
  alias for natural references and explicit IDs. Their titles are not retained
  as second natural aliases, and formatted labels keep their Markdown markup.
- Plain visible shorthand cross-references now match section titles containing
  `pass:[...]` or `+...+` content. A shorthand target containing a passthrough
  remains unresolved, and its link displays and retains the visible text.
- When `:compat-mode:` is active at a title-based shorthand cross-reference,
  it keeps its literal fragment and bracketed unresolved fallback. Source-order
  changes apply only to later references, and explicit local IDs still link to
  the section title.
- Interdocument `xref:` macros now link to the corresponding `.md` file and
  fragment instead of a same-named local section. Empty and explicit link text
  both keep the external destination.
- Image destinations and audio and video fallback links honor `imagesdir`,
  normalize relative paths, and encode spaces as `%20`.
- Markdown output now uses one blank line between blocks and one final newline,
  without redundant spacing around quotes and nested lists.
- Description-list fallbacks now indent nested levels and preserve repeated
  continuations, formatted terms, titled boundaries, named styles, and
  trailing unanswered Q&A items.
- Unindented ordered and unordered markers now render as nested mixed lists,
  matching Asciidoctor's list ownership.
- Brackets in block attribute values no longer make the attribute line leak
  into Markdown output.
- **Inline markup in `link:` text** — link text with nested formatting is now rendered
  through the full inline pipeline instead of emitted verbatim.
