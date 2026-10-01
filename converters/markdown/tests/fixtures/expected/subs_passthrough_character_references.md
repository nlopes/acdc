# char-refs(1)

<a id="_name"></a>
## NAME

char-refs - passthrough character references

<a id="_synopsis"></a>
## SYNOPSIS

Character references follow substitution order.

<a id="_description"></a>
## DESCRIPTION

P01 &#169; &#xA9; &#000169; &#x000A9;

P02 &amp;#169; &amp;#xA9; &amp;#000169; &amp;#x000A9;

P03 &#169; &lt;tag&gt; &amp;

P04 &amp;#169; &lt;tag&gt; &amp;

P05 &amp;#169; \\&amp;#169; \\\\&amp;#169;

P06 &amp;#169; \&amp;#169;

P07 &amp;#9; &amp;#x9; &amp;#0000169; &amp;#X00A9; &amp;#x0000A9; &amp;#bad; &amp;a; &amp;ab123; &amp;ab1c;

P08 &amp;#169; &amp;lt;tag&amp;gt; &amp;amp;

P09 &#169;

P10 &amp;#169; &amp;lt;tag&amp;gt; &amp;amp;

P11 &#169;

P12 &#169;

P13 &#169;

P14 &#169;

P15 &amp;#169;

P16 &#169;

P17 **&#169;**

P18 **&#169;**

P19 [https://example.org](https://example.org)\[&#169;\]

P20 [https://example.org](https://example.org)\[&#169;\]

P21 café &#169;Ω &#169;&#xA9;

P22 &#169; — tail

P23 head — &#169;

P24 &amp;amp;#169; &amp;amp;

P25 &amp;amp;#169;

```
C01 &#169; <tag> &
C02 &#169; &lt;tag&gt; &amp;
C03 &#169; \&#169;
C04 &#9; &#x9; &#0000169; &#X00A9;
```

```
D01 pass:c,r[&#169;]
```
