# profile-edges(1)

<a id="_name"></a>
## NAME

profile-edges - retained attribute profile edge cases

<a id="_description"></a>
## DESCRIPTION

before prefix **Éva** / *Italic* suffix after / *Literal* / **Éva** / https://example.org[Protected] before **Éva** / *Italic* after / {bold} / https://example.org[Literal] / **Éva** / https://example.org[Literal] / **Éva**.

 / **{missing}** / **{name}** / **Early**.

[Site](https://example.org) / [^1] / [^2] / [^n] / [^n] / [^n].

pass:q,c\[\*Escaped\*\] / pass:a,c\[{bold}\] / pass:typo\[\*Unknown\*\].

[^1]: Anonymous note.
[^2]: Anonymous note.
[^n]: Named note.
