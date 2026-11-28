# Changelog

All notable changes to `acdc-converters-html` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking changes

- Highlighting reads only `highlight-style` and `highlight-css`. Use them
  instead of `syntect-style` and `syntect-css`, with giallo theme names.
- The `pre-spec-subs` Cargo feature is removed. Remove it from your feature
  lists. Block `subs=` settings apply in every build.

### Added

- `--backend html5s` writes semantic HTML5 with its own stylesheets, like
  [asciidoctor-html5s](https://github.com/jirutka/asciidoctor-html5s). It
  supports `html5s-force-stem-type`, `html5s-image-default-link` and
  `html5s-image-self-link-label`. ([#329])
- `:source-highlighter:` turns on giallo highlighting (`highlighting` feature).
  `:highlight-style:` picks the theme. The default is `github-light`, or
  `github-dark` with `:dark-mode:`. `:highlight-css: class` uses CSS classes,
  written to `acdc-highlight.css` with `:linkcss:`.
- `:dark-mode:` applies a dark color scheme. Standalone output supports a
  `:csp:` Content Security Policy, `:docinfo:` and Asciidoctor's stylesheet
  attributes, such as `:linkcss:`.
- `:sectnums:`, `:sectnumlevels:` and `:partnums:` number sections and book
  parts. `[appendix]` sections get letters. As in Asciidoctor,
  `:partnums: false` turns numbering on, so use `:partnums!:`. ([#342])
- `:acdc-terminal:` shows terminal-like source blocks, such as `console`, as
  terminal previews. `[terminal]` does this for one block.
  `:acdc-terminal-cols:` and `:acdc-terminal-rows:` set the size. This needs
  the `terminal` feature (`html-terminal` in the cli).
- `[terminal%replay]` animates raw ANSI output, which needs `cols` and `rows`.
  `format=asciicast` plays asciicast v2 and v3 recordings.
  `replay-idle-limit-ms` and `replay-duration-ms` set the timing. In `server`
  and `secure` safe modes, previews and replays show as plain listings.
- `:acdc-index:` fills `[index]` sections with a linked index. Without it,
  they stay empty, as in Asciidoctor.
- Icon macros take `set=` or `pack=` to pick a Font Awesome family.
  `:icon-set:` or `:icon-pack:` sets the default, `fa-solid`. ([#360])
- Lists support `[%interactive]`, `start`, `%reversed` and marker styles.
  Tables support spans, alignment, `frame`, `grid`, `stripes` and
  `%autowidth`. `[%collapsible]` examples show as `<details>`.
- Listing and literal blocks accept `subs=`. ([#280])
- `latexmath:[]` and `asciimath:[]` work, and standalone HTML applies
  `eqnums`.
- Cross-references follow `xrefstyle` and signifiers such as `chapter-refsig`,
  and resolve section titles. Links use `window=`, `opts=nofollow` and
  `:hide-uri-scheme:`. ([#359])
- The backend name is `html5` or `html5s`, for conditionals.
- Recoverable problems and unsupported content show as warnings.

### Changed

- HTML output streams to its destination, so memory stays flat on large
  documents.
- Source blocks with a line over 16 KiB skip highlighting, with a warning.
- With `:highlight-css: class`, regenerate the HTML and CSS together when you
  change themes.
- With `:icons: font`, admonitions use Font Awesome 7 classes and the FA 7.2.0
  CDN.

### Fixed

- Substitutions, passthroughs, character references and typography now match
  Asciidoctor more closely, also in highlighted code. ([#291], [#323])
- Cross-references, links, sections, headers, titles and quotes now match
  Asciidoctor more closely. ([#357])
- Lists, callouts, tables, footnotes, index terms, media and `html5s` output
  now match Asciidoctor more closely.
- Remote `data-uri` images need the `network` feature and `allow-uri-read`.
  Image reads have a 20 MiB limit and a 10-second HTTP timeout.
- IDs, roles and titles can no longer inject HTML.
- Keyboard, button and menu macros stay literal unless `experimental` is set,
  as in Asciidoctor.
- The live editor builds for browser WASM again.

[#360]: https://github.com/nlopes/acdc/issues/360
[#357]: https://github.com/nlopes/acdc/issues/357
[#359]: https://github.com/nlopes/acdc/issues/359
[#280]: https://github.com/nlopes/acdc/issues/280
[#342]: https://github.com/nlopes/acdc/issues/342
[#291]: https://github.com/nlopes/acdc/issues/291
[#323]: https://github.com/nlopes/acdc/issues/323
[#329]: https://github.com/nlopes/acdc/issues/329
