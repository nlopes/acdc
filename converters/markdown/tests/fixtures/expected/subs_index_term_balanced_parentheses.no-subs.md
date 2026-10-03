# index-parentheses(1)

2026-10-03

<a id="_name"></a>
## Name

index-parentheses - complete index labels

<a id="_synopsis"></a>
## Synopsis

**index-parentheses**

<a id="_description"></a>
## Description

S01 <a id="_indexterm_0"></a>Term (R) after.

S02 <a id="_indexterm_1"></a>Term (R) after.

S03 <a id="_indexterm_2"></a>Term (R) after.

```
S04 ((Code (R))) after https://example.org/04[Next04]
```

```
S05 ((Code (R))) after https://example.org/05[Next05]
S06 ((fn((x)) tail)) after https://example.org/06[Next06]
S07 ((Code (R)))) after https://example.org/07[Next07]
```

```
S08 ((Late (R))) after https://example.org/08[Next08]
S09 ((pass:r[\(R)] Escaped)) after https://example.org/09[Next09]
S10 ((pass:[(R)] Literal)) after https://example.org/10[Next10]
```

```
S11 ((Attribute {symbol})) after https://example.org/11[Next11]
```

```
S12 ((Late attribute {symbol})) after https://example.org/12[Next12]
S13 ((Late close {close})) after https://example.org/13[Next13]
S14 ((Late open {open}literal)) after https://example.org/14[Next14]
```

```
S15 ((Disabled macro (R))) after.
```

S16 [^1] after.

<a id="_index"></a>
## Index

### F

- Footnote (R) — [Description](#_indexterm_3)

### T

- Term (R) — [Description](#_indexterm_0), [Description (2)](#_indexterm_1), [Description (3)](#_indexterm_2)

[^1]: A <a id="_indexterm_3"></a>Footnote (R) term.
