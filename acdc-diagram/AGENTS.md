# acdc-diagram — Developer Guide

A Rust port of [asciidoctor-diagram]. The Ruby gem is an Asciidoctor extension
that runs during parsing; acdc's parser has no extension registry, so this crate
is a **pass over the finished AST** instead, run between parsing and conversion.
The reference sources are worth keeping to hand when changing a generator — the
Ruby `lib/asciidoctor-diagram/<tool>/converter.rb` files map one-to-one onto
`src/converters/<tool>.rs`.

[asciidoctor-diagram]: https://github.com/asciidoctor/asciidoctor-diagram

## Layout

| Module | Responsibility |
|---|---|
| `processor.rs` | Walks the AST, recognises diagram blocks and macros, replaces them |
| `render.rs` | Format choice, cache lookup, generation, image attributes |
| `source.rs` | `DiagramSource`: the code plus attribute lookup, checksums, tool discovery |
| `cache.rs` | The JSON sidecar next to each generated image |
| `converters/` | One module per tool: formats, options, command line |
| `generate.rs` | The four input/output shapes a tool invocation takes |
| `image/` | Measuring PNG/GIF and normalising SVG |
| `attrlist.rs` | Parsing the block-macro form out of a paragraph |

## Where a diagram comes from

Two forms reach this crate, and they arrive very differently:

- **Block style** — `[graphviz, target, format]` over a listing or literal
  block. The parser has already parsed the attribute list; positional values
  come back in order from `BlockMetadata::positional_values()`.
- **Block macro** — `graphviz::chart.dot[…]`. The parser does not know the name,
  so it produces a plain paragraph. `attrlist.rs` recognises the macro in that
  paragraph's text and parses the attribute list itself.

The inline-macro form is **not** supported: by the time this pass runs, the
parser has turned it into text with no structure left to recognise.

## Adding a tool

1. Write `src/converters/<tool>.rs` implementing `DiagramConverter`. Pick the
   `generate::` helper matching how the tool takes input and emits output.
2. Register it in `converters/mod.rs`: a `lookup` arm **and** a `NAMES` entry.
   The unit tests there check the two agree.
3. Follow the Ruby converter for flag names and defaults — the option keys are
   compared against cached runs, so renaming one invalidates existing caches.

Tools are never bundled. Locate them with `source.find_command`, which checks
document attributes before `PATH`; use `find_command_opt` where a missing tool
is a routine branch rather than an error. Java-based tools go through
`converters/java.rs`.

## Testing without the tools

Unit tests here cover parsing, path resolution and image measurement — nothing
that shells out. End-to-end checking needs the real tools, and the upstream
examples are the quickest exercise:

```sh
cp asciidoctor-diagram/examples/{features,design}.adoc "$(mktemp -d)"
acdc convert <dir>/features.adoc <dir>/design.adoc
```

`asciidoctor -r asciidoctor-diagram` over the same files is the reference. Two
differences are expected and not bugs: file names derived from a diagram's
content use SHA-256 rather than MD5, and image dimensions differ when the local
tool is a different version from the one the gem bundles.

`ACDC_LOG=acdc_diagram=debug` logs every tool lookup, every command line, and
whether each diagram was generated or reused from cache.

## Caching

Each generated image has a `<image>.cache` JSON sidecar recording the diagram's
digest, the converter options, and the measured size. Regeneration happens when
the image is missing, the digest differs, the source file of a block macro is
newer, or the options differ. **Anything that affects the generated image must be
in one of those** — an option read directly inside `convert` instead of being
returned from `collect_options` will not invalidate the cache when it changes.
