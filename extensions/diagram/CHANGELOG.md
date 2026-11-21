# Changelog

## [Unreleased]

- Applications can register the `tikz` literal-block style to produce PDF or SVG with a caller-selected LuaLaTeX executable. Use `preamble=true` and a `~~~~` line for package and font declarations. Rendering requires unsafe mode and runs with shell escape disabled.

- Applications can generate SVG or PNG images from `[wavedrom, ,svg]` or `[wavedrom, ,png]` literal blocks by registering the WaveDrom style with a trusted executable and output directory. The generated image respects the block’s `imagesdir`. Rendering requires unsafe mode.
