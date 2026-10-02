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
C01 {a}
C02 {c}
C03 {ac}
C05 {long}
C07 {escaped}
C08 {nested}
C09 Before {empty} after.
C10 Before {missing} after.
C11 {brackets}
C12 {alias}
C13 {normal-a}
C14 {normal-ac}
C15 {normal-ca}
C17 {verbatim} | {long-verbatim}
C18 {entities}
C19 {unicode}
C22 {nested-c}
```

```
C06 {none}
```

```
N01 {c}
N02 {normal-ac}
N03 {entities}
```

```
R01 {c}
R02 {normal-ac}
R03 {entities}
```

```
O01 {c}
O02 {normal-ac}
O03 {entities}
```

```
O04 {c}
O05 {normal-ac}
O06 {entities}
```

```
D01 {a} | {c}
```

I01 &amp;#169;

I02 &#169;

I03 &amp;#169;
