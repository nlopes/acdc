# Changelog

All notable changes to `acdc-converters-pdf` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- PDF conversion attributes now include `backend=pdf`, `basebackend=html`,
  `filetype=pdf`, `outfilesuffix=.pdf`, `htmlsyntax=html`, and their conditional
  convenience attributes, matching Asciidoctor PDF's backend traits.
- Initial Typst-backed PDF converter with core AsciiDoc rendering, themed page chrome,
  branding and watermark options, colour emoji, local and remote images, asset warnings
  and strict mode, plus optional generated-Typst output for debugging. Unsupported icons,
  audio, and video degrade with warnings or textual fallbacks.

### Changed

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
- Named footnote references reuse the original footnote and its assigned number.
- Inline IDs such as `[#term]*Term*` now create PDF link targets on formatted
  text.
- A passthrough block's title serves only as cross-reference text and never
  appears above the block, matching `asciidoctor`.
- Built-in roles now control inline text size, decoration, colour, and
  background in PDF output. Roles can be combined; unknown roles are ignored.
  Unlike Asciidoctor PDF, acdc renders `overline` and colour roles by default.
- Highlighted text keeps its default background only when it has neither an ID
  nor a role, matching Asciidoctor.
- Passthrough block titles remain available as automatic cross-reference text
  without being displayed above the block, matching Asciidoctor PDF.
