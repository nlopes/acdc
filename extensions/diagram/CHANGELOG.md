# Changelog

## [Unreleased]

- WaveDrom and TikZ blocks accept `subs=attributes` to expand document attributes before rendering. With the default settings, braces and backslashes pass through verbatim. Unnamed images use the expanded source for their filenames. Only the attribute step of a substitution group is applied to diagram source.

- WaveDrom and TikZ also render listing blocks delimited by `----`. They accept the same source, attributes, and output formats as literal blocks delimited by `....`.

- Applications can supply separate directories for linked and embedded HTML diagrams
  when the converted file lives outside the source directory. Passing a single
  directory still places all generated assets there. TikZ defaults to SVG for
  the PDF backend, which requires a configured pdf2svg executable to embed the diagram.

- Applications can register the `tikz` literal-block style to produce PDF or SVG with a caller-selected LuaLaTeX executable. Use `preamble=true` and a `~~~~` line for package and font declarations. Rendering requires unsafe mode and runs with shell escape disabled.

- Applications can generate SVG or PNG images from `[wavedrom, ,svg]` or `[wavedrom, ,png]` literal blocks by registering the WaveDrom style with a trusted executable and output directory. The generated image respects the block’s `imagesdir`. Rendering requires unsafe mode.
