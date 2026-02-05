# Changelog

All notable changes to `acdc-converters-html` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Trademark replacement** - `(TM)` is now converted to `&#8482;` (™) in normal text.
  Use `\(TM)` to escape and keep the literal text.

- **Dark mode support** - Documents with `:dark-mode:` attribute now render with a full
  dark color scheme (`#1a1a1a` background, adapted headings, links, code blocks, tables,
  admonitions, and named colors).
- **Syntax highlighting for source blocks** - Code blocks with a language specified now
  render with syntax highlighting using syntect. Outputs inline CSS styles. Falls back
  to plain text when the language isn't recognized. Requires `highlighting` feature flag.
- **Section numbering** - Documents with `:sectnums:` attribute now render numbered
  section headings (e.g., "1. Introduction", "1.1. Overview"). Respects `:secnumlevels:`
  to control depth of numbering.
- **ToC numbering** - Table of contents entries are now numbered when `:sectnums:` is
  set, matching asciidoctor behavior.
- Table colspan and rowspan rendering (`colspan="n"` and `rowspan="n"` attributes on `<th>`/`<td>`)
- Table visual attribute support:
  - `frame` attribute - controls outer border (`all`, `ends`/`topbot`, `sides`, `none`)
  - `grid` attribute - controls inner gridlines (`all`, `rows`, `cols`, `none`)
  - `stripes` attribute - controls row striping (`even`, `odd`, `all`, `hover`)
  - `width` attribute - sets explicit table width (e.g., `width=75%`)
  - `%autowidth` option - uses `fit-content` sizing instead of `stretch`
  - Custom roles from metadata applied as CSS classes
- Cell-level alignment overrides are now respected, falling back to column-level defaults
- Initial support for `[subs=...]` attribute on verbatim blocks (listing, literal)
  - `subs=none` - disables all substitutions, outputs raw content
  - `subs=specialchars` - only escapes HTML special characters
  - `subs=+replacements` - enables typography (arrows, dashes, ellipsis) in verbatim blocks
  - `subs=+attributes` - enables attribute expansion (`{attr}` → value) in verbatim blocks
  - `subs=+quotes` - enables inline formatting (`*bold*`, `_italic_`, etc.) in verbatim blocks
  - Default behavior unchanged (escapes HTML characters, no replacements/attributes/quotes)
  - Requires parser's `pre-spec-subs` feature flag. ([#280])

### Fixed

- TOC entries no longer produce invalid nested `<a>` tags. Inline elements that generate
  anchors (footnotes, links, cross-references, autolinks, mailto, inline anchors, index
  terms) are rendered as plain text within TOC entries. Formatting like bold and italic is
  preserved.
- Inline passthroughs `+...+` and `++...++` now preserve literal content instead of
  applying the enclosing block's typography replacements (e.g., `...` → ellipsis). The
  converter now uses the substitution list carried on each `Raw` node directly. ([#323])
- Admonition blocks now render Font Awesome icons when `:icons: font` is set, outputting
  `<i class="fa icon-{variant}" title="{caption}"></i>` instead of a text label. Matches
  asciidoctor behavior.
- Superscript (`^text^`) and subscript (`~text~`) now respect the quotes substitution
  setting, matching asciidoctor behavior. Previously they always rendered as `<sup>`/`<sub>`
  even when quotes was disabled (e.g., in listing blocks or with `[subs=-quotes]`).
- Passthrough content (`pass:[]`, `+++`, `++`, `+`) no longer has attribute references
  incorrectly expanded by the converter. Attribute expansion is now handled solely by
  the parser based on each passthrough's own substitution settings. ([#291])
- Verbatim blocks (listing/literal) now correctly skip typography replacements by default,
  matching asciidoctor behavior. Previously, smart quotes were incorrectly applied.
- HTML5 compliance: removed self-closing syntax (`<col />` → `<col>`, `<img />` → `<img>`)
  and deprecated `frameborder` attribute from iframes.
- Callout references in source blocks now render with `<b>` tags wrapping the number
  instead of the entire `<i class="conum">` element, matching asciidoctor output.
- Callout lists now render callout markers correctly, with proper `<b>` tag placement
  matching asciidoctor output. (This seems a repeat of the previous entry but we had
  messed it up earlier.)

### Changed

- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])

[#280]: https://github.com/nlopes/acdc/issues/280
[#291]: https://github.com/nlopes/acdc/issues/291
[#313]: https://github.com/nlopes/acdc/pull/313
[#323]: https://github.com/nlopes/acdc/issues/323
