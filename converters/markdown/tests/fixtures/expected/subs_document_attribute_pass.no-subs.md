# attribute-pass(1)

2026-10-01

<a id="_name"></a>
## Name

attribute-pass - passthrough attribute values

<a id="_description"></a>
## Description

```
A01 {plain}
A02 {literal-ref}
A03 {escaped-ref}
A04 Before {empty} after.
A05 {partial}
A06 {escaped}
A07 {brackets}
A08 {alias}
A09 {nested}
A10 {line}
A11 {unicode}
A15 {entity}
```

P01 Generated \| {name} \| Generated / {name} \| pass:\[Nested\]

P02 **{name}** \| *pass:\[Nested\]* \| `{name}`

P03 prefix pass:\[Embedded\] \| \\pass:\[Escaped\]

P04 first second \| café α

P05 Before  after.

P07 &#169;

P08 \\{name} \| **\\{name}**

```
A12 {plain}
A13 {alias}
```

P06 Body

```
A14 {plain}
```
