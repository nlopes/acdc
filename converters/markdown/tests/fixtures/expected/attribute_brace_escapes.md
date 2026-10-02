# brace-escapes(1)

2026-10-02

<a id="_name"></a>
## NAME

brace-escapes - preserve escaped attribute references

<a id="_description"></a>
## DESCRIPTION

E01 {value} / {value} / {value} / Expanded.

E02 {missing} / {missing} / {missing}.

E03 {empty} / {empty} / {empty} / .

E04 {markup} / {markup} / {markup}.

E05 α {value} β {value} γ.

E06 {value}{value}Expanded / x{value}y.

E07 \\{value} / \\{value} / \\\\{value}.

E08 {value\\\\} / \\{value\\\\} / {value\\\\\\}.

E09 \\{bad name} / {bad name\\} / \\{bad name\\}.

E10 \\{} / {\\} / {unfinished\\.

E11 **{value} and {value} and {value}**.

E12 `{value}` / `{value}` / `{value}`.

E13 [{value} / {value} / {value}](https://example.org).

E14 [^n].

E15 {value} / {value} / {value}.

E16 \\{value} / {value\\} / \\{value\\}.

E17 \{value} / {value\} / \{value\}.

E18 {value} / {value} / {value} / Expanded.

E19 \{value} / {value\} / \{value\} / {value}.

E21 \{bad name} / {bad name\} / \{bad name\} / \{}.

E22 {\_value\_} / {\_value\_} / {\_value\_}.

E20 {value} / {value} / Later.

[^n]: {value} / {value} / {value}
