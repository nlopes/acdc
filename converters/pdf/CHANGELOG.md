# Changelog

All notable changes to `acdc-converters-pdf` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Cross-references preserve formatted explicit text through supported nested
  inline macros. Empty references to captioned blocks honor source-order
  `xrefstyle=basic`, `short`, and `full`, including custom and disabled
  captions, matching Asciidoctor PDF.
- Visible index terms and generated catalog labels preserve inline formatting
  and attribute substitutions. Formatted labels remain distinct catalog terms,
  matching Asciidoctor PDF.
- Substitutions ordered after macros update both visible index terms and their
  catalog labels, matching Asciidoctor DocBook. Asciidoctor PDF leaves the
  earlier catalog value unchanged.
- `[index]` sections now generate page-linked catalogs from visible and
  concealed index terms, including secondary and tertiary terms. Empty index
  sections are omitted, and `index-pagenum-sequence-style=page` or `range`
  consolidates repeated page numbers. Non-screen media uses unlinked, unique
  page numbers with contiguous ranges. Catalogs use two columns by default,
  support theme-controlled column counts and gaps, and accept `%notitle` to hide
  the visible heading, matching Asciidoctor PDF.
- Tagged PDFs now include explicit or filename-derived alternative text for
  embedded block and inline images. Asciidoctor PDF 2.3.15 does not emit tagged
  PDF structure.
- With source highlighting enabled, source blocks honor `linenums`, `start`,
  and `highlight`, including source paragraphs, blocks without a language,
  callouts, and wrapped lines. PDF continues to ignore HTML-only wrapping
  options, matching `asciidoctor-pdf`.
- Book documents with `:partnums:` number level-zero parts with Roman numerals.
  Part headings use `Part` by default and honor a custom or unset
  `part-signifier`; chapter numbers continue across parts, matching
  `asciidoctor-pdf`.
- PDFs include the document title, subtitle, authors, subject, and keywords in
  their file metadata when these values are set.
- Document subtitles render below the title with inline formatting and links.
- Title pages show the document revision number, date, and remark below the
  authors. Normal article headers continue to omit revision details.
- PDF conversion attributes now include `backend=pdf`, `basebackend=html`,
  `filetype=pdf`, `outfilesuffix=.pdf`, `htmlsyntax=html`, and their conditional
  convenience attributes, matching Asciidoctor PDF's backend traits.
- Initial Typst-backed PDF converter with core AsciiDoc rendering, themed page chrome,
  branding and watermark options, colour emoji, local and remote images, asset warnings
  and strict mode, plus optional generated-Typst output for debugging. Stem content
  degrades with a warning and escaped textual fallback.

### Changed

- Audio and video blocks now render clickable, labelled static fallbacks instead
  of plain placeholder text. Local targets honor `imagesdir`; video alternatives
  remain separate links; and an available poster links to the first video source.
  Titles render below the fallback, playback options do not affect it, and one
  document-level warning explains that in-document playback requires HTML,
  matching Asciidoctor PDF without fetching playable media.
- Inline icons now support text, built-in glyph, and image modes. Text and
  missing-glyph fallbacks use the icon alternative text; image mode honors
  `iconsdir` and `icontype`, including `icons=svg` and similar format values.
  Font icons honor the `1x` through `5x`, `lg`, and `fw` sizes. Asciidoctor
  PDF ignores icon titles and image dimensions, so ACDC does too. Unsupported
  font icons and unavailable image icons emit a warning for each macro while
  retaining a readable text fallback.
- Inline passthroughs now honor special-character, quote, attribute,
  replacement, macro, and post-replacement policies, including their written
  order and the `normal` and `verbatim` groups. Escaped delimiters, numeric
  character references, and hard line breaks match Asciidoctor PDF. Raw backend
  markup remains literal text instead of being executed as Typst source.
- Link fallback text keeps `mailto:` for a `link:` target, preserves angle
  brackets around bracketed email autolinks, and maps inter-document
  references to the PDF output suffix, matching Asciidoctor PDF.
- Link roles use the same supported size, decoration, colour, and background
  formatting as other inline text. Browser-only link attributes do not affect
  PDF output, matching Asciidoctor PDF.
- When `hide-uri-scheme` is set, including to `false`, PDF links without custom
  text omit their URI scheme. Body attribute changes apply in source order,
  matching Asciidoctor PDF. Mailto macros and email autolinks retain one
  `mailto:` scheme in their destination and display the email address.
- Keyboard keys render as individual keycaps, buttons use bold bracketed
  labels, and menu paths use bold text with chevron separators, matching
  Asciidoctor PDF.
- Keyboard, button, and menu macros remain literal unless `experimental` is
  set, including when the attribute changes in the document body, matching
  Asciidoctor PDF.
- Ordered and unordered lists now preserve numbering and item ownership when
  continued blocks or nested lists span pages, matching Asciidoctor PDF.
- Description lists now keep nested levels indented and retain continued
  blocks with their owning item. Mixed nested lists, formatted terms, titled
  boundaries, named styles, and unanswered Q&A items match Asciidoctor PDF.
- Bibliography lists now use square markers and show each entry's bracketed
  reference label. Automatic citations link to entries, and entry labels link
  back to the first automatic citation. List titles, IDs, alignment roles, and
  attached blocks are preserved, matching Asciidoctor PDF.
- `[ordered]` and `[unordered]` description lists now render as numbered or
  bulleted lists. Terms are bold, punctuation and stacked answers are
  preserved, and attached content stays with its item, matching Asciidoctor
  PDF.
- `[qanda]` description lists now render numbered, italic questions with
  answers beneath them. Shared questions, attached content, nested Q&A lists,
  alignment roles, and numbering restarts match Asciidoctor PDF.
- Horizontal description lists now render terms and descriptions in a
  borderless two-column layout. The term column fits its content up to half
  the available width, and multiple terms and attached blocks stay with their
  description, matching Asciidoctor PDF.
- Block images without an explicit width now keep their intrinsic size instead
  of expanding to the page width. Oversized images still fit the available
  area, matching Asciidoctor PDF.
- Wide table cells now wrap long uninterrupted text, monospace and literal
  content, and nested source blocks instead of clipping or overlapping nearby
  cells, matching `asciidoctor-pdf`.
- Tables honor `frame`, `grid`, and static `stripes` values, including
  source-order `table-frame`, `table-grid`, and `table-stripes` defaults.
  Headers, footers, and merged cells keep their correct rules and fills. Unlike
  `asciidoctor-pdf`, stripe alternation continues across a page break instead
  of restarting after the repeated header.
- Article abstract sections now stay out of the PDF table of contents and
  document outline, use centered titles and italic text, and keep their heading
  number hidden while preserving the `sectnums=all` sequence, matching
  `asciidoctor-pdf`.
- Book abstracts now take chapter numbers, `sectnums=all` includes special
  sections, and ordinary chapter numbering continues after an appendix,
  matching `asciidoctor-pdf`.
- Book table-of-contents entries now omit part and chapter signifiers, nest
  chapters under their parts, and keep chapter numbering continuous, matching
  `asciidoctor-pdf`.
- Numbered book chapter headings now use the default `Chapter` signifier and
  honor custom, empty, or unset `chapter-signifier`, matching
  `asciidoctor-pdf`.
- In a book, `:partnums: false` now enables Roman part numbers because the
  attribute is set. Use `:partnums!:` to disable them, matching
  `asciidoctor-pdf`.
- Book parts and chapters now start on new pages by default, matching
  `asciidoctor-pdf`. Themes can let either heading follow preceding content or
  avoid a forced break before a part's first chapter.
- PDF paragraphs now support `text-left`, `text-center`, `text-right`, and
  `text-justify` for prose content. Literal, listing, and source paragraphs
  continue to ignore alignment roles, matching Asciidoctor PDF.
- Quote rules now span the content width and include the attribution, while
  paragraph alignment roles apply only to the quote body.
- Dialogue hard breaks and em dashes now match Asciidoctor PDF:
  paragraph-leading and trailing `--` are replaced, while dashes beside or at
  the edge of inline formatting stay literal.
- PDF paragraphs collapse repeated source spaces and ordinary newlines.
  Paragraph `hardbreaks` options and document `hardbreaks` attributes preserve
  newlines, while empty ` +` lines and `{empty} +` insert blank or leading
  lines, matching Asciidoctor PDF.
- PDF text uses the document `lang` value, including an optional region suffix.
  Unlike Asciidoctor PDF, values Typst cannot represent fall back to English
  with a warning.
- Running headers use the full document title, including its subtitle. An
  explicit `--title` value still overrides the running-header text.
- Title-page revision numbers use `version-label`; an empty or unset label
  removes the prefix.
- Author details no longer appear below a normal article title. A title page,
  including the default book title page, shows author names but omits email
  addresses, matching Asciidoctor PDF's default output. Running headers start
  after the first numbered page so they do not repeat the document title.
- Quote and verse attribution stays literal unless single-quoted, when
  formatting and links render. A citation without an attribution is hidden,
  matching Asciidoctor PDF.
- Nested the private PDF implementation crates under `converters/pdf/crates`; their Cargo
  package names and the converter's public API remain unchanged.
- Refined and hardened the initial backend: themes and images are validated and bounded,
  image access follows safe mode, fonts load only from explicitly configured directories,
  and unsupported stem notation remains escaped text. Table headers, TOC placement and
  configuration, `subs=`, images in titles, Unicode and punctuation-heavy
  cross-references, asset diagnostics, and timing counts now behave consistently with the
  rest of the converter pipeline.
- Cross-references to IDs on tables, listings, images, lists, admonitions, media, and
  other blocks now link to the target in the PDF. References without explicit link text
  use the target's reference label or title, including its inline formatting, and fall
  back to `[id]` for untitled targets. A reference to an id that no element
  defines renders as `[id]` text with no link, and a reference inside another
  one's reference text renders as `[id]`, so neither a broken nor a
  self-referencing target can fail the document's compilation.
- An anchored single-line admonition emits one PDF target, while anchors inside
  compound admonitions are preserved.
- `[subs="…"]` on a single-line admonition now applies to its content.
- PDF tables now preserve `n+|` column spans, `.n+|` row spans, and combined
  `n.m+|` spans. Declared header rows are emitted as semantic Typst table
  headers and repeat across page breaks.
- Block images now honor bare numeric and percentage `width` values. The
  generic `height` attribute and unit-suffixed `width` values remain ignored,
  matching Asciidoctor PDF.
- Inline images now keep their intrinsic size by default and honor bare numeric
  and percentage `width` values instead of being forced to the text height.
  Generic `height` and unit-suffixed `width` values remain ignored, matching
  Asciidoctor PDF.
- Block and inline images now honor `pdfwidth`, which overrides `width` and
  accepts PDF units, percentages, and intrinsic-width scaling. Block images
  also accept page-width scaling, matching Asciidoctor PDF.
- Block images now honor `align=left|center|right` and the corresponding
  alignment roles. A named alignment takes precedence over the last alignment
  role, matching Asciidoctor PDF.
- Block images with `float=left|right` now render on the requested side and
  emit a warning for each affected image. Following text starts below the image
  because Typst does not yet support side wrapping.
- Block and inline images now honor `link=`, including missing-image fallback
  text. Block-image captions remain outside the link, and an inline image's own
  link takes precedence over an enclosing link, matching Asciidoctor PDF. Empty
  link targets are ignored because Typst does not accept them.
- Titled block images now render as numbered figure captions below the image,
  including linked and missing images. Missing images show their alt text and
  source target. Untitled and inline images do not consume a figure number,
  matching Asciidoctor PDF.
- Captioned block images now stay with their captions across page breaks,
  matching Asciidoctor PDF.
- Figure captions honor custom, empty, and unset `figure-caption` values,
  document-level `caption`, and per-image `caption=` overrides. Disabled and
  explicit prefixes do not consume a figure number.
- Ordered lists honor explicit `arabic`, `decimal`, `loweralpha`, `upperalpha`,
  `lowerroman`, `upperroman`, and `lowergreek` numbering styles, matching
  Asciidoctor PDF.
- Unstyled nested ordered lists use Arabic, lower-alpha, lower-Roman,
  upper-alpha, and upper-Roman numbering through level five, then Arabic at
  deeper levels. Explicit parent styles do not affect their children, matching
  Asciidoctor PDF.
- Ordered lists honor positive `start` values across all supported numbering
  styles. Nested lists keep style and start attributes placed directly before
  their first item, matching Asciidoctor PDF.
- Named footnote references reuse the original footnote and its assigned number.
- Inline IDs such as `[#term]*Term*` now create PDF link targets on formatted
  text.
- A passthrough block's title serves only as cross-reference text and never
  appears above the block, matching `asciidoctor`.
- A quote or verse block keeps its attribution and cited work, set under the
  block after an em dash. A `[quote]`, `[verse]`, `[literal]`, `[listing]`,
  `[source]`, or `[example]` paragraph now reads as its delimited counterpart
  rather than as an ordinary paragraph.
- Verse renders as verse: proportional text with the line breaks the source
  gives it, rather than as monospace code.
- Example, sidebar, and open blocks each take their own treatment: a light
  frame, a shaded box with a centred title, and no frame at all. A titled
  example takes a numbered caption (`Example 1. Title`, using
  `example-caption`), matching the other backends.
- Abstract paragraphs and open blocks now use a centered optional title and
  larger italic body text instead of ordinary paragraph or quote styling.
- Titled example blocks and `[example]` paragraphs now share `example-caption`
  numbering; titled listing blocks, `[listing]` and `[source]` paragraphs share
  `listing-caption`. The same styles on an open or literal block are captioned
  too, though their content renders as before.
- Titled tables now use document-wide numbered captions and honor
  `table-caption` and per-table `caption=` values, matching `asciidoctor-pdf`.
  Nested tables take their number before their containing table.
- Caption numbering now follows the document: a caption attribute changed
  part-way through applies from that point on, a nested block is numbered before
  the one containing it, and blank, disabled, and custom captions are honoured,
  matching `asciidoctor-pdf`. A title added after parsing still takes a caption,
  numbered after every parsed one.
- Collapsible examples render as expanded disclosures with a marker and title, or
  `Details` when untitled. They do not consume an example number, and `%open` and
  `%closed` produce the same static PDF output, matching `asciidoctor-pdf`.
- A block title sits on its own line above its block instead of running into
  the content that follows it.
- Built-in roles now control inline text size, decoration, colour, and
  background in PDF output. Roles can be combined; unknown roles are ignored.
  Unlike Asciidoctor PDF, acdc renders `overline` and colour roles by default.
- Highlighted text keeps its default background only when it has neither an ID
  nor a role, matching Asciidoctor.
- Passthrough block titles remain available as automatic cross-reference text
  without being displayed above the block, matching Asciidoctor PDF.
- Passthrough blocks render as unframed monospace text instead of code cards,
  matching Asciidoctor PDF. Their content is always escaped and cannot be
  interpreted as Typst source.
- Ordinary block titles and numbered captions use the PDF theme's caption
  style. The default is near-10pt regular italic text in `#333333`, with
  spacing close to Asciidoctor PDF, instead of bold body-size text.
- Document blocks use a 12 pt vertical margin by default, including blocks in
  open containers, list continuations, and AsciiDoc table cells. List items and
  headings keep their own spacing, matching Asciidoctor PDF.
- Admonitions use text labels and a vertical divider, with titles above the body
  in the content column, matching Asciidoctor PDF.
- Source blocks use language-aware syntax highlighting when `source-highlighter`
  is set. Missing and unknown languages remain readable as plain source, and
  callout comment guards do not appear in the rendered code. Callout references
  use parenthesized number markers.
- Callout explanations use their matching source-callout number as the list
  marker, without an unrelated bullet.
- Source, listing, and literal blocks preserve indentation, blank lines, and
  configured tab stops, and wrap long code instead of clipping it at the page
  edge.
- Source, listing, and literal blocks expand document attributes when their
  `subs` configuration enables attribute substitution.
- Tables declared with `options=footer` retain their footer semantics in PDF
  output and show the footer once at the end of the table.
- Table columns now follow proportional, percentage, and automatic `cols`
  widths. Ordinary tables fill the available width; `%autowidth` tables keep
  content-sized columns, matching `asciidoctor-pdf`.
- Tables now honour numeric `width` percentages, including values without a
  `%` suffix. Local widths override `%autowidth` and the `stretch` role, and
  `%autowidth` alone remains content-sized. When a width or `stretch` fixes an
  autowidth table's size, its columns use declared or equal proportions because
  Typst cannot expand content-sized columns to a fixed table width.
- Tables can be placed at the left, centre, or right with `align=` or a matching
  role; valid local attributes take precedence over the last matching role and
  the PDF theme default. Table `float=` is ignored in print, matching
  `asciidoctor-pdf`.
- Table column specifications now align cell content horizontally with `<`,
  `^`, and `>`, and vertically with `.<`, `.^`, and `.>`, matching
  `asciidoctor-pdf`.
- Table cells now honour their own horizontal and vertical alignment markers
  in headers, bodies, footers, and spanned layouts. Column defaults follow each
  cell's source-row position after spans and across repeated cells, matching
  `asciidoctor-pdf`.
- Table columns and cells now apply emphasis, strong, monospace, literal, and
  header styles. Literal cells keep their source text and whitespace, while
  semantic header rows ignore cell styles, matching Asciidoctor PDF.
- AsciiDoc-style table cells now preserve nested blocks. Cell headings follow
  local section-numbering changes, number independently, and stay out of the
  outer table of contents and PDF outline. Their vertical alignment is
  preserved, while horizontal cell alignment does not affect nested blocks,
  matching `asciidoctor-pdf`.
- Section headings and table-of-contents entries now use one source-order number
  assigned by the parser, including parts, appendices, and special sections.

### Fixed

- Block and inline images honor `imagesdir`, normalize relative targets, and
  encode spaces as `%20`. Percent-encoded local image and poster targets still
  load from their corresponding filesystem paths, matching Asciidoctor PDF.
- Checklist-like prefixes in ordered lists now remain visible text instead of
  rendering as checkboxes, matching Asciidoctor PDF. ACDC continues to accept
  `[X]` in unordered checklists as an intentional extension.
- Unindented ordered, unordered, and checklist items now retain their automatic
  mixed-list nesting, matching `asciidoctor-pdf`.
- `[listing]`, `[source]`, `[literal]`, and `[verse]` before a block image
  macro now render the macro text in that block style instead of expanding an
  image, matching Asciidoctor PDF. Listing and figure counters remain separate.
