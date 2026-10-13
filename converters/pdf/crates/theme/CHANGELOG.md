# Changelog

All notable changes to `acdc-pdf-theme` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The crate now lives under `converters/pdf/crates` as a non-publishable implementation
  component of `acdc-converters-pdf`; its Cargo package name remains unchanged.

### Added

- Native themes can set page dimensions and margins in points, plus orientation,
  under `page`. Omitted page settings keep the existing PDF defaults.

- Convert supported Asciidoctor PDF 2.3.15 theme settings into native acdc themes
  and page defaults. Unsupported settings and differences are reported by source
  key path. References, numeric expressions, and the pinned `base` and `default`
  parent themes are supported. External parent themes and font catalogs are not loaded.
  Quoted RGB variables keep their leading zeros. Heading sizes follow section
  roles: `heading_h2_font_size` through `heading_h6_font_size` set native levels
  1 through 5. Separate document-title and book-part sizes are unsupported and
  produce a warning.
  Valid explicit code sizes and minima produce no adjustment warning. The
  native fallback minimum is reduced only when needed to fit the normal size.

- Bundled bold italic serif and sans fonts preserve both styles when emphasis
  and strong text are combined, including in headings and custom themes.
- Bundle IBM Plex Math as a symbol fallback, so double arrows such as `⇒`,
  `⇐` and `⇔` render without an external font directory. CJK fonts remain
  caller supplied.
- Bundled italic and bold-italic monospace fonts preserve emphasis in code
  blocks when quote substitutions are enabled.
- Themes can control the page-header alignment, title size and weight, logo
  height, page-one visibility, footer size, and page-number position. Existing
  themes retain the previous header and footer appearance.
- Themes can set `page_numbering_start_at` to `cover`, `title`, `toc`,
  `after-toc`, `body`, or a positive body-page number. Earlier pages use
  lower-Roman labels, and Arabic numbering restarts at 1.
- Themes can control the normal and minimum block-code sizes used by PDF
  autofit. Existing themes retain the current 0.8 em normal size and use a
  0.6 em minimum.
- Themes can control the number of index catalog columns and their gap. The
  bundled defaults are two columns with a 12 pt gap; custom themes without a
  gap use the body font size, matching Asciidoctor PDF.
- Themes can place tables at the left, centre, or right of the available page
  width. The default is left.
- Themes can control table borders, the header divider, row stripes, and header
  and footer backgrounds. The defaults match Asciidoctor PDF.
- Themes can control the vertical margin between document blocks. The 12 pt
  default matches Asciidoctor PDF.
- Themes can control the alignment, colour, size, weight, style, and spacing of
  block titles and captions. The defaults closely match Asciidoctor PDF.
- Themes can control page breaks before book parts and chapters and can avoid a
  forced break before a part's first chapter.
- This crate - the starting code.
