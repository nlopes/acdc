# Changelog

All notable changes to `acdc-converters-pdf` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Breaking:** configure conversion with the parser options builder, then use
  the converter's validated parser options for parsing. Invalid attribute values
  are rejected before conversion.
- PDF conversion borrows source-ordered attributes while preparing and rendering
  content, reducing copies of attribute names and values.

### Fixed

- Listing and literal blocks honour enabled quote and attribute substitutions,
  including bold, italic, nested formatting, and empty formatted anchor targets.
  Quote markup introduced after quote substitution stays literal, matching
  Asciidoctor PDF. Formatting survives code wrapping and syntax highlighting.
- Visible and concealed index terms no longer insert extra spaces in text,
  formatting, links, footnotes, or table cells. `x((Term))y` renders as `xTermy`,
  matching Asciidoctor PDF. Authored spaces remain intact, including before
  concealed terms where Asciidoctor PDF sometimes removes them.

- Attribute values consisting of a single backslash remain literal and no longer
  consume the following line. Tabs and extra spaces after the attribute name
  are accepted as separators. Real continuations include colon-prefixed lines
  as value text, matching Asciidoctor.

- Macro escapes retain extra backslashes and only consume an escape when the
  macro is complete and enabled. Backslashes introduced by later attribute
  substitution remain literal. As in Asciidoctor, `\link:https://example.org[Site]`
  keeps its leading backslash and creates a link, while `\https://example.org[Site]`
  stays literal. Multiple backslashes before a bare URL macro all remain visible.
  Enabled code links stay clickable with syntax highlighting, including cases
  where Asciidoctor PDF drops their annotations.

- Cross-references to IDs containing colons now resolve locally in both shorthand
  and `xref:` macros, including explicit labels and enabled code. As in
  Asciidoctor, use `xref:#a-b.c:d[]` for a local ID containing a dot;
  `xref:a-b.c:d[]` addresses an external resource. Existing passthrough targets
  remain supported, unlike Asciidoctor, which leaves those macros literal.

- Literal bracket backslashes now survive in escaped macros and ordinary text,
  matching Asciidoctor. Active links, cross-references, footnotes, and named
  index terms consume only their closing-delimiter escape. Footnotes with `\]`
  keep the complete body instead of ending at the escaped bracket.

- Index labels preserve literal square-bracket backslashes in shorthand terms
  such as `((One \] term))`. Named index macros remove one closing-bracket escape;
  enclosing links remove one more from display text after catalog registration,
  matching Asciidoctor.

- `mailto:address[label,subject,body]` now carries percent-encoded subject and
  body values, including Unicode, quoted commas, and empty arguments. Unlike
  Asciidoctor 2.0.26, acdc preserves literal `&` and passthrough text instead of
  encoding HTML entities or internal placeholders, keeps formatting and
  typography syntax as plain email text, accepts unquoted empty
  subjects, and correctly reads escaped apostrophes in single-quoted values.
  Existing query headers are retained; positional subject/body values replace
  matching headers and use `&`, rather than adding a second `?`. Message-body
  line breaks use CRLF encoding.

- Preserve literal quotes and commas in link labels unless attribute-list
  syntax applies. Accept escaped quotes in quoted link, URL, and mailto labels,
  including repeated backslashes, passthroughs, and enabled code. Link and URL
  labels use `=` to trigger attribute parsing; mailto labels use a comma,
  matching Asciidoctor. Index entries and footnote definitions retain their own
  quote escapes when displayed inside a quoted link label.

- Keep the complete text of link, URL, and mailto labels containing escaped
  closing brackets, including enabled code and passthroughs. Repeated backslashes
  retain the literal backslashes before the bracket, matching Asciidoctor.

- Render nested links, anchors, and footnotes inside `pass:m[...]` when their
  closing brackets are escaped. Reusing a named footnote in a later paragraph
  retains its definition.

- Resolve cross-references to inline anchors in code with `subs=+macros`,
  including highlighted, numbered, and wrapped listings, without inserting
  spaces into the code. Code containing only an anchor also creates a target.
  Unlike Asciidoctor PDF, formatted anchors and final anchor-only lines retain
  usable targets.

- Named footnote markers no longer add an unwanted space before the following text or code.

- Render footnotes enabled by `subs=+macros` in code and literal blocks, including highlighting, wrapping, line numbers, note-only lines, and link labels. Markers and backlinks remain clickable, including cases where Asciidoctor PDF with Rouge loses links.

- Keep links and cross-references functional in verbatim blocks with `subs=+macros`, including highlighted, numbered, wrapped, and autofit code. Explicit link IDs remain valid destinations, and unresolved references render as text. Unlike Asciidoctor PDF with Rouge, highlighted links retain clickable destinations.

- Keep index labels unchanged by later substitutions and avoid duplicate footnotes from catalog labels, matching Asciidoctor PDF. Recognize index terms in ordered passthroughs and retain entries whose visible labels become empty.

- Index catalogs retain literal attribute references and quote markers when `subs=` puts
  attributes or quotes after macros, matching Asciidoctor registration order. Visible
  terms still receive the later substitutions.

- Index terms in numbered source blocks keep their code aligned with the line
  numbers. Page links remain on the term's page when a code line is near a page break.

- Index terms enabled with `subs=+macros` in listings and literal blocks join
  the catalog with page links to their code lines, including highlighted and
  numbered source blocks.

- Early indexes include terms in later headings that have automatic IDs, without
  page numbers until those headings are rendered. Explicit IDs and `sectids!`
  suppress early registration, matching Asciidoctor PDF.

- Multiple index catalogs keep `see` and `see-also` links within their own
  catalog without duplicate definition targets. This also prevents PDF generation
  from failing when the same term is referenced by more than one catalog.

- Cross-references to anchors inside horizontal description-list terms no longer
  fail PDF generation. Links point to the visible term.

- File-qualified references to fully included sources create internal PDF
  links and honor `xrefstyle`. Whole-document references link to the first
  page and display `[^top]` unless text is supplied, matching asciidoctor-pdf.

- Explicit `xrefstyle=basic` emphasizes automatic chapter and appendix titles,
  matching Asciidoctor. Selected styles retain this emphasis for unnumbered
  chapters; omitted or unset styles keep the title as written.

- Automatic cross-references to numbered sections honor `xrefstyle`, matching
  Asciidoctor: `short` gives `Section 1.1`, `Chapter 2`, `Appendix A` or
  `Part I`, and `full` adds the title, in quotation marks for a section or part
  and in emphasis for a chapter or appendix. The word comes from
  `section-refsig`, `chapter-refsig`, `appendix-refsig` or `part-refsig` as it
  stands where the reference is written, and unsetting it leaves the number
  alone. As in Asciidoctor, a section deeper than `sectnumlevels` is still
  numbered in a reference although its heading is not. Previously every
  section reference showed only the title.
- `xref:target[xrefstyle=short]` sets the style for that one reference, and the
  text before the first named attribute is the link text, as in Asciidoctor.
  Previously the whole bracket content, `xrefstyle=short` included, became the
  link text.

- Explicit duplicate IDs no longer stop PDF generation. References use the first
  definition, while each section keeps its own table-of-contents destination.
  Asciidoctor PDF emits duplicate destination names, whose resolution can vary
  by viewer. Typst diagnostics for known source IDs show their original names
  and source locations.
- Anchors in copied titles and reference text no longer create extra destinations.
  References to anchors in repeated table headers resolve to their first occurrence.

- Repeated section titles receive distinct generated destinations, allowing
  PDF cross-references and table-of-contents entries to render successfully.

- Index terms accept parentheses in named macros and brackets in concealed
  shorthand. Nested delimiters behave consistently at paragraph starts and
  after text, matching Asciidoctor.
- `[source]`, `[listing]`, and `[literal]` blocks with `--` delimiters now
  keep double parentheses literal, matching Asciidoctor (#455).
- Index `see` and `see also` targets can contain parentheses without a parse
  error (#455). Backticks still allow index markup, as in Asciidoctor; use
  passthroughs inside backticks for literal code.
- Bibliography entries substitute attributes at their source position while
  citation labels retain their original attribute references, matching Asciidoctor.
- AsciiDoc table cells apply their child document settings without changing
  parent or sibling settings.
- Body document attributes now apply in source order to later PDF assets,
  media and icon paths, source blocks, section labels and numbering,
  cross-reference suffixes, and table autofit. Asset discovery observes the
  same order, and nested AsciiDoc table-cell changes do not escape.
- PDF parser attributes now expose `embedded` only when embedded output is selected.

- Document-title IDs now create named PDF destinations, and automatic
  references to them use the full formatted title and subtitle. With stacked
  title anchors, the last one is retained, matching Asciidoctor PDF.
- PDF indexes now link index terms in repeated table headers to the last
  rendered occurrence instead of failing when Typst lays out the header more
  than once, matching Asciidoctor PDF.
- Sections with a named `reftext` now link natural references and explicit IDs
  with that display label. Their titles are not retained as second natural
  aliases, and formatted labels keep their PDF styling, matching Asciidoctor
  PDF.
- Plain visible shorthand cross-references now match section titles containing
  `pass:[...]` or `+...+` content. A shorthand target containing a passthrough
  remains unresolved and displays its visible text without a broken Typst
  label.
- When `:compat-mode:` is active at a title-based shorthand cross-reference,
  it renders its bracketed unresolved fallback without a PDF destination.
  Source-order changes apply only to later references, while explicit local IDs
  still link to the section. Asciidoctor PDF retains a broken link for the
  unresolved target; acdc omits it so Typst compilation succeeds.
- Interdocument `xref:` macros no longer link to a same-named local section.
  Their file and fragment targets produce external PDF links, matching
  Asciidoctor PDF; natural shorthand references remain local.

### Added

- Title-based shorthand cross-references now link to the matching generated or
  explicit section destination, including when the reference supplies custom
  text.
- Explicit links, direct URL macros, and `mailto:` macros with a named `id`
  attribute now create named PDF destinations under that ID. References before
  or after the link target that destination and use the `[id]` fallback text,
  while links from other PDFs can address the same ID. For duplicate link IDs,
  acdc keeps the first link destination so the PDF remains valid; Asciidoctor
  PDF writes duplicate destination names and leaves selection to the viewer.
- PDF output includes a semantic tag tree, document language, and image
  alternative text for content supported by Typst. This baseline tagged output
  does not claim PDF/UA-1 conformance.
- Initial Typst-backed PDF converter with broad support for AsciiDoc document
  structure, blocks, inlines, navigation, lists, tables, images, source code,
  indexes with `see` and `see-also` relationships, and books. It includes PDF
  themes, page headers and footers, portrait and landscape layouts, A3/A4/A5 and
  common US page sizes, custom page dimensions and per-document margins, document
  metadata, lower-Roman front matter with configurable Arabic page-numbering
  starts, trusted fonts, safe asset loading, strict asset checks, and optional
  Typst output for diagnostics. Print indexes keep Roman page labels separate
  and can span from the final Roman label into a contiguous Arabic range.
