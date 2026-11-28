# Changelog

All notable changes to `acdc-converters-manpage` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking changes

- Dates now come from `docdate`, not `revdate`, as in Asciidoctor. Set
  `docdate` to pin the printed date. Default dates use `SOURCE_DATE_EPOCH` and
  source file timestamps. `reproducible` and `-a docdate!` leave the date out.
  Asciidoctor restores the default date for `-a docdate!`.

### Added

- Manpages have NOTES and AUTHOR(S) sections, except in embedded output.
- `man source`, `man manual`, `man-source` and `man-manual` are aliases for
  `mansource` and `manmanual`.
- Conversion sets the `manpage` backend, `man` file type and `.man` suffix
  attributes.
- Convention problems and unsupported content show as warnings.
- Sections support `%notitle`. Level 3 and deeper show as bold headings.
- Lists support checklists, `start`, `%reversed` and list styles. Asciidoctor's
  manpage backend uses decimal markers for alphabetic and Roman styles.
- Tables support frame, grid, widths, alignment and spans. Stripes, floats and
  right alignment fall back with a warning.
- Quote and verse blocks show their attribution. `[listing]` and `[source]`
  paragraphs are preformatted.
- Verbatim blocks show callouts as bold `(N)` labels, removing comment prefixes
  or a custom `line-comment`.
- Links use the `.URL` and `.MTO` macros, and apply `hide-uri-scheme`.
- Typography and built-in text roles use roff escapes or fallbacks. Inexact
  roles show a warning.
- Images, icons, audio and video show alt text or links.
- Cross-references use reference text, `xrefstyle` and the `section-refsig`,
  `chapter-refsig`, `appendix-refsig` and `part-refsig` attributes.
- An `[index]` section lists the earlier index terms. This is an acdc extension.
- Bibliography entries show their labels, unlike Asciidoctor's manpage backend.

### Changed

- Output matches Asciidoctor much more closely. Paragraphs use `.sp`,
  monospace uses Courier (`\f(CR`), and level 2 headings keep their case.
- Output streams, keeping memory flat on large documents.
- Block `subs=` works in every build.
- The `embedded` attribute is set only for embedded output.
- Character references show as characters. Asciidoctor's manpage output leaves
  most named ones as text.
- Nested ordered lists default to `a.`, `i.`, `A.` and `I.` by level.
  Asciidoctor's manpage backend uses `1.` at every level.

### Fixed

- Headers, sections, lists, tables, quotes, captions, media, substitutions and
  spacing now match Asciidoctor more closely.
- Nested formatting now restores the outer font. Asciidoctor's manpage output
  doesn't.
- Links, `mailto:` queries and macro escapes now match Asciidoctor more closely.
  Nested links no longer make groff abort.
- Index terms now handle parentheses and brackets, and sort by shown labels.
