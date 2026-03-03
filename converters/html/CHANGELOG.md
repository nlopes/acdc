# Changelog

All notable changes to `acdc-converters-html` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **Blockquote citation links** — URL macros in blockquote attributions (e.g.,
  `-- https://example.com/[Example]`) are now rendered as clickable links. Citation titles
  use the `<cite>` element. ([#357])

### Added

- **`:hide-uri-scheme:` support** — when the document attribute `:hide-uri-scheme:` is set, the URI
  scheme (e.g., `https://`, `http://`, `ftp://`) is stripped from displayed link text for autolinks,
  URL macros, and link macros without custom text. The `href` attribute retains the full URL. ([#359])

- **Collapsible blocks in standard mode** — example blocks with `[%collapsible]` and paragraphs
  with `[example%collapsible]` now render as `<details>/<summary>` elements in the standard HTML
  backend. Supports `%open` for initially expanded blocks and defaults to "Details" as the
  summary text when no title is provided.

### Fixed

- **Admonition font icons** — use native Font Awesome 7 class names (`fa-solid fa-circle-info`,
  `fa-lightbulb`, etc.) instead of custom `icon-*` classes that only work with asciidoctor's
  embedded CSS. Icons now render correctly with the FA 7.2.0 CDN.

### Added

- **Docinfo file support** — inject custom HTML snippets into `<head>`, after `<body>`, or
  before `</body>` via the `:docinfo:` attribute. Supports `shared`, `private`, and granular
  values (`shared-head`, `private-footer`, etc.), `:docinfodir:` for alternate directories,
  and `:docinfosubs:` for attribute substitution. Disabled in embedded mode and
  `SafeMode::Secure`.
- **No-stylesheet mode (`:!stylesheet:`)** — setting `:!stylesheet:` now disables all
  stylesheet output: no embedded `<style>`, no linked `<link>`, no Google Fonts link, and
  no `copycss` file writing. Other head elements (MathJax, Font Awesome, syntax CSS) are
  still rendered.
- **Webfonts attribute control** — the `:webfonts:` attribute now controls the Google Fonts
  `<link>` tag: `:!webfonts:` suppresses it entirely, a custom value like
  `:webfonts: Roboto:400,700` uses that value in the URL, and the default (empty) emits
  the standard Open Sans / Noto Serif / Droid Sans Mono link.
- **Built-in stylesheet copying for `linkcss`** — when `:linkcss:` is set with the default
  stylesheet, the built-in CSS content is now written to disk (e.g., `asciidoctor-light-mode.css`)
  instead of silently failing because no source file exists.
- **`copycss` source path override** — when `:copycss:` has a non-empty string value, it is
  used as the source file path to read the stylesheet from, decoupling the source location
  from the output filename specified by `:stylesheet:`.
- **Appendix support (`[appendix]` style on level-0 sections)** — in book doctype, level-0
  sections with `[appendix]` style are demoted to level 1 and prefixed with "Appendix A: ",
  "Appendix B: ", etc. in both section headings and TOC entries. The caption is configurable
  via `:appendix-caption:` and can be disabled with `:!appendix-caption:`. ([#343])
- **Part numbering (`:partnums:` / `:part-signifier:`)** — book doctype documents with
  `:partnums:` now render part headings and TOC entries with uppercase Roman numeral
  prefixes (e.g., "Part I. ", "Part II. "). The signifier text is configurable via
  `:part-signifier:`. Chapter numbering resets at each part boundary. ([#342])
- **Custom embedded stylesheets** — when `stylesheet` and `stylesdir` attributes are set
  without `linkcss`, the CSS file is read from disk and embedded in a `<style>` tag,
  replacing the default stylesheet. Falls back to the built-in CSS if the file cannot be
  read.
- **CSS class-based syntax highlighting** — new `:syntect-css: class` document attribute
  switches syntax highlighting from inline `style=` attributes to CSS class names
  (`class="syntax-*"`). A corresponding `<style>` block with theme CSS is automatically
  embedded in `<head>`. When `:linkcss:` is set, the CSS is instead linked via
  `<link rel="stylesheet" href="{stylesdir}/acdc-syntect.css">` and written to disk
  alongside the HTML output (analogous to asciidoctor's `asciidoctor-coderay.css`).
  The `:syntect-style:` attribute allows overriding the default theme
  (e.g., `:syntect-style: Solarized (dark)`). Inline mode remains the default
  for backward compatibility. ([#341])
- **Book doctype parts rendering** — level 0 sections render as standalone
  `<h1 class="sect0">` (no wrapper div), matching asciidoctor. Body class now respects
  `:doctype: book` document attribute, and TOC includes level 0 entries. ([#312])
- **Semantic HTML5 backend (`html5s`)** — new `--backend html5s` option produces semantic HTML5
  using `<section>`, `<aside>`, `<figure>`, ARIA roles, and proper heading hierarchy instead of
  the traditional div-based layout. Inspired by Jakub Jirutka's
  [asciidoctor-html5s](https://github.com/jirutka/asciidoctor-html5s). Includes dedicated
  light and dark mode stylesheets, and supports `html5s-force-stem-type`,
  `html5s-image-default-link`, and `html5s-image-self-link-label` document attributes. ([#329])
- **Bibliography list class** - Unordered lists inside `[bibliography]` sections now render
  with `class="ulist bibliography"` on the wrapper div and `class="bibliography"` on the
  `<ul>` element, matching asciidoctor.
- **Bare URL `class="bare"`** - Autolinks (bare URLs without display text) now render with
  `class="bare"` on the `<a>` tag, matching asciidoctor.
- **URL `role` attribute** - URL macros (`https://...[]`) now support the `role` attribute,
  rendered as a CSS class on the `<a>` tag.
- **Link `target` attribute** - `window=` and `target=` attributes on URL, link, and mailto
  macros now render as `target="..."` on the `<a>` tag. `_blank` automatically adds
  `rel="noopener"` for security, matching asciidoctor behavior.

- **Trademark replacement** - `(TM)` is now converted to `&#8482;` (™) in normal text.
  Use `\(TM)` to escape and keep the literal text.
- **Copyright replacement** - `(C)` is now converted to `&#169;` (©) in normal text.
  Use `\(C)` to escape and keep the literal text.
- **Registered replacement** - `(R)` is now converted to `&#174;` (®) in normal text.
  Use `\(R)` to escape and keep the literal text.

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

- **Duplicate footnote IDs in TOC entries** — footnote `<sup>` elements in TOC entries
  no longer emit `id="_footnote_{id}"`, avoiding duplicate IDs that caused broken DOM
  rendering in browsers (particularly visible in the WASM editor preview).
- **`copycss` skipped in embedded mode** — `after_write()` no longer runs `handle_copycss()`
  or `handle_copy_syntax_css()` when embedded mode is active, matching asciidoctor's behavior
  of only writing stylesheet files for standalone output.
- **Curly quotes no longer over-applied** — single quotes are now only converted to
  `&#8217;` when they appear between alphanumeric characters (actual apostrophes like
  `it's`). Previously, `'word'` pairs were incorrectly converted to curly quotes.
- **Admonition content wrapped in paragraph** — single-paragraph admonition content now
  renders inside `<div class="paragraph"><p>...</p></div>`, matching asciidoctor output.
- **TOC renders in embedded mode** — documents with `:toc:` attribute now generate a
  table of contents in embedded mode for both standard and semantic HTML variants.
  Previously, TOC was only rendered for the semantic variant.
- **TOC renders for headerless documents** — documents without a `= Title` but with
  `:toc:` attribute now generate a TOC. In full mode, the TOC is wrapped in a
  `<div id="header">` block matching asciidoctor placement.
- **TOC class downgraded in embedded mode** — `:toc: left` and `:toc: right` now use
  `class="toc"` instead of `class="toc2"` in embedded mode, since sidebar positioning
  doesn't apply without the full page layout.
- **`toc::[]` macro adds `class="title"` to toctitle** — the `#toctitle` div inside a
  `toc::[]` macro block now includes `class="title"`, matching asciidoctor output.
- **No trailing newline after `</html>`** — the HTML output no longer appends an extra
  newline after the closing `</html>` tag.
- **Ordered list depth styling** - Nested ordered lists rendered via cross-type
  nesting (e.g., an ordered list inside an unordered item) now get depth-appropriate
  styling (`loweralpha` for depth 2, `lowerroman` for depth 3, etc.) instead of
  always using `arabic`. Depth is derived from the marker (`.` = 1, `..` = 2).
- **Callout rendering matches icon mode** - Callout markers and callout lists now branch on
  the `:icons:` attribute. Default mode uses `<b class="conum">(N)</b>` markers and `<ol>`
  lists; `:icons: font` mode uses `<i class="conum">` markers and `<table>` lists, matching
  asciidoctor output in both modes.
- **Named footnote references** - `footnote:name[]` references now render with
  `class="footnoteref"` and no IDs, matching asciidoctor. Previously, all occurrences
  used `class="footnote"` and duplicated the `id="_footnote_{name}"` attribute.
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
- Bare `&` in URL query strings within `href` attributes is now escaped to `&amp;` for
  valid HTML. Applies to all link types: autolinks, link macros, URL macros, mailto macros,
  and inline/block images with link attributes.

### Changed

- **Section numbering types moved to `acdc-converters-core`** — `SectionNumberTracker`,
  `PartNumberTracker`, `AppendixTracker`, `to_upper_roman`, and `DEFAULT_SECTION_LEVEL`
  are now re-exported from `acdc-converters-core::section` instead of being defined locally.
- **BREAKING**: Updated to new `Converter` trait API (renamed from `Processable`) ([#313])

[#357]: https://github.com/nlopes/acdc/issues/357
[#359]: https://github.com/nlopes/acdc/issues/359
[#280]: https://github.com/nlopes/acdc/issues/280
[#342]: https://github.com/nlopes/acdc/issues/342
[#291]: https://github.com/nlopes/acdc/issues/291
[#313]: https://github.com/nlopes/acdc/pull/313
[#323]: https://github.com/nlopes/acdc/issues/323
[#329]: https://github.com/nlopes/acdc/issues/329
