# attribute-pass(1)

2026-10-01

<a id="_name"></a>
## Name

attribute-pass - passthrough attribute values

<a id="_description"></a>
## Description

```
A01 Generated
A02 {name}
A03 \{name}
A04 Before  after.
A05 prefix pass:[Embedded]
A06 \pass:[Escaped]
A07 a[b\]c]d
A08 Generated / {name}
A09 pass:[Nested]
A10 first second
A11 café α
A15 &#169;
```

P01 Generated \| {name} \| Generated / {name} \| pass:\[Nested\]

P02 **{name}** \| *pass:\[Nested\]* \| `{name}`

P03 prefix pass:\[Embedded\] \| \\pass:\[Escaped\]

P04 first second \| café α

P05 Before  after.

P07 &#169;

P08 \\{name} \| **\\{name}**

```
A12 Body
A13 Generated / {name}
```

P06 Body

```
A14 {plain}
```
