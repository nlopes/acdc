# Changelog

All notable changes to `acdc-diagram` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Diagrams written in a plain-text diagram language are generated during
  conversion and replaced by the image they produce, so every backend renders
  them as ordinary images. A diagram is written either as a block style or as a
  block macro:

  ```asciidoc
  [graphviz, ethane, svg]
  ----
  graph ethane { C_0 -- H_0; }
  ----

  plantuml::activity.puml[format=svg, align=center]
  ```

  The first positional attribute names the generated image, the second picks the
  format; both can also be given as `target=` and `format=`. Every other
  attribute (`align`, `role`, `width`, `link`, …) is carried over to the image,
  and the block's title, id and roles are kept.

- Recognised diagram types: `a2s`, `actdiag`, `blockdiag`, `bpmn`, `bytefield`,
  `d2`, `dbml`, `diagrams`, `ditaa`, `dpic`, `erd`, `gnuplot`, `goat`,
  `graphviz`, `graphviz_py`, `lilypond`, `meme`, `mermaid`, `msc`, `nomnoml`,
  `nwdiag`, `oxdraw`, `packetdiag`, `penrose`, `pikchr`, `pintora`, `plantuml`,
  `rackdiag`, `salt`, `seqdiag`, `shaape`, `smcat`, `svgbob`, `symbolator`,
  `syntrax`, `tape`, `tikz`, `umlet`, `vega`, `vegalite`, `wavedrom`. Each needs
  its tool installed; none is bundled.

- `tikz` blocks choose the command that typesets them with `command=`. The
  value is the command's name, taken as written and not checked against any
  list: `command=xelatex` runs `xelatex`, `command=lualatex` runs `lualatex`,
  and `command=my-latex-wrapper` runs that. `:tikz-command:` sets it for the
  whole document, and the default is `pdflatex`. The command is then located
  like every other diagram tool, so a document attribute of the same name
  (`:xelatex: /opt/texlive/bin/xelatex`) pins a particular build and a value
  holding a path separator is used as the path. Changing the command
  regenerates the diagram rather than serving the cached image.

  Whatever is named has to accept `pdflatex`'s arguments and leave its PDF next
  to the input, which the `TeX` engines do; a tool with its own command line,
  such as `tectonic`, is run and reports its own error. This attribute has no
  counterpart in `asciidoctor-diagram`, which always runs `pdflatex`.

- Generated images are cached. A diagram is only re-rendered when its code, its
  attributes, or the tool options behind it change — or, for a block macro, when
  the file it points at is newer than the image. A document whose diagrams are
  all cached runs no external tools at all. `[graphviz%nocache]` opts a block
  out, `%cache-images` keeps the image in the cache directory and links it into
  the output tree, and `:diagram-cachedir:` moves the cache away from its default
  `.asciidoctor/diagram`.

- Output formats follow the tool: `svg`, `png`, `pdf`, `gif` and `jpeg` become
  image blocks, while `txt`, `atxt` and `utxt` become literal blocks holding the
  generated ASCII or Unicode art. For an HTML backend, PDF is used only when a
  diagram type offers nothing else, and the measured image size is written onto
  the image as `width`/`height` (scaled by `scale=` for tools that do not scale
  themselves). Generated SVG gains `xmlns`, `preserveAspectRatio` and a
  `viewBox` when the tool omitted them; `%nooptimise` keeps the tool's comments.

- Where a tool lives can be pinned per document: `:dot: /opt/graphviz/bin/dot`
  and the like, checked before `PATH`. The Java-based tools (`plantuml`,
  `ditaa`, `umlet`, `syntrax`) prefer a native launcher and otherwise run
  `java -jar` against an archive named by an attribute (`:plantumljar:`,
  `:ditaajar:`, …) or by the matching `DIAGRAM_*_CLASSPATH` environment
  variable.

- A diagram that fails to generate leaves its source visible in the output and
  reports the reason as a warning — a missing tool names the attribute that can
  point at it. `:diagram-on-error: abort` turns the first failure into a hard
  error instead.

- `[tape]` diagrams run the shell commands in their body, so they are generated
  only in unsafe mode.

### Divergences from `asciidoctor-diagram`

- Cached images live exactly where the gem puts them — the image in
  `imagesoutdir`/`imagesdir`, a `<image>.cache` sidecar under
  `.asciidoctor/diagram` (or `:diagram-cachedir:`), and, with `%cache-images`,
  the image in the cache directory hard-linked into the output tree. The
  sidecar's contents are not interchangeable with the gem's, though: file names
  derived from a diagram's content use a truncated SHA-256 digest
  (`diag-plantuml-sha256-…`) where the gem uses MD5. Running both tools over one
  directory regenerates every diagram that has no explicit `target`, and each
  tool ignores the other's sidecars.
- The inline-macro form (`graphviz:chart.dot[]` inside a sentence) is not
  supported; the block and block-macro forms are.
- Rendering is always local: the gem's `:diagram-server-url:` delegation to a
  remote rendering service has no equivalent.
- `barcode` and `structurizr` are not available.
- Mermaid is driven through `mmdc`; the gem's fallback onto the pre-`mmdc`
  `mermaid` binary under PhantomJS is not carried over.
- `PlantUML` preprocessing is skipped for diagrams that use none of the
  preprocessor's syntax, which avoids a JVM start-up per block. `preprocess=false`
  still disables it outright.
