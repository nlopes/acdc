# Changelog

## [Unreleased]

- HTML output uses the generated SVG’s width and height for WaveDrom and TikZ. A numeric `scale` multiplies those defaults, and an explicit `width` or `height` overrides that dimension. acdc uses CSS pixel conversions for inches, centimeters, millimeters, and picas, and takes `viewBox` extents independently of their origin. These cases can differ from Asciidoctor Diagram.

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
