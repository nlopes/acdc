# Changelog

All notable changes to `acdc-converters-pdf` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Typst-based PDF converter, with trusted fonts, safe asset loading,
  strict asset checks and optional Typst output. The `acdc-pdf-*` changelogs
  cover themes, fonts, pages and images.
- `--emit-theme-config FILE` still needs a document or `--stdin`. It leaves
  out page overrides from the document or command line. acdc refuses a `FILE`
  that matches an input or output file.
- `--theme-format asciidoctor` without `--theme` uses the bundled Asciidoctor
  default theme, only with `--emit-theme-config`.
- Page breaks with `page-layout=portrait|landscape`, or the matching role,
  switch later pages to that orientation. Invalid values produce a warning.
- Links with an `id`, sections, anchors and blocks create named PDF
  destinations. Other PDFs can link to them, as in
  `manual.pdf#nameddest=chapter`. For duplicate IDs, the first wins.

### Changed

- Typography such as `(C)`, `...`, `--` and `->` now shows as symbols, also in
  backticks. This matches Asciidoctor PDF.
- Images over the loading limits show fallback text and a warning, or an error
  with `--strict`.

### Fixed

- Passthroughs, character references and attribute values now match
  Asciidoctor PDF more closely. Block `subs=` works in every build.
- Inline formatting, spacing and verse text now match Asciidoctor PDF more
  closely. Nested bold and italic use a bold italic font.
- With `subs=+macros`, links, cross-references, footnotes and index terms in
  code now work. Listing and literal blocks apply enabled quote and attribute
  substitutions. Callouts use `line-comment`.
- Link labels and macro escapes now match Asciidoctor more closely.
  `mailto:address[label,subject,body]` now adds the subject and body.
- Cross-references now match Asciidoctor PDF more closely. Numbered sections
  use `xrefstyle` and the `section-refsig`, `chapter-refsig`, `appendix-refsig`
  and `part-refsig` attributes. `xref:target[xrefstyle=short]` sets the style
  for one reference. Duplicate IDs no longer stop PDF generation.
- Footnotes in backticks and AsciiDoc table cells now keep their own notes.
- Indexes now sort and merge entries by their shown labels. Index terms work
  in repeated table headers and with parentheses or brackets.
- Description lists keep shared terms and empty definitions. Nested ordered
  lists no longer inherit `start` or `%reversed`.
- Images, icons and videos now apply `icon=`, `link=`, `title=` and alt text.
- `[sidebar]` and `[pass]` paragraphs now show like their delimited blocks.
- Attribute entries in the body now apply in source order. AsciiDoc table
  cells keep their own settings.
- PDF conversion sets `embedded` only with `--embedded`.
- An invalid or oversized `pdf-page-margin` falls back to the theme's margins.
