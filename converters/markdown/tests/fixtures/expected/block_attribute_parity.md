# Block Attribute Parity

<a id="_delimiters_and_slots"></a>
## Delimiters and slots

> Balanced brackets.
> — A \[x\] B, <cite>Notes</cite>

> Unmatched opening bracket.
> — A \[x, <cite>Notes</cite>

> Extra closing bracket.
> — A \] B, <cite>Notes</cite>

> Unquoted comma.
> — https://example.com\[Someone, <cite>Part 1\]</cite>

> Quoted comma.
> — https://example.com\[Someone, Part 1\], <cite>Final</cite>

> Quote-delimited slot.
> — Quoted author, <cite>Adjacent citation</cite>

> Empty slot.
> <cite>Empty-slot citation</cite>

> Named slot.
> <cite>Named-slot citation</cite>

> Named author.
> — Named Author, <cite>Positional Citation</cite>

> Named citation.
> — Positional Author, <cite>Named Citation</cite>

> Both fields named.
> — Named Author, <cite>Named Citation</cite>

> Unset None value.
> <cite>Literal Value</cite>

> Quoted None value.
> — None, <cite>Quoted Literal</cite>

> Positional None value.
> — None, <cite>Positional Literal</cite>

> Spaced equals signs.
> — Comma, Author, <cite>Work</cite>

> Expanded slots.
> — Ada Lovelace, <cite>Notes</cite>

<a id="_stacked_lists"></a>
## Stacked lists

> Named-only stack.
> — Original Author, <cite>Named Citation</cite>

> Positional overlay stack.
> — Replacement Author, <cite>Original Citation</cite>

> Style replacement stack.
> — Replacement Poet, <cite>Replacement Poem</cite>

<a id="_substitutions"></a>
## Substitutions

> Unquoted values.
> — Ada \*Lovelace\*, <cite>\`Notes\`</cite>

> Double-quoted values.
> — Grace \*Hopper\*, <cite>\`Compiler\`</cite>

> Single-quoted formatting.
> — Margaret **Hamilton**, <cite>`Apollo`</cite>

> Escaped single quote.
> — A'B, <cite>Notes</cite>

> Unquoted macros.
> — https://example.com\[Literal Author\], <cite>https://example.org\[Literal Work\]</cite>

> Single-quoted macros.
> — [Linked Author](https://example.com), <cite>[Linked Work](https://example.org)</cite>

> Named substitutions.
> — Named **Author**, <cite>`Named Work`</cite>

<a id="_quote_and_verse_forms"></a>
## Quote and verse forms

Styled quote paragraph.
— Paragraph **Author**, <cite>`Paragraph Work`</cite>

Citation-only quote paragraph.
<cite>Paragraph citation only</cite>

Styled verse paragraph.
— Paragraph **Poet**, <cite>`Paragraph Poem`</cite>

> Delimited verse block.
> — Block **Poet**, <cite>`Block Poem`</cite>

<a id="_styles_and_context_slots"></a>
## Styles and context slots

<a id="styled-quote"></a>
> Adjacent shorthand.
> — Ada

Spaces disable shorthand.

Bracketed unknown style.

\[\[not an anchor\]\]
Double-bracket anchor syntax with spaces remains text.

<a id="only-id"></a>
Shorthand-only metadata.

```rust
fn main() {}
```

```
slot three is not a language
```

```
empty language slot
```

puts "styled source paragraph"

<!-- Warning: STEM/math blocks not natively supported in Markdown, skipping (use LaTeX-enabled renderer) -->
