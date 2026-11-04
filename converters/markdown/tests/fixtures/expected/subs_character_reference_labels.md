# reference-labels(1)

2026-01-01

## Table of Contents

- [Name](#_name)
- [Description](#_description)
- [Heading &#169;](#_heading)
- [Index](#_index)

<a id="_name"></a>
## Name

reference-labels - resolved references in display text

<a id="_description"></a>
## Description

<a id="_heading"></a>
## Heading &#169;

P01 Code `&` and `©` and `&amp;`.

P02 Buffered <sup>[Label&#169;](other.md)</sup> and <sub>[Low&#169;](other.md)</sub>.

<strong>Audio &#169; and &amp;#169;</strong>

[Audio: Audio © and &amp;#169;](sample.ogg)

<strong>Video &#169; and &amp;#169;</strong>

[Video: Video © and &amp;#169;](sample.mp4)

![Alt © and &amp;#169;](missing-reference-image.png)

<strong>Prefix &amp;#169; Image caption</strong>

P03 Inline image: ![Alt © and &amp;#169;](missing-inline-reference.png).

|  |  |
| :--- | :--- |
| Interpreted &#169; | <code>`Literal &#169;.`</code> |

<!-- Warning: description lists not natively supported in Markdown, using regular list -->
- **Term &#169;**
  Description.
- **Term &#46;**
  Description with a full stop.

P04 Passthrough &#169; and &amp;#169;.

```
P05 Nested © and &#169;.
```

P06 <a id="_indexterm_0"></a>Zebra and <a id="_indexterm_1"></a>&#65;lpha and <a id="_indexterm_2"></a>&copy; and <a id="_indexterm_3"></a>&amp;copy;.

P07 ![Alt](missing-tooltip.png "Tip © and &amp;#169;") and [Label](https://example.org "Tip © and &amp;#169;").

<a id="_index"></a>
## Index

### @

- &amp;copy; — [Heading ©](#_indexterm_3)
- &copy; — [Heading ©](#_indexterm_2)

### A

- &#65;lpha — [Heading ©](#_indexterm_1)

### Z

- Zebra — [Heading ©](#_indexterm_0)
