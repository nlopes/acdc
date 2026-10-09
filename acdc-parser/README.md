# acdc-parser

AsciiDoc parser written in Rust. It parses source into an abstract syntax tree
(AST), collects warnings, and can serialize the result as JSON based on the draft
AsciiDoc Abstract Semantic Graph (ASG). Includes, conditionals, and document
attributes are processed before block and inline parsing.

The implementation here follows from:

* [Language Lexicon](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/modules/ROOT/pages/lexicon.adoc): nomenclature of elements
* [Language Outline](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/outline.adoc): behaviour/layout
* [Asciidoctor Language Documentation](https://docs.asciidoctor.org/asciidoc/latest): behaviour/layout

These are compatibility references, not a claim of complete conformance.

Requires Rust 1.88 or later.

## Quick start

```rust
use acdc_parser::{Options, parse};

let parsed = parse("= Example\n\nHello, *world*.\n", &Options::default())?;
let document = parsed.document();
assert!(!document.blocks.is_empty());

for warning in parsed.warnings() {
    eprintln!("{warning}");
}
# Ok::<(), acdc_parser::Error>(())
```

Use `parse_file` for a file, `parse_from_reader` for a reader, or `parse_inline`
for inline content. `ParseResult` owns the parsed document and its backing text;
keep it alive while reading `document()`. A successful parse can contain warnings
and recovered content. See [Source text and diagnostics](#source-text-and-diagnostics).

## Cargo features

| Feature | Default | Effect |
| --- | --- | --- |
| `pre-spec-subs` | On | Supports experimental block `subs=` settings and their AST types. |
| `setext` | Off | Compiles support for legacy underlined headings; enable it with `OptionsBuilder::with_setext()`. |
| `network` | Off | Compiles built-in HTTP(S) include support outside bare WebAssembly. Caller permission and safe mode still apply. |

With `pre-spec-subs` disabled, explicit `subs=` settings are ignored with a
source-recovery warning. Inline attribute substitution remains available.

## Migrating from 0.10

The parser now defaults to `SafeMode::Secure`, matching the Asciidoctor API.
This applies to `Options::default()`, the options builder, and all parse functions.
Include directives become links. The parser does not read their targets.
`parse_file` still reads the entry file that the caller selects.

To enable includes in a trusted project, select Safe and set the include base:

```rust
use acdc_parser::{Options, SafeMode};

let options = Options::builder()
    .with_safe_mode(SafeMode::Safe)
    .with_base_dir("/workspace/docs")
    .build()?;
# Ok::<(), acdc_parser::Error>(())
```

Select `SafeMode::Unsafe` to allow local includes outside the base directory.
`IncludeLoader::System` remains the default loader.
A custom provider does not change the safe mode.
For remote includes, the caller must also set `allow-uri-read`.

The CLI keeps its existing defaults. `convert`, `lint`, and `inspect` use Unsafe.
`execute` uses Safe. Use `--safe-mode secure` to prevent include reads.

Secure controls include reads and access to local path attributes.
It does not sanitize rendered HTML or sandbox commands.
Safe and Server restrict include paths to the base directory, but do not resolve symlinks.
A symlink can point outside that directory.
The 10 MiB limit applies to selected text from each include.
It does not limit total expanded input or total parser memory use.

## Migrating from 0.9

Attribute configuration now validates inputs and returns `Result`. Pass an
iterator instead of building a mutable `DocumentAttributes` map:

```rust
use acdc_parser::{DocumentAttributeAssignment, Options, parse};

let options = Options::builder()
    .with_attribute("max-include-depth", "8")
    .with_attribute("sectnums", true)
    .with_default_attribute("imagesdir", "images")
    .build()?;
let parsed = parse(":project: acdc\n\nContent.\n", &options)?;
let attributes = &parsed.document().attributes;
assert_eq!(attributes.get("project").and_then(|v| v.as_str()), Some("acdc"));

if let Some(DocumentAttributeAssignment::Set(value)) = attributes.assignment("project") {
    assert_eq!(value.text(), Some("acdc"));
}
# Ok::<(), acdc_parser::Error>(())
```

Use `Options::document_attributes()` to read configured values and
`Options::into_builder()` to change configuration. `with_attributes()` replaces
earlier application inputs; `with_defaults()` replaces earlier defaults.
`DocumentAttributes::into_inputs()` exports values for fresh configuration.
`Options::with_document_attributes()` instead reuses an existing snapshot without
changing its assignment precedence.

`Document::attributes` stops at the end of the header. Read later
`Block::DocumentAttribute` nodes in source order and match `assignment()` on
`Set(value)` or `Unset`. Attribute maps no longer expose mutation methods or
`get_string()`. Use `as_str()` for text, `as_integer()` for validated numbers,
`is_presence()` for a value set without text, and `text()` or `write_text()` for
the attribute's written representation.

Other API and JSON changes:

| In 0.9 | In 0.10 |
| --- | --- |
| `Position` line and column as `usize` | `u32`; use `Position::from_line_col()` for `usize` inputs. |
| `SourceLocation::positioning` and `Positioning` | `SourceLocation::location`; use `at_position()` or `at_location()`. |
| `Location::shift`, `shift_inline`, `shift_line_column` | Removed. Locations identify original source; use `ParseResult::source_location()` to resolve files. |
| `TocEntry::numbered` and `style` | `kind: SectionKind` and `number()`. |
| Index labels as strings | Inline-node slices from `term()`, `secondary()`, and `tertiary()`; JSON labels are arrays. `InlineMacro::IndexTerm` contains a `Box`. |
| Only `Explicit` and `Modifiers` substitution specifications | Also handle `SubstitutionSpec::Source`; parsed lists and JSON retain source entries. |
| `macros_disabled()` and `attributes_disabled()` | `BlockMetadata::uses_substitution()` or `SubstitutionSpec::resolve()`, with the block's defaults. |
| Unconditional substitution configuration types | `SubstitutionSpec`, `SubstitutionOp`, and `BlockMetadata::substitutions` require `pre-spec-subs`. |
| `inlines_to_string()` | Removed; walk inline nodes with the text policy your application needs. |

`Position` JSON adds a `file` array for included content. Index relationships add
an optional `relationship` field. Document-attribute JSON keeps its existing shape.
The `ParseResult` ownership model is unchanged from 0.9.

<details>
<summary>Features supported</summary>

* [x] Document Headers
    * [x] Author parsing (first/middle/last name, email)
    * [x] Revision info
* [x] Section
    * [x] ATX-style (`=` markers)
    * [x] Setext-style (underlined, optional feature)
    * [x] Discrete headers
* [x] Delimited Block
    * [x] Comment
    * [x] Example
    * [x] Listing
    * [x] Literal
    * [x] Open
    * [x] Sidebar
    * [x] Table
    * [x] Pass
    * [x] Quote
    * [x] Verse
* [x] Paragraph
    * [x] Bold (constrained & unconstrained)
    * [x] Italic (constrained & unconstrained)
    * [x] Monospace (constrained & unconstrained)
    * [x] Literal Monospace
    * [x] Highlight (constrained & unconstrained)
    * [x] Subscript / Superscript
    * [x] Curved quotes and apostrophes
    * [x] Passthrough (inline and macro)
* [x] Image (block and inline)
* [x] Video
* [x] Audio
* [x] Lists
    * [x] Ordered
    * [x] Unordered
    * [x] Description Lists
    * [x] Checklist items
    * [x] List continuation (`+`)
    * [x] Ancestor list continuation
* [x] Thematic Break
* [x] Page Break
* [x] Tables
    * [x] Header and footer rows
    * [x] Column formatting (`cols` attribute with alignment, width, style)
    * [x] Cell spanning (colspan `2+|`, rowspan `.2+|`)
    * [x] Cell duplication (`3*|`)
    * [x] Cell-level alignment (`<|`, `^|`, `>|`, `.<|`, `.^|`, `.>|`)
    * [x] Cell-level style (`s|`, `e|`, `m|`, `a|`, etc.)
    * [x] CSV, PSV, DSV formats
    * [x] AsciiDoc content in cells (`a` style)
    * [x] Nested tables (`!===` delimiter in AsciiDoc cells)
* [x] Admonition
* [x] Anchors
    * [x] Block anchors (`[[id]]`)
    * [x] Inline anchors (`[#id]`)
    * [x] Bibliography anchors (`[[[anchor]]]`, `[[[anchor,label]]]`)
* [x] Attributes
    * [x] Document attributes
    * [x] Attribute references
    * [x] `:leveloffset:` for includes
    * [x] Substitution control (`subs` with `+quotes`, `-callouts` modifiers)
* [x] Titles
* [x] Footnotes (including inline content)
* [x] Cross References
    * [x] xref macro
    * [x] Shorthand notation (`<<id>>`)
    * [x] Attribute substitution in targets and text
* [x] Links and URLs
    * [x] Link macro
    * [x] URL detection and autolinks
    * [x] Autolink syntax (`<https://...>`)
    * [x] Mailto macro
* [x] Inline Macros
    * [x] Button
    * [x] Keyboard
    * [x] Menu
    * [x] Icon
    * [x] Pass
* [x] Stem/Math
    * [x] `stem:[formula]` inline
    * [x] `latexmath:[...]` and `asciimath:[...]`
    * [x] Stem blocks
* [x] Index terms
    * [x] Visible `((term))`
    * [x] Concealed `(((term,secondary,tertiary)))`
* [x] Callouts
    * [x] Callout markers in source blocks (`<1>`, `<2>`, etc.)
    * [x] Callout lists
* [x] Table of contents (`toc::[]` macro)
* [x] Includes
    * [x] Offsets
    * [x] Tagged regions (`tag=`, `tags=`, wildcards `*`/`**`, negation `!tag`)
    * [x] `:leveloffset:` adjustment
* [x] Conditionals
    * [x] ifdef
    * [x] ifndef
    * [x] ifeval
* [x] Line breaks (+)

</details>

## Parser options

* **Safe mode** - `Safe`, `Secure`, `Server`, `Unsafe`
* **Strict mode** - Rejects manpage titles that do not use `name(volume)`;
  other recoverable warnings remain warnings.
* **Base directory** - Entry-input include resolution through
  `Options::builder().with_base_dir(path)`
* **Document attributes** - `with_attribute` and `with_attributes` set application
  overrides; `with_default_attribute` and `with_defaults` set values the document
  can replace. Built-in read-only and API-only names remain protected.
* **Setext headers** - Optional feature flag for two-line underlined headers
* **Manpage doctype** - `doctype=manpage` with derived attributes

Document configuration accepts standard Rust iterators. Application overrides
and document-overridable defaults use the same input values; callers do not need
to track parser locks or assignment policy.

```rust
use acdc_parser::{Options, parse};

let options = Options::builder()
    .with_attributes([("max-include-depth", "064")])
    .with_defaults([("imagesdir", "images")])
    .build()?;
let parsed = parse("= Example\n\nContent.\n", &options)?;
let depth = parsed.document().attributes.get("max-include-depth").unwrap();
assert_eq!(depth.as_integer(), Some(64));
assert_eq!(depth.text(), Some("064"));
# Ok::<(), acdc_parser::Error>(())
```

`with_attributes` replaces earlier application inputs; `with_attribute` sets one
input. `with_defaults` replaces earlier defaults. Use `false` or `()` to unset an
attribute, including one supplied by defaults.

## Parsing and conversion

With the experimental `pre-spec-subs` feature, `subs="attributes+"` moves an
already-enabled attribute stage to the front. Groups such as `normal+` move
their members together in group order. Modifiers apply left to right; append
syntax such as `+attributes` leaves an existing stage in place, matching
Asciidoctor. Use `-attributes,+attributes` to move it to the end.

A plain first entry replaces block defaults, including when later entries have
modifiers: `subs="quotes,+attributes"` enables only quotes and attributes.
A modifier first entry retains block defaults: `subs="+quotes,attributes"`
adds to those defaults. `none,+quotes` enables only quotes. These rules match
Asciidoctor; later entries do not reset the starting list.

Removing a group also disables its members during parsing. For example,
`-normal,attributes+` enables attributes without restoring formatting or macros;
`-verbatim,+callouts` restores callouts without restoring special-character
substitution. Asciidoctor leaves these callout markers literal without the
special-character stage. acdc retains source-based recognition of enabled callouts
under its SDR-5 parsing/conversion policy; SDR-5 does not specify this experimental
substitution combination.

AST and JSON substitution metadata retain the list entries. For example,
`verbatim,-macros` stays `["verbatim", "-macros"]`. Groups, aliases, duplicates
and modifiers are preserved; group expansion and removal happen only when the
effective stages are resolved. Metadata parsing still expands attribute references
in the list and trims whitespace around each entry.

Standalone `subs="none"` and `subs=""` retain `[]`, meaning no substitutions.
Absent substitution metadata uses block defaults. Mixed lists retain `none`:
`none,+quotes` starts empty and enables only quotes; removing `none` would
incorrectly restore block defaults.

Nonempty parsed lists use `SubstitutionSpec::Source`, except standalone `none`,
which retains `Explicit([])` like an empty value. Consumers that match this enum must
handle that variant; `resolve()` and the existing typed `Explicit`/`Modifiers`
constructors remain available.

acdc follows [SDR-5's separation of parsing and conversion](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/sdrs/sdr-005-formal-grammar-for-inline-syntax.adoc).
The parser expands source references and constructs inline nodes. Converters
produce output markup and apply the escaping required by their output format.
The parser may use explicit `backend` conditions, but must not render an attribute
to HTML or roff to decide its value.

Custom substitution order also retains structured formatting and links when
escaping runs last, where Asciidoctor can display generated markup as text.
In source blocks with `subs="quotes,+attributes"`, acdc PDF keeps formatted text;
Asciidoctor PDF displays generated tags when special-character substitution is
absent. These retained differences keep the parsed structure independent of the
output backend.

Constrained formatting uses source punctuation as its boundary. For example,
`>*Bold*` produces a literal `>` followed by bold text; each converter escapes
the `>` for its output format. Asciidoctor's default substitutions escape it
before formatting and therefore leave the asterisks literal. In `pass:q[...]`,
both processors recognize formatting directly after a raw tag. Literal entity
references such as `&gt;*Bold*` retain their semicolon boundary and need doubled
formatting marks.

This follows [SDR-5's decision on special characters](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/sdrs/sdr-005-formal-grammar-for-inline-syntax.adoc#special-characters),
which assigns their encoding to converters after parsing. SDR-5 does not
prescribe this exact example; the behavior follows acdc's boundary rule and
deferred escaping.

Formatted attributes retain AsciiDoc source text
and supported inline parsing profiles. For example, `:value: pass:q[*Bold*]`
retains `*Bold*` for `text()`, JSON, conditions, and include paths, while its
references can produce bold inline content. Asciidoctor instead stores generated
backend markup at definition time; acdc deliberately does not adopt that behavior.

References selected by `a` freeze at definition time. Both `a,q` and `q,a` parse
that frozen source at use. Aliases retain their profiles and protect raw text.
An explicit `a` list imports profiles while keeping surrounding text literal;
an explicit structural list controls the complete expanded source.
Unused values register no macros; anonymous footnotes register separately at each
use, while named notes share their ID. Lists such as `q,c` or `m,c` that need
rendered markup, unknown substitution names, and text-only `a`/`c` combinations
that reference profiled values stay literal with a structured parser warning.
Profiles apply typography during conversion, including code and backtick spans.
For example, `:value: pass:r[(C)]` retains `(C)` for text reads and renders `©`
at each use. `:value: pass:r[\(C)]` retains `\(C)` for text reads and renders
literal `(C)`, including code that enables replacements. Asciidoctor removes
that escape during assignment and can later render `©` in ordinary prose and
replacements-enabled code. acdc's behavior follows its source-first policy under
SDR-5; SDR-5 does not prescribe these custom-list semantics.
Raw-arrow replacement and some block-level code replacement paths remain
separate compatibility limits.

Visible index shorthand preserves balanced internal parentheses. For example,
`((Term (R)))` contains the complete label `Term (R)` and displays `Term ®`
when replacements are enabled. `((fn((x)) tail))` keeps its nested pairs;
`((word)))` leaves an extra `)` outside the term. Unbalanced literal parentheses
retain the existing compatibility delimiter rules. Unlike Asciidoctor's legacy
macro stage, acdc also keeps complete labels when replacements are disabled
or run after macros. This source-based delimiter policy keeps parsing separate
from conversion under SDR-5; SDR-5 does not specify these exact index rules.
Converters retain neighboring text across visible index labels,
hidden index terms, and ordinary anchors. With replacements enabled,
`prefixindexterm2:[--]tail` displays `prefix—tail` in HTML, PDF, manpages, and
terminal output, including highlighted code. The same context handles a split
dash pair in `prefix-indexterm2:[-]tail`, consumes one space on each side of
`prefix indexterm2:[--] tail`, and renders `Saindexterm2:[m]'s` as `Sam’s`.
Repeated spaced pairs share their source delimiters: `-- -- --` replaces the
first and third pairs, matching Asciidoctor. Escaped dashes and disabled
replacements remain literal; registration-time catalog text stays unchanged.
This matches Asciidoctor HTML and manpages when replacements run after macros
and there are no intervening generated anchors. Asciidoctor's default order
leaves this example literal, and Asciidoctor PDF retains more macro boundaries.
acdc deliberately uses parsed word context during conversion under its selected
SDR-5 policy; SDR-5 does not prescribe these exact typography rules. Formatting
and link boundaries still separate replacement runs. Passthroughs retain their
own substitution profiles: their text can supply word context, but replacements
cannot consume spaces, escapes, or dash halves from another profile. For example,
`prefix indexterm2:[pass:r[--]] tail` retains `--`; Asciidoctor converts the
isolated passthrough to a spaced em dash before inserting it.

Existing `pre-spec-subs` compatibility behavior is not complete SDR-5 conformance.
SDR-5 leaves the exact mapping of custom `pass:` lists unresolved; support for
common profiles is an acdc policy. Earlier text-only escaping and ordinary
attribute-introduced formatting remain separate migration work. See the
[architecture document](../ARCHITECTURE.adoc) for the boundary and migration scope.

## Source text and diagnostics

`Paragraph::source_text()` and `DelimitedBlock::source_text()` return the body
after preprocessing and before inline substitutions. Preprocessing normalizes
line endings, removes trailing whitespace, and processes includes and conditionals.
Delimited text excludes metadata and delimiters but retains the newline before
the closing delimiter. Programmatically constructed blocks return `None` unless
they have retained text. This text does not change JSON or semantic equality.

Use `Block::metadata()` and `Block::location()` without matching each block
variant. `BlockMetadata::uses_substitution()` checks enabled substitutions against
the defaults for that block. `substitute_attributes()` performs one text-only
attribute pass; it does not apply other inline substitutions or shell quoting.

`ParseResult::source_location()` resolves an AST location to its original file,
including partial and nested includes. Each location boundary has its own include
chain. A span across files resolves to the file at its start. Use
`Location::byte_len()` for an inclusive byte length; it returns `None` across files.
For reindented includes, line and column are original-source coordinates but byte
offsets remain in preprocessed coordinates. Do not use them to slice the original
file.

`ParseResult::source_recovery()` returns the first warning about omitted or
recovered content, including disabled includes, incomplete tables, and unmatched
block delimiters. It remains available after `take_warnings()` and excludes
presentation warnings. Applications that need complete input must check it before
acting on recovered content. Rendering can continue with the recovered document.

Parser tracing records sizes, counts, positions, and diagnostic categories.
It does not dump document text, options, AST nodes, paths, or URLs.
Read full diagnostics from `ParseResult::warnings()`,
`ParseInlineResult::warnings()`, and returned errors.
These diagnostics can contain source text and paths.

## Intrinsic document attributes

acdc initializes the intrinsic backend, input, time, safe-mode, and environment
attributes before preprocessing. Later document assignments apply in source order.
File input derives `docdir`, `docfile`,
`docfilesuffix`, `docname`, and the document timestamp from the entry file. Server
and Secure modes conceal the directory and home path. `SOURCE_DATE_EPOCH` makes
both the document and conversion timestamps deterministic and formats them in UTC.

Cross-references do not load source files. After explicit includes are expanded,
references to the current file or fully included sources resolve within the same
document. Partial includes remain external, as in Asciidoctor. `CrossReference::target`
contains the effective fragment ID, and `target_is_local` distinguishes it from an
external resource even when the ID contains punctuation. An empty local target
addresses the document top; the empty entry in `Document::references` supplies its
reference text. Converters must check the local flag before treating a target as
a filename. When changing a destination, update both fields.

The parser records the effective attributes at the end of the document header in
`Document::attributes`. Later accepted set and unset entries appear as ordered
`Block::DocumentAttribute` nodes and do not change that header snapshot. Each node
exposes `assignment()`, which returns `DocumentAttributeAssignment::Set` or `Unset`.
A set value provides `as_str()`, `as_integer()`, and `is_presence()` for semantic
reads, and `text()` or `write_text()` to retain its written representation.
`get()` borrows a value without copying it. `assignment()` and `assignments()` on
the header snapshot also expose explicit unsets. Rejected and invalid entries do
not become semantic AST events.

Custom converters can use `acdc-converters-core::TraversalContext` with the shared
visitor to read attributes at each body position. The context applies events in
source order and restores parent attributes after nested AsciiDoc table cells.
Parser consumers that do not use the visitor can interpret the ordered set/unset
events directly; the parser exposes no mutable traversal or precedence state.

The header column in the Asciidoctor document-attribute reference identifies where a
feature normally reads its setup value. It is not a general ban on later assignments.
acdc accepts supported body assignments in source order, while setup-only consumers
continue to use the header snapshot. Caller values, API-only attributes, and read-only
intrinsic values remain protected. A source `backend` value can be visible as document
text, but it does not change the converter that the API or CLI already selected.
`outdir` and `outfile` are output metadata exposed by the converter result; they are
not available to source substitution.

## Recoverable document warnings

A section directly nested in a `[bibliography]` section remains in the AST and
adds a located warning to `ParseResult::warnings()`. Parsing and conversion
continue. Asciidoctor keeps the same content but reports the condition at error
severity.

## Include targets

Include targets may contain internal spaces. Leading or trailing ASCII whitespace
still makes the line invalid as an include directive, so we leave it for ordinary
document parsing.

Local targets with spaces resolve normally below Secure mode. Targets beginning with
an ASCII URI scheme are classified before local paths, so unsupported schemes never
fall through to filesystem access. A scheme must contain at least two characters,
preserving Asciidoctor's Windows drive-path disambiguation. Scheme names are
case-insensitive. In Secure mode, and for URI targets without caller-supplied URI
authority, we preserve the complete target in the same link fallback as Asciidoctor.
For authorized HTTP(S) reads, the target follows `ureq`'s URI rules: a raw space takes
the located unreadable-URI recovery path. Authorized targets with other schemes
produce the same unresolved-URI recovery without a transport attempt.

## Partial includes

The `lines`, `tag`, and `tags` attributes select content from the original target
before that content is processed. An include or conditional outside the selected
lines does not run and does not produce a warning.

If a directive has more than one kind of selector, `lines` takes precedence over
`tag`, and `tag` takes precedence over `tags`. For repeated attributes of the same
kind, the last value wins. Line selections are sorted and deduplicated, and a
negative range end means the end of the file.

Tag selection supports nested tags, repeated tag names, `*` and `**`, and negated
selectors. Missing tags and malformed selected tag boundaries produce located
warnings and parsing continues.

## Include depth

Built-in includes have a trusted `max-include-depth` attribute that defaults to `64`
and is visible as `{max-include-depth}` in the parsed document. The fallback
participates in attribute lookup, membership, effective iteration, substitution, and
conditionals. On the borrowed value, `as_integer()` returns its numeric value, `text()` returns the
original spelling of an explicit value, and `write_text()` writes either that spelling
or the formatted default. Use `is_explicit()` to test whether an assignment exists,
including an unset assignment. The default is not serialized unless the caller supplied
a value. Set it through the parser options when a different limit is needed:

```rust
let options = acdc_parser::Options::builder()
    .with_attribute("max-include-depth", "8")
    .build()?;
# Ok::<(), acdc_parser::Error>(())
```

The entry document does not count toward the limit; each currently open included file
counts as one level. A value of `0` disables built-in include processing and leaves
each directive as literal content with a located source-recovery warning.
Boolean `true` also selects zero; use a decimal string for a numeric limit.
A string value can have
surrounding Unicode whitespace, but the complete trimmed value must be a non-negative
ASCII decimal integer. Malformed, empty, decimal-fraction, and negative values return
an `InvalidDocumentAttribute` configuration error when options are built. The original
spelling of a valid value remains visible as the document attribute, and very large
positive values saturate safely without overflow. At a positive limit, the blocked
directive is preserved, a located diagnostic is added to `ParseResult::warnings()`,
and parsing continues. Only an exact `include::` directive is processed; block macro
names that merely begin with `include` remain ordinary content without include
diagnostics. Declarations in document content are consumed but cannot change or unset
the trusted value and do not appear as `Block::DocumentAttribute` nodes in the AST.

## Include indentation

The `indent` attribute on an include accepts a non-negative integer from `0` through
`4096`. A larger value returns a located error before the target is read, preventing
one directive from requesting an unbounded space prefix for every non-empty included
line.
Malformed and negative values remain invalid.

The `4096` cap is an intentional acdc security policy, not a requirement of the
AsciiDoc language. It deliberately creates an acceptance divergence from
asciidoctor, which coerces the value using Ruby's `to_i` and does not impose a
documented or implemented maximum before allocating the indentation prefix.

## Table limits

Tables accept at most 100 logical columns and 1,000 rows, bounding an accepted table
to 100,000 materialized cells. The column bound also applies to `cols` multipliers,
cell duplication counts, and column spans; row spans are bounded by the row limit.
Larger requests and oversized CSV, TSV, DSV, or PSV dimensions return a located
parse error before unbounded expansion.

These are fixed internal safety limits. They cannot be changed through parser options
or document attributes. This intentionally diverges from asciidoctor, which has no
equivalent table dimension cap.

## Include loading

`SafeMode` sets processing restrictions.
`Options::include_loader` selects where include content comes from.
The default is `IncludeLoader::System` for string, reader, and file input.
The default safe mode is Secure, so the parser does not read include targets.
Select a lower mode to enable include reads.

- `IncludeLoader::Disabled` preserves literal include directives in modes below
  Secure without reading their targets. Skipped includes produce source recovery diagnostics.
  `parse_file` still reads the entry file selected by the caller.
- `IncludeLoader::System` reads local files.
  With `network`, it also reads permitted HTTP(S) targets outside bare WebAssembly.
- `IncludeLoader::custom(provider)` reads sources through an
  `IncludeSourceProvider`, such as unsaved editor buffers or a virtual filesystem.

```rust
use acdc_parser::{IncludeLoader, Options, SafeMode, parse};

let options = Options::builder()
    .with_safe_mode(SafeMode::Server)
    .with_include_loader(IncludeLoader::Disabled)
    .build()?;
let document = parse("include::part.adoc[]", &options)?;
assert!(document.source_recovery().is_some());
# Ok::<(), acdc_parser::Error>(())
```

Providers receive resolved paths or absolute HTTP(S) URIs and return byte streams.
The parser applies safe-mode restrictions before it calls a provider.
It also handles attribute substitution, decoding, line/tag selection, nested
includes, diagnostics, and source locations.
Custom URI providers do not require the `network` feature, but still require
caller-supplied `allow-uri-read`. Secure mode never calls a provider and always
produces the usual link fallback, including when the loader is `Disabled`.

The explicit `Disabled` option differs from Asciidoctor's Secure-mode link fallback.
At equivalent safe modes, `System` retains the existing Asciidoctor include
behavior. Both APIs default to Secure; both conversion CLIs default to Unsafe.

## Include base directory

When include loading is enabled, string and reader input resolve relative includes
from the current working directory. File input uses the entry file's parent directory.
`Options::builder().with_base_dir(path)` overrides the include base for all input types.
`parse_file` still reads the entry file from the path that the caller supplies.

The effective base resolves includes in the entry input. Once a file is included,
nested relative includes resolve from the directory containing that file. In `Safe`
and `Server` modes the effective base also remains the local-include boundary.

## Local include confinement

In `Safe` and `Server` modes, the effective include base directory is the local
include boundary. For file input it defaults to the entry document's directory; for
string and reader input it defaults to the current working directory.

For example, assume the entry document is `/workspace/docs/main.adoc`, so the
boundary is `/workspace/docs`:

| Directive location | Include target | Path opened | Result |
| --- | --- | --- | --- |
| `/workspace/docs/main.adoc` | `chapters/intro.adoc` | `/workspace/docs/chapters/intro.adoc` | No warning |
| `/workspace/docs/main.adoc` | `../shared.adoc` | `/workspace/docs/shared.adoc` | The `..` that would leave the boundary is discarded, and a warning is emitted |
| `/workspace/docs/main.adoc` | `/workspace/docs/appendix.adoc` | `/workspace/docs/appendix.adoc` | No warning because the absolute target is already inside the boundary |
| `/workspace/docs/main.adoc` | `/tmp/shared.adoc` | `/workspace/docs/tmp/shared.adoc` | The outside absolute path is moved beneath the boundary, and a warning is emitted |
| `/workspace/docs/chapters/part.adoc` | `../../shared.adoc` | `/workspace/docs/shared.adoc` | The first `..` reaches the boundary, the second is discarded, and a warning is emitted |

Nested includes resolve from the directory of the file that contains the directive,
but continue to use `/workspace/docs` as their boundary. With `opts=optional`, the
target is transformed first, the recovery warning is retained, and a missing
transformed file is then skipped without a missing-file warning.

`Unsafe` mode does not apply these transformations: from
`/workspace/docs/main.adoc`, `../shared.adoc` attempts to read
`/workspace/shared.adoc`, and `/tmp/shared.adoc` remains `/tmp/shared.adoc`.

The boundary checks the path as written but does not resolve symlinks. If
`/workspace/docs/linked.adoc` points to `/private/secret.adoc`, including
`linked.adoc` reads `/private/secret.adoc` without a boundary warning. These
transformations match asciidoctor; they are not strict symlink containment.

## Remote includes

The parser checks for a URI before it treats a target as a local path.
URI reads need a safe mode below `Secure` and the caller's `allow-uri-read` attribute.
A document cannot grant itself permission to read a URI.

`IncludeLoader::System` supports HTTP(S) and needs the `network` feature.
Custom providers can supply permitted HTTP(S) sources without that feature.

On bare WebAssembly targets such as `wasm32-unknown-unknown`, `network` does not provide an HTTP client.
If the system loader attempts an HTTP(S) include, the parser reports a warning at the directive and leaves it unresolved.
Fetch the content in the host application before parsing, then supply it through a custom `IncludeSourceProvider`.
The same permissions and safe modes apply.

`IncludeLoader::System` uses `ureq` for HTTP framing, redirects, TLS, and timeouts.
Asciidoctor uses Ruby's OpenURI.
We do not try to match all OpenURI behavior.

If a source cannot open or a read fails before selection completes, the parser issues a warning at the include directive.
It keeps the unresolved directive, discards partial text, and continues parsing.
Encoding errors in required text and provider errors marked `Fatal` stop parsing.

When `compat-mode` is set, denied URI includes omit the `include` role from their
fallback link, matching Asciidoctor.

## Include source size limit

Each include can select up to 10 MiB of normalized UTF-8 text, including newlines.
The parser checks this limit before and after `indent=`, before it processes nested directives.
If the selected text exceeds the limit, parsing fails with `Error::IncludeSourceTooLarge`.
A document attribute cannot change the limit.
The limit does not apply to the entry document.

Use `lines=`, `tag=`, or `tags=` to select a small part of a larger source.
For example, `include::large.log[lines=100..120]` selects lines 100 to 120.
If these lines fit within the limit, the source file can be larger than 10 MiB.
The parser decodes the source in chunks and discards skipped lines.

A finite `lines=` selection stops at its highest requested line, as Asciidoctor does for local files.
Encoding errors or read failures after that line do not affect the result.
Full includes, open-ended ranges such as `lines=100..-1`, and tag selections read to the end of the source.
A tag can occur again later in the source.
The text limit does not limit scan time or the total text from nested or repeated includes.

Line selections remove trailing whitespace from each line independently.
This includes Unicode whitespace such as U+2003, which Asciidoctor preserves.
Later text cannot change the selected text or its source locations.
A selected blank final line counts the same whether more lines follow or not.
Earlier versions normalized the whole source, which made this count depend on later lines.

The built-in HTTP(S) loader has a separate 10 MiB limit for bytes read after decompression.
This includes bytes read ahead of the selection.
When a finite line selection is complete, the loader closes the response without reading or checking the rest.
Thus, a small selection near the start of a larger response can succeed.
A selection near the end can exceed the transfer limit before the reader reaches it.
Custom providers control their own transport limits, but the parser still limits their selected text.

Tag names, active tag state, and diagnostics also have fixed 10 MiB metadata limits.
These limits prevent unbounded tag data when the reader scans a large source.
If tag data exceeds a limit, parsing fails with `Error::IncludeSelectionTooComplex`.

These resource limits deliberately differ from Asciidoctor.

## Include encodings

acdc decodes include bytes before it selects lines or tags.
A byte order mark (BOM) identifies UTF-8, UTF-16LE, or UTF-16BE.
Explicit UTF-16LE and UTF-16BE labels also work without a BOM.
The parser accepts WHATWG single-byte encoding labels and aliases through `encoding_rs` and converts their text to UTF-8.
If an encoding label is unknown, acdc uses BOM detection or UTF-8, as Asciidoctor does.

For local files with an explicit UTF-16 encoding, invalid text produces an unreadable-file warning at the include directive.
The parser keeps the unresolved directive and continues.
Invalid UTF-8 without transcoding stops parsing if it occurs in the text that the selector must read.

A finite `lines=` selection ignores invalid bytes after its last requested line, even in the same read buffer.
acdc keeps the same decoder across reads, so BOM detection also works with `lines=` and `tag=`.
Asciidoctor 2.0.26 uses a separate line reader for selections.
Its UTF-16 behavior differs when only a BOM specifies the encoding.

## Parser fixtures

We use two fixture styles, and they are intentionally different.

### `fixtures/tests`

These are general parser and AST fixtures. The fixture test in `src/lib.rs`
discovers `.adoc` files recursively and compares each with its adjacent `.json`.
Supporting files belong in `fixtures/tests/includes/`, which discovery excludes.
The generator accepts only top-level fixture filenames.

After approval to update expected output, regenerate only the affected fixtures
from the workspace root and review the diff:

```console
cargo run -p acdc-parser --example generate_parser_fixtures --all-features -- example.adoc
```

### `fixtures/preprocessor`

These are focused scenarios for includes, conditionals, encoding, warnings, and
source mapping. They are not discovered automatically and usually do not have a
JSON companion. A Rust test must open a root fixture directly; any supporting
fixtures are then reached through ordinary `include::` directives.

For example:

```text
Rust test
  └─ main.adoc
       └─ include::target.adoc[]
            └─ include::inner.adoc[]
```

Adding a file to `fixtures/preprocessor` does not add test coverage by itself. Either
reference it from a Rust test or include it from a root fixture that a test already
opens. Use this style when the assertion needs more than a serialized AST, such as
exact warnings, missing-file behavior, or file, line, column, and include-chain
attribution.

Run `acdc lint` over new or changed AsciiDoc fixtures before handing them off.

## Deliberate divergences from asciidoctor

acdc uses the [AsciiDoc Language draft specification](https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/)
and [Asciidoctor](https://asciidoctor.org) as references. Some behavior deliberately
differs. These choices do not imply complete draft-specification conformance.

* **Nested inline markup**: Formatting, links, footnotes, and index labels retain
  a nested AST. acdc keeps complete labels where Asciidoctor's substitution order
  can truncate them or produce crossed markup. Formatting boundaries use source
  characters before output escaping. Balanced parentheses inside visible index
  shorthand remain part of the label, even when replacement substitutions are off.
* **Attribute values**: Supported `pass:` values retain AsciiDoc text, not generated
  HTML or roff. Output-dependent substitution lists stay literal with a warning.
  Escaped attribute names remain protected from formatting. Unused values and
  discarded titles do not register footnotes, anchors, or index terms.
* **Quoted conditions**: `ifeval` respects operators inside quoted strings and
  removes both quote delimiters. Asciidoctor 2.0.26 can split at an operator inside
  a quoted value or retain its closing quote.
* **Callouts**: Enabled callouts are recognized without special-character
  substitution. Nested callout lists keep their outer validation context.
* **References**: Link IDs enter the reference catalog. Anchors retain labels in
  contexts where Asciidoctor can fall back to `[id]`; valid IDs can also include
  non-ASCII symbols. Title-based references in `compat-mode` remain unresolved and
  warn. Cross-references never trigger file reads.
* **Recovery warnings**: Disabled includes and ignored `subs=` settings report
  source recovery. Conflicting named footnotes warn and keep the first body.
  Nested bibliography and index sections remain in the AST with warnings, while
  Asciidoctor reports their structural restriction at error severity.
* **Protected attributes**: Document entries cannot change read-only or API-only
  attributes, including derived names that Asciidoctor does not consistently lock.
  An explicitly selected backend name remains visible even if it is not a known
  Asciidoctor backend; selecting an output converter is the application's job.
* **Unicode boundaries**: Formatting and passthrough boundaries use Rust's Unicode
  tables. Characters added in Unicode 17 can differ on Ruby versions with newer
  tables.

* **Uppercase checklist marker**: acdc treats `[X]` as checked, alongside `[x]`
  and `[*]`. Asciidoctor leaves `[X]` in the item's text. This acceptance
  difference is an intentional acdc extension.

* **Include source limit**: Each include can select up to 10 MiB of UTF-8 text.
  The built-in HTTP(S) loader also limits bytes read after decompression to 10 MiB.
  See [Include source size limit](#include-source-size-limit).
* **Remote include transport**: We use `ureq` and don't try to reproduce the URI
  transport behavior that Asciidoctor's Ruby implementation inherits from OpenURI.
  URI classification uses the portable ASCII scheme syntax and Asciidoctor's
  two-character Windows-path carve-out. MRI Asciidoctor also accepts non-ASCII
  scheme-like prefixes as a side effect of Ruby's Unicode-aware regular expressions;
  acdc treats those prefixes as paths. Only HTTP(S) is fetched; FTP and other ASCII
  schemes take URI recovery and never local-file handling.
  If a request or read fails before selection completes, the parser discards partial text and keeps the unresolved directive.
  See [Remote includes](#remote-includes).
* **Include encoding labels and selection**: acdc uses the WHATWG-oriented
  `encoding_rs` label set rather than Ruby's exact aliases.
  The parser decodes bytes in chunks. Finite line selections ignore encoding errors after the last requested line.
  See [Include encodings](#include-encodings).
* **Include indentation limit**: As an intentional acdc security policy,
  `include::file[indent=N]` accepts at most `4096` spaces instead of allowing an
  unbounded allocation. AsciiDoc does not require this cap, and asciidoctor does not
  impose it. See [Include indentation](#include-indentation).
* **Table dimension limits**: Tables accept at most 100 logical columns and 1,000
  rows, with fixed internal limits that cannot be raised by parser options or
  document attributes. See [Table limits](#table-limits).
* **Boolean include-depth value**: Boolean `true` as `max-include-depth` selects
  zero and disables includes with a warning. Asciidoctor raises an error for this
  input. Use a decimal string for a numeric limit; see
  [Include depth](#include-depth).
* **Strict include-depth validation**: acdc trims surrounding Unicode whitespace and
  requires the complete `max-include-depth` string to be a non-negative ASCII decimal
  integer. Malformed, empty, fractional, and negative values return a structured
  configuration error. Asciidoctor 2.0.26 instead accepts a
  leading signed decimal prefix, so a value such as `8notes` silently becomes `8`.
  The strict rule follows the documented [integer (≥ 0) domain](https://docs.asciidoctor.org/asciidoc/latest/attributes/document-attributes-ref/#security-attributes)
  and is an intentional compatibility divergence. See [Include depth](#include-depth).
* **Symmetric escape of constrained markers**: `\*foo\*`, `\_foo\_`, `` \`foo\` ``, `\#foo\#` all emit the literal marker pair (`*foo*`, `_foo_`, etc.). asciidoctor strips only the opening backslash and leaves the trailing `\` in the output. The draft spec's backslash-escaping section (`spec/outline.adoc`) states: "a backslash in front of a reserved markup character will be removed, regardless of whether the text would have been interpreted or not" — acdc follows that rule symmetrically.

## See also

- [CHANGELOG](CHANGELOG.md) for detailed feature history and version notes
- [Architecture](../ARCHITECTURE.adoc) for parsing and conversion boundaries
