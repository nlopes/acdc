# Changelog

## [Unreleased]

- Render `[plantuml]` blocks and `plantuml::chart.puml[]` files with `--plantuml PATH`. Choose PNG, SVG, ASCII `atxt`, or Unicode `txt` and `utxt`. Local includes resolve from the diagram directory. Configure Graphviz with `--graphviz-dot` or use the built-in Smetana layout.

- Render `[penrose]` blocks and `penrose::chart.substance[]` files with `--roger PATH`. Provide `domain_file` and `style_file` relative to the diagram source. Use `variation` to choose a layout.

- Render `[erd]` blocks and `erd::chart.erd[]` files with `--erd PATH`. Configure Graphviz with `--graphviz-dot PATH`. The ERD renderer produces DOT and Graphviz writes the final PNG or SVG.

- Render `[lilypond]` blocks and `lilypond::chart.ly[]` files with `--lilypond PATH`. The output is a cropped PNG or PDF score. Set `resolution` to select the raster resolution.

- Render `[oxdraw]` blocks and `oxdraw::chart.mmd[]` files with `--oxdraw PATH`. SVG is the default. Use `background` and `scale` to change the output.

- Render `[mermaid]` blocks and `mermaid::chart.mmd[]` files with `--mmdc PATH`. Choose PNG or SVG and set `theme`, `scale`, or a local `config` file. For SVGs with a nonzero viewBox origin, acdc uses the declared width and height. Asciidoctor Diagram 3.2.1 subtracts the origin from those dimensions.

- Render `[bpmn]` blocks and `bpmn::chart.bpmn[]` files with `--bpmn-js PATH`. Choose PNG (default), SVG, PDF, or JPEG. Set `width` and `height` to select the canvas size.

- Render `[umlet]` blocks and `umlet::chart.uxf[]` files with `--umlet PATH`. Choose SVG (default), PNG, PDF, or GIF using a UMLet command-line launcher. Supports the SVG 1.0 declaration emitted by the standalone UMLet distribution.

- Render `[gnuplot]` blocks and `gnuplot::chart.gp[]` files with `--gnuplot PATH`. Choose PNG, SVG, GIF, or a literal text plot. Set `width` and `height` together to select a terminal size.

- Render `[tape]` blocks and `tape::chart.tape[]` files with `--vhs PATH`. GIF is the default. Use `format=svg` for an SVG recording. Tape scripts run in unsafe mode.

- Render `[ditaa]` blocks and `ditaa::chart.ditaa[]` files with `--ditaa PATH`. Choose PNG (default), SVG, or literal text. Set `round-corners=true`, `shadows=false`, or `scale` to change the drawing.

- Render `[diagrams]` blocks and `diagrams::chart.py[]` files with `--diagrams-python PATH`. Render Python diagrams as PNG (default), SVG, or PDF with an interpreter that has the diagrams package installed.

- Render `[syntrax]` blocks and `syntrax::chart.spec[]` files with `--syntrax PATH`. Choose PNG or SVG. Use `heading`, `scale`, `style-file`, and `transparent=true` to customize a railroad diagram.

- Render `[svgbob]` blocks and `svgbob::chart.bob[]` files with `--svgbob PATH`. Choose SVG or literal text. The SVG renderer accepts `font-family`, `font-size`, `stroke-width`, and `scale`.

- Render `[a2s]` blocks and `a2s::chart.txt[]` files with `--a2s PATH`. SVG is the default. Use `format=txt` for the original text, `scalex` and `scaley` for dimensions, or `noblur=true` for crisp edges.

- Render `[pintora]` blocks and `pintora::chart.pintora[]` files with `--pintora PATH`. Choose PNG (default) or SVG. Set `theme`, `width`, `background-color`, or `pixel-ratio` to customize the result.

- Render `[symbolator]` blocks and `symbolator::chart.vhd[]` files with `--symbolator PATH`. Choose PNG (default), PDF, or SVG.

- Render `[shaape]` blocks and `shaape::chart.shaape[]` files with `--shaape PATH`. PNG is the default. Select SVG with `format=svg`.

- Render `[d2]` blocks and `d2::chart.d2[]` files with `--d2 PATH`. SVG is the default. PNG and PDF require the corresponding D2 browser support. Set `theme`, `layout`, or `sketch` to change the drawing.

- Render `[graphviz_py]` blocks and `graphviz_py::chart.dot[]` files with `--graphviz-py PATH`. Choose PNG (default), PDF, or SVG. Set `layout` and `argument` to pass a layout engine and a preprocessor argument.

- Render `[dbml]` blocks and `dbml::chart.dbml[]` files with `--dbml-renderer PATH`. Output is SVG.

- Render `[smcat]` blocks and `smcat::chart.smcat[]` files with `--smcat PATH`. Set `direction` or `engine` to choose the SVG layout.

- Render `[nomnoml]` blocks and `nomnoml::chart.nomnoml[]` files with `--nomnoml PATH`. Output is SVG.

- Render `[rackdiag]` blocks and `rackdiag::chart.diag[]` files with `--rackdiag PATH`. PNG is the default. PDF, SVG, and a custom font file via `fontpath` are also supported.

- Render `[packetdiag]` blocks and `packetdiag::chart.diag[]` files with `--packetdiag PATH`. PNG is the default. PDF, SVG, and a custom font file via `fontpath` are also supported.

- Render `[nwdiag]` blocks and `nwdiag::chart.diag[]` files with `--nwdiag PATH`. PNG is the default. PDF, SVG, and a custom font file via `fontpath` are also supported.

- Render `[actdiag]` blocks and `actdiag::chart.diag[]` files with `--actdiag PATH`. PNG is the default. PDF, SVG, and a custom font file via `fontpath` are also supported.

- Render `[seqdiag]` blocks and `seqdiag::chart.diag[]` files with `--seqdiag PATH`. PNG is the default. PDF, SVG, and a custom font file via `fontpath` are also supported.

- Render `[blockdiag]` blocks and `blockdiag::chart.diag[]` files with `--blockdiag PATH`. PNG is the default. PDF, SVG, and a custom font file via `fontpath` are also supported.

- Render packet and binary layouts as SVG from `[bytefield]` blocks or `bytefield::layout.edn[]` file macros with `--extension diagram --bytefield-svg PATH`.

- Render GoAT ASCII diagrams from `[goat]` blocks and `goat::chart.txt[]` file macros as SVG with `--extension diagram --goat PATH`. Set `svg-color-dark-scheme` and `svg-color-light-scheme` to choose the colors.

- Render Pikchr source from `[pikchr]` blocks and `pikchr::chart.pikchr[]` file macros as SVG with `--extension diagram --pikchr PATH`. Syntax errors report the renderer’s diagnostic and preserve the previous image.

- Render PIC source from `[dpic]` blocks or `dpic::chart.pic[]` file macros as SVG with `--extension diagram --dpic PATH`. acdc accepts both bare source and pictures already enclosed in `.PS` and `.PE`. Asciidoctor Diagram 3.2.1 can append a duplicate closing delimiter to a complete picture.

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
