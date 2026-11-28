# Changelog

All notable changes to `acdc-converters-core` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Cross-references support `xrefstyle=short`, `xrefstyle=full` and
  `<name>-refsig` in every backend. `<<Section Title>>` resolves by title.
- Links without their own text can hide the URI scheme with `hide-uri-scheme`.
- Each backend sets `backend`, `basebackend`, `filetype`, `outfilesuffix`,
  `htmlsyntax` and convenience attributes such as `doctype-book`.
- Attributes set or unset in the document body apply from that point on.
- Converters report non-fatal problems as warnings.
- Source blocks share `linenums`, `start` and `highlight` handling across
  backends. A block with a line over 16 KiB isn't highlighted, and gets a
  warning.
- Terminal and manpage ordered lists number by their style, such as
  `loweralpha`. This matches Asciidoctor.
- Plain-text headings and captions include every inline element. Non-HTML
  backends decode character references.
- Replacements also apply to index term text, unlike Asciidoctor.

### Changed

- Plain text shows the parser's resolved typography and never applies it
  twice.
- The `embedded` attribute is set only for embedded output. The output
  directory isn't exposed as an attribute.
- Source blocks read the language only from position 2.
- Numbered sections and body attribute changes use less memory.

### Fixed

- Cross-references now match Asciidoctor more closely, including captions,
  `xrefstyle=basic`, `reftext` and inter-document targets.
- Section numbers come from the parser in every backend. Book chapter numbers
  continue across parts, matching Asciidoctor.
- TOCs use `toc2`, `toc-position`, `toc-placement` and `chapter-signifier`.
- Index sections now generate anywhere, including book parts and AsciiDoc
  table cells.
- Table column widths use one numeric scale, matching Asciidoctor. Footnotes
  in AsciiDoc table cells now show their text.
- Attributes passed with `-a` or through the parser API now override document
  entries. Fallbacks such as `lang=en` resolve as `{lang}` only when set.
- Attribute values, including `imagesdir`, keep literal quotes. Relative media
  URIs resolve against `imagesdir`.
- Any `icons` value other than `font` selects image icons. YouTube and Vimeo
  URLs keep their playback options, matching Asciidoctor.
- `mailto:` links percent-encode the subject and body. Unlike Asciidoctor
  2.0.26, acdc keeps literal `&` and passthrough text there.
- Backslash escapes, passthrough arrows and em dashes now match Asciidoctor.
- Highlighted HTML code keeps inline anchors and footnote markers.
- Builds with `--no-default-features` no longer turn on the parser's default
  features.
- Conversion creates missing output directories. ([#358])
- The preamble wrapper appears only with a document title, a section, and
  content before that section. ([#275])

[#275]: https://github.com/nlopes/acdc/issues/275
[#358]: https://github.com/nlopes/acdc/issues/358
