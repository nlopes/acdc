# Changelog

All notable changes to `acdc-converters-terminal` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Typography shows as Unicode: dashes, arrows, ellipses, symbols, curved
  quotes, superscript and subscript.
- Output fits the terminal width, up to 120 columns. Example, sidebar, open
  and admonition blocks are drawn as boxes, and text wraps inside them.
- Document headers, tables of contents and section numbering work, including
  `sectnums`, `partnums` and appendices.
- Lists use their explicit marker style and `start` value, and align their
  markers.
- Tables support `colspan`, `rowspan`, column alignment and styles, set widths
  and `%autowidth`.
- Source blocks get syntax colors, line numbers and highlighted lines.
- Figures, tables, examples, listings and source blocks show numbered captions.
- Images show their alt text. Audio and video show as labeled links.
- Links are clickable (OSC 8) and use `hide-uri-scheme`.
- Cross-references resolve by ID or title, and use `xrefstyle` and `reftext`.
- `[index]` sections list the index terms, with `see` and `see-also` entries.
- Recoverable problems show as warnings.
- HTML `[terminal%replay]` blocks play asciicast v2 and v3 recordings.

### Changed

- Output streams as it renders, so memory use stays flat on large documents.
  Conversion is also faster.
- Source blocks with a line over 16 KiB show without highlighting, with a
  warning.
- `line-through` text uses terminal strike-through.
- Building with `terminal-emulator` or `html-terminal` needs Zig 0.16.0.

### Fixed

- Lists, tables, footnotes and body attributes now match Asciidoctor more
  closely.
- Passthroughs, `subs=` and character references now give the same text as
  Asciidoctor. Block `subs=` works in every build.
- Cross-references, section titles and captions now keep their formatting.
- Index terms now handle parentheses, brackets and escapes, and sort by their
  shown labels.
