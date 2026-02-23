# Changelog

All notable changes to `acdc-editor-wasm` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Switch preview from standard (div-soup) HTML to html5s semantic HTML variant,
  outputting `<section>`, `<aside>`, `<figure>`, ARIA roles, and semantic class
  names instead of `<div class="sect1">`, `<div class="admonitionblock">`, etc.
- Overhaul preview CSS to target html5s elements and classes (`section.doc-section`,
  `aside.admonition-block`, `figure.example-block`, `aside.sidebar`,
  `section.quote-block`, `.listing-block`, `figure.image-block`, `ol.toc-list`,
  `section.footnotes`, `ol.callout-list`, semantic list wrappers, etc.).

### Fixed

- Autolink highlighting now covers the entire URL (last character was excluded).

### Added

- MathJax 4 integration for rendering `asciimath` and `latexmath` expressions in
  the preview pane. Both stem blocks (`[stem]`) and inline stem macros (`stem:[]`)
  are now rendered as typeset math instead of raw delimiters. MathJax is
  lazy-loaded from CDN only when the document sets `:stem:`.
- Preview styling for semantic HTML5 elements: literal blocks
  (`section/div.literal-block`), verse blocks (`section/div.verse-block`), STEM/math
  blocks (`figure/div.stem-block`), open blocks (`section/div.open-block`), description
  list wrappers (`section/div.dlist`), horizontal description lists (`dl.horizontal`),
  Q&A lists (`dl.qanda`), untitled image blocks (`div.image-block`), and titled listing
  block figcaptions (`figure.listing-block`).
- Comprehensive preview styling for all AsciiDoc constructs: admonitions,
  example blocks, sidebars, quote blocks, verse blocks, listing/literal blocks,
  tables (frames, grids, striping, alignment), lists (ordered, unordered,
  checklists, description, callout), images, footnotes, keyboard/menu/button
  macros, collapsible blocks, block titles, lead paragraphs, text color roles,
  and utility classes.
- Pane labels ("Editor" / "Preview") so users immediately know which side is which.
- Default cursor on preview pane instead of text cursor.
- Granular syntax highlighting for all macros (image, video, audio, footnote,
  link, icon, kbd, btn, menu, stem, pass, xref): target in green, bracket
  content in dark pink.

## [0.3.0] - 2026-02-14

### Added

- Add `:toc: macro` and `toc::[]` to the default template.

## [0.2.2] - 2026-02-14

### Fixed

- When a block gives an error, don't hide the block, show it without any highlighting.
- TOC preview no longer breaks when a section title contains a footnote (duplicate
  `id` attributes caused browsers to corrupt the DOM when using `set_inner_html`).

## [0.2.1] - 2026-02-05

### Fixed

- Source code blocks in the preview now render with syntax highlighting. The
  `source-highlighter` attribute is now set automatically, enabling the
  highlighting feature that was compiled in v0.2.0 but not activated.

## [0.2.0] - 2026-02-05

### Added

- Comprehensive README with API reference, CSS classes, and usage examples
- Syntax highlighting support via `acdc-converters-html` `highlighting` feature
- wasm-bindgen release profile settings for optimized builds

### Changed

- Reduced input debounce from 250ms to 25ms for more responsive editing

### Fixed

- GitHub issues link now points to correct repository (`nlopes/acdc`)

## [0.1.0] - 2026-02-04

### Added

- Initial WASM package for the AsciiDoc live editor
- AST-based syntax highlighting with `<span class="adoc-*">` CSS classes
- Live preview using the same parser and HTML converter as the CLI
- DOM orchestration: debounced parsing, scroll sync, Tab key insertion,
  clipboard copy, and pre-filled GitHub issue links
- GitHub Actions release workflow (`release-editor-wasm.yml`) for building
  with wasm-pack and publishing as GitHub Release assets

[Unreleased]: https://github.com/nlopes/acdc/compare/acdc-editor-wasm-v0.3.0...HEAD
[0.3.0]: https://github.com/nlopes/acdc/compare/acdc-editor-wasm-v0.2.2...acdc-editor-wasm-v0.3.0
[0.2.2]: https://github.com/nlopes/acdc/compare/acdc-editor-wasm-v0.2.1...acdc-editor-wasm-v0.2.2
[0.2.1]: https://github.com/nlopes/acdc/compare/acdc-editor-wasm-v0.2.0...acdc-editor-wasm-v0.2.1
[0.2.0]: https://github.com/nlopes/acdc/compare/acdc-editor-wasm-v0.1.0...acdc-editor-wasm-v0.2.0
[0.1.0]: https://github.com/nlopes/acdc/releases/tag/acdc-editor-wasm-v0.1.0
