# Changelog

## [Unreleased]

- Render message sequence charts from `[msc]` blocks or `msc::chart.msc[svg]` with `--extension diagram --mscgen PATH`. PNG is the default. Use `font=` or `msc-font` to select a PNG font.

- Render a local DOT file with `graphviz::chart.dot[svg,alt="*Graph*"]`. File macros use the configured Graphviz executable and share the document’s include byte limit.

- Render Graphviz listing and literal blocks with the registered `dot` executable (`--extension diagram --graphviz-dot PATH` in the CLI). PNG is the default. SVG, PDF, and the `layout` setting are supported.

- On Unix, only the current user can access the temporary directory containing a diagram’s source and intermediate files, even when the output directory is shared.

- Use `diagram-svg-type=inline` to embed SVG markup or `interactive` to render an HTML object. The default `static` links an image. WaveDrom and TikZ each accept a renderer-specific setting and a per-block override.

- HTML output uses the generated image’s width and height for WaveDrom SVG and PNG diagrams and for TikZ SVG diagrams. A numeric `scale` multiplies those defaults, and an explicit `width` or `height` overrides that dimension. acdc uses CSS pixel conversions for inches, centimeters, millimeters, and picas, and takes `viewBox` extents independently of their origin. These cases can differ from Asciidoctor Diagram.

- Set `imagesoutdir` to stage WaveDrom and TikZ files in a local relative directory. Linked images use `imagesdir`, and embedded images read the staged files. acdc rejects absolute directories and parent traversal.

- WaveDrom and TikZ can write named diagrams into subdirectories of `imagesdir`. For example, `[wavedrom,boards/clock,svg]` produces and links to `boards/clock.svg`.

- WaveDrom and TikZ images use the authored `alt`, including `alt=""` for decorative diagrams. The default label comes from the target name, or `Diagram` for unnamed images.

- Failed WaveDrom and TikZ diagrams now produce a located warning and show the error and source in a listing. As in Asciidoctor Diagram, `diagram-on-error` defaults to `log`. Set it to `abort` to stop parsing.

- WaveDrom and TikZ blocks accept `subs=attributes` to expand document attributes before rendering. With the default settings, braces and backslashes pass through verbatim. Unnamed images use the expanded source for their filenames. Only the attribute step of a substitution group is applied to diagram source.

- WaveDrom and TikZ also render listing blocks delimited by `----`. They accept the same source, attributes, and output formats as literal blocks delimited by `....`.

- Set `diagram-format` to choose a document-wide output format for WaveDrom and TikZ. A renderer-specific format or an explicit block format takes precedence.

- Applications can supply separate directories for linked and embedded HTML diagrams
  when the converted file lives outside the source directory. Passing a single
  directory still places all generated assets there. TikZ defaults to SVG for
  the PDF backend, which requires a configured pdf2svg executable to embed the diagram.

- Applications can register the `tikz` literal-block style to produce PDF or SVG with a caller-selected LuaLaTeX executable. Use `preamble=true` and a `~~~~` line for package and font declarations. Rendering requires unsafe mode and runs with shell escape disabled.

- Applications can generate SVG or PNG images from `[wavedrom, ,svg]` or `[wavedrom, ,png]` literal blocks by registering the WaveDrom style with a trusted executable and output directory. The generated image respects the block’s `imagesdir`. Rendering requires unsafe mode.
