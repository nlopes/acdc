# colon-xrefs(1)

<a id="_name"></a>
## Name

colon-xrefs - colon-containing local targets

<a id="_synopsis"></a>
## Synopsis

**colon-xrefs**

<a id="_description"></a>
## Description

C01 [Leading colon](#:colon) and [Leading label](#:colon) and [Leading macro](#:colon) and [Leading fragment](#:colon).

C02 [Mixed ID](#a-b.c:d) and [Mixed shorthand](#a-b.c:d) and [a-b.c:d](a-b.c:d) and [Mixed macro](a-b.c:d) and [Mixed fragment](#a-b.c:d).

C03 [Topic One](#topic:one) and [Topic One](#topic:one) and [Topic fragment](#topic:one).

C04 [Scheme-like local](#http:local) and [Scheme-like ID](#http:local).

C05 [[missing:one]](#missing:one) and [Missing macro](#missing:one) and [Missing fragment](#missing:one).

C06 [Other document](guide.md#topic:one) and [External URL](https://example.org/guide.md#topic:one).

C07 [Chapter: Details](#_chapter_details) and [Ordinary](#ordinary).

<a id=":colon"></a>Leading target.

<a id="a-b.c:d"></a>
Mixed target.

<a id="topic:one"></a>
Topic target.

<a id="http:local"></a>
Scheme-like target.

<a id="ordinary"></a>
Ordinary target.

<a id="_chapter_details"></a>
## Chapter: Details

C08 [Topic One](#topic:one) and [Backward macro](#topic:one).

C09 [Attribute](#topic:one) and [Passthrough](#topic:one) and <<a-b.c:d,Passed shorthand>>.

C10 xref:topic:one\[Escaped\] and <<topic:one>> and xref:topic:one\[unfinished.

C11 [Directory dot](#dir.name/topic:one) and [Missing dotted shorthand](#missing.id) and [External file](missing.id).
