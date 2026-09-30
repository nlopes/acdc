# escaped-macros(1)

<a id="_name"></a>
## Name

escaped-macros - literal macro labels

<a id="_description"></a>
## Description

E01 indexterm2:\[One \\\] term\]

E02 indexterm:\[Hidden \\\\\] term\]

E03 ((Short \\\] term))

E04 https://example.org\[URL \\\] text\]

E05 mailto:user@example.org\[Mail \\\] text\]

E06 xref:target\[Cross \\\] text\]

E07 footnote:\[Note \\\] text\]

E08 anchor:target\[Anchor \\\] text\]

E09 indexterm2:\[Open \\\[ text\]

E10 indexterm2:\[**Bold** -- Attribute \\\] text\]

E11 indexterm2:\[Repeated \\\\\\\] term\]

E12 Ordinary \\\] and \\\[ and \\\\\] text.

E13 ((Nested **bold** \\\] term))

E14 <a id="_indexterm_0"></a>Active \] term

E15 [Active \] label](https://example.org)

E16 [^1] [^2]

E17 <a id="_indexterm_1"></a>Short active \\\] term

E18 [Repeated \\\] label](https://example.org)

E19 [<a id="_indexterm_2"></a>Nested active \] term](https://example.org)

E20 \[\[escaped-target\]\] and <<escaped-target>>.

E21 \*literal\* \\\] text and <em>raw HTML</em>.

> E22 <a id="_indexterm_3"></a>Quoted \\\] term and \*literal\* \\\] text.

<a id="target"></a>
E23 [Cross \] label](#target) and [Repeated \\\] label](#target).

E24 [^3] and [Opening \\\[ label](#target).

<a id="_index"></a>
## Index

### A

- Active \] term — [Description](#_indexterm_0)

### N

- Nested active \\\] term — [Description](#_indexterm_2)

### Q

- Quoted \\\] term — [Description](#_indexterm_3)

### S

- Short active \\\] term — [Description](#_indexterm_1)

[^1]: Active \] note
[^2]: Repeated \\\] note
[^3]: Opening \\\[ note
