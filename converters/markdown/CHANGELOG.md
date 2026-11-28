# Changelog

All notable changes to `acdc-converters-markdown` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Markdown output comes in two variants, `gfm` (the default) and `commonmark`.
  Select one with `convert --variant`.
- Conversion sets `backend` and `basebackend` to `markdown`, `filetype` to `md`
  and `outfilesuffix` to `.md`.
- Fallbacks report a warning, once per unsupported feature per document.
- Document headers, section numbers and the TOC show as Markdown. The TOC uses
  `toc-title`, `toclevels` and the macro's `levels`.
- `%notitle` hides only a section's heading. Asciidoctor HTML ignores it on
  sections.
- Block titles show as bold lines, with numbered or custom captions.
- Quote, verse, literal, listing, source, example and abstract styles each get
  a Markdown form. Source blocks keep their callouts.
- Ordered lists use `start`. Description, horizontal and Q&A lists show as
  Markdown lists.
- GFM tables keep column alignment, spans, header rows and footers.
- Images, audio and video keep alt text, titles, sizes and links.
- Example blocks with `%collapsible` are written as `<details>` sections.
- STEM shows as code. UI macros, passthroughs and roles keep readable text. GFM
  writes `line-through` as strikethrough.
- Link fallback text uses `hide-uri-scheme`. CommonMark footnotes show as
  linked endnotes.
- IDs are written as HTML anchors, so local cross-references work in both
  variants.
- With `:acdc-index:` in the header, an `[index]` section gets a generated
  index, with `see` and `see-also` links.

### Changed

- Reversed ordered lists, including `[qanda]` lists, now count up from `start`,
  with a warning.
- Copyright and trademark signs, ellipses, em dashes and apostrophes are written
  as Unicode characters.
- Nested blocks and AsciiDoc table cells convert faster.

### Fixed

- Literal angle brackets, backslashes, ampersands and Markdown markers now stay
  visible. Inline code and links stay valid Markdown.
- Passthroughs, `subs=`, character references, document attributes and lists
  now match Asciidoctor more closely.
- Section references now use `xrefstyle` and the `section-refsig`,
  `chapter-refsig`, `appendix-refsig` and `part-refsig` attributes.
  `xref:target[xrefstyle=short]` sets the style for one reference.
- Numbered chapters use `chapter-signifier`.
- Cross-references now resolve more targets and use `reftext`. Interdocument
  `xref:` macros link to the `.md` file.
- Cross-references, captions, titles and link text now keep their formatting.
- `mailto:` links now percent-encode the subject and body.
- Image, audio and video targets now use `imagesdir`.
- Index terms now handle parentheses, brackets and escapes, and sort by their
  shown labels.
- Output now uses one blank line between blocks and ends with a single newline.
