# Changelog

All notable changes to `acdc-converters-pdf` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

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
  attribute now create PDF destinations. References before or after the link
  target that destination and use the `[id]` fallback text. For duplicate link
  IDs, acdc keeps the first link destination so the PDF remains valid;
  Asciidoctor PDF writes duplicate destination names and leaves selection to
  the viewer.
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
