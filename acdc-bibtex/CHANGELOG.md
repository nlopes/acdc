# Changelog

All notable changes to `acdc-bibtex` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- First release: BibTeX citations and bibliographies, a port of
  `asciidoctor-bibtex`. `cite:[key]` renders a parenthetical citation,
  `citenp:[key]` one that reads as part of the sentence, `bibitem:[key]` a
  rendered entry inline, and `bibliography::[]` the reference list. Several
  keys can share one macro (`cite:[a,b]`), each can carry a locator
  (`cite:[Lane12(59)]`), and text before the bracket becomes pretext
  (`cite:See[Lane12]`). Every citation links to its entry in the list.
- Citations are resolved for every backend — HTML, PDF, manpage, markdown and
  terminal — because the pass runs over the parsed document rather than
  inside one converter.
- Citations are found wherever prose can appear: paragraphs, block and section
  titles, every kind of list, table cells, verse and quote blocks, admonitions,
  and inside footnotes and inline formatting.
- A relative `bibtex-file` is read as written, from the directory the command
  was run in, which is how `asciidoctor-bibtex` reads it — so
  `:bibtex-file: papers/refs.bib` builds from the directory above `papers`. A
  path that is not there is then looked for beside the document, so the same
  document also builds from elsewhere; the original simply fails in that case.
  When the document names no file, the directory holding it is searched first,
  then the one the command was run in.
- Configuration through the document attributes `bibtex-file`, `bibtex-style`,
  `bibtex-order`, `bibtex-format`, `bibtex-throw` and
  `bibtex-citation-template`, with the same meanings and defaults as the
  original extension. `bibliography::refs.bib[apa]` also names the database and
  style, for a document that sets neither attribute.
- The styles `ieee`, `apa` and `chicago-author-date` (also reachable as
  `chicago`, and `harvard` for `apa`), rendered natively. Entry types
  `article`, `book`, `incollection`, `inbook`, `inproceedings`, `conference`,
  `phdthesis`, `mastersthesis`, `techreport`, `manual`, `proceedings`,
  `misc` and `unpublished` each take the shape their style gives them, and a
  type this crate does not know is placed by the fields it carries.
- `bibtex-format` set to `bibtex`, `latex` or `biblatex` emits `\cite{…}`,
  `\parencite{…}` and `\textcite{…}` passthroughs instead of formatted text,
  for a document that will be finished by a LaTeX toolchain.
- LaTeX in a `.bib` file is decoded: accents and diacritics, special letters,
  dashes and quotation marks, `\url{…}`, and the markup macros whose argument
  is the text. `@string` definitions and `#` concatenation are expanded.
- A DOI or a URL in an entry becomes a link the reader can follow.
- An unknown citation key is reported as a warning and rendered as written;
  `:bibtex-throw: true` makes it an error that stops the conversion instead.

### Notes on divergence from `asciidoctor-bibtex`

- The original renders through citeproc and the CSL style files, which cover
  thousands of styles. This crate implements the three its documentation and
  tests are written against, and reports any other style name before falling
  back to `ieee`.
- `bibtex-locale` is accepted and ignored: the built-in styles render in
  English.
- A directory holding more than one `.bib` file is left for the document to
  resolve with `bibtex-file`, rather than one being chosen arbitrarily.
