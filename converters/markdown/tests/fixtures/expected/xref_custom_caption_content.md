# Custom caption references

Forward: [**First**](#custom) / [**First**, “An *example*”](#custom) / [An *example*](#custom).

<a id="custom"></a>
<strong>**First**. An *example*</strong>

<!-- Warning: example blocks not natively supported in Markdown, using blockquote -->
> Body.

Backward: [**First**](#custom) / [**First**, “An *example*”](#custom).

<a id="table"></a>
<strong>**Table prefix**. A table</strong>

|  |
| :--- |
| Cell |

Table: [**Table prefix**](#table) / [**Table prefix**, “A table”](#table).

<a id="listing"></a>
<strong>**Code prefix**. A listing</strong>

```
code
```

Listing: [**Code prefix**](#listing) / [**Code prefix**, “A listing”](#listing).

<a id="punctuation"></a>
<strong>**Keep.** Styled period</strong>

<!-- Warning: example blocks not natively supported in Markdown, using blockquote -->
> Body.

Punctuation: [**Keep.** ](#punctuation) / [**Keep.** , “Styled period”](#punctuation).

<a id="raw"></a>
<strong>**Raw** . Separate suffix</strong>

<!-- Warning: example blocks not natively supported in Markdown, using blockquote -->
> Body.

Raw suffix: [**Raw** ](#raw) / [**Raw** , “Separate suffix”](#raw).

<a id="raw-role"></a>
<strong><u class="underline">Keep. </u>Attributed raw suffix</strong>

<!-- Warning: example blocks not natively supported in Markdown, using blockquote -->
> Body.

Raw role: [<u class="underline">Keep. </u>](#raw-role) / [<u class="underline">Keep. </u>, “Attributed raw suffix”](#raw-role).

<a id="labelled"></a>
<strong>**Unused**. A title</strong>

<!-- Warning: example blocks not natively supported in Markdown, using blockquote -->
> Body.

Label: [Chosen *label*](#labelled) / [Chosen *label*](#labelled).

<a id="nested"></a>
<strong>**See** [An *example*](#custom). Nested reference</strong>

<!-- Warning: example blocks not natively supported in Markdown, using blockquote -->
> Body.

Nested: [**See** [custom]](#nested) / [**See** [custom], “Nested reference”](#nested).

<a id="linked"></a>
<strong>**Visit** [site](https://example.org). External link</strong>

<!-- Warning: example blocks not natively supported in Markdown, using blockquote -->
> Body.

Linked: [**Visit** site](#linked) / [**Visit** site, “External link”](#linked).

<a id="registered"></a>
<strong><a id="caption-mark"></a>**Mark** [^caption-note]. Registered content</strong>

<!-- Warning: example blocks not natively supported in Markdown, using blockquote -->
> Body.

Registered: [**Mark** [1]](#registered) / [**Mark** [1], “Registered content”](#registered).

Buffered: <sup>[**First**](#custom)</sup>.

```
custom / custom
punctuation / punctuation
```

[^caption-note]: One note
