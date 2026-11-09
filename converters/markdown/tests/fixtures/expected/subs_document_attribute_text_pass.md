# attribute-text-pass(1)

2026-10-01

<a id="_name"></a>
## Name

attribute-text-pass - text-only attribute substitutions

<a id="_description"></a>
## Description

P01 Early \*Bold\*

P02 &lt;x&gt; &amp; {name} \*Bold\*

P03 &lt;raw&gt; &amp;

P04 <raw> &

P05 Early &lt;x&gt; &amp;

P06 {name} &

P07 {name} {name} {name} \\{name}

P08 pass:\[Early\]

P09 Before  after.

P10 Before {not-set} after.

P11 Early a\\\]b

P12 prefix &lt;x&gt; &amp; {name} \*Bold\* / Early \*Bold\*

P13 &lt;ordinary&gt; &amp;

P14 &lt;ordinary&gt; &amp;

P15 &lt;ordinary&gt; &amp;

P16 <raw> &

P17 &lt;x&gt; &amp; {name} \| &lt;x&gt; &amp; {name}

P18 &#169; &amp;

P19 café &lt;α&gt; &amp;

P20 <>&

P21 &lt;&amp;&gt;

P22 pass:c\[Early\]

```
C01 Early *Bold*
C02 <x> & {name} *Bold*
C03 <raw> &
C05 Early <x> &
C07 {name} {name} {name} \{name}
C08 pass:[Early]
C09 Before  after.
C10 Before {not-set} after.
C11 Early a\]b
C12 prefix <x> & {name} *Bold* / Early *Bold*
C13 <ordinary> &
C14 <ordinary> &
C15 <ordinary> &
C17 <x> & {name} | <x> & {name}
C18 &#169; &amp;
C19 café <α> &
C22 pass:c[Early]
```

```
C06 {name} &
```

```
N01 <x> & {name} *Bold*
N02 <ordinary> &
N03 &#169; &amp;
```

```
R01 <x> & {name} *Bold*
R02 <ordinary> &
R03 &#169; &amp;
```

```
O01 <x> & {name} *Bold*
O02 <ordinary> &
O03 &#169; &amp;
```

```
O04 <x> & {name} *Bold*
O05 <ordinary> &
O06 &#169; &amp;
```

```
D01 {a} | {c}
```

I01 &amp;#169;

I02 &#169;

I03 &amp;#169;
