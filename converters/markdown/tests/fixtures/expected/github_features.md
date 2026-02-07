# GitHub-Specific Features Test

This document tests GitHub Flavored Markdown specific features.

## GitHub Alerts

Alerts are GitHub's native admonition syntax.

> [!NOTE]
> This is a note alert using GitHub's native syntax.


> [!TIP]
> This is a helpful tip.


> [!IMPORTANT]
> Pay attention to this important information.


> [!WARNING]
> This is a warning about potential issues.


> [!CAUTION]
> Exercise caution with this operation.


## Footnotes

GitHub supports footnotes using the `\[^1\]` syntax.

Here is a simple footnote.[^1]

You can also use named footnotes.[^named]

Multiple references to the same footnote work too.[^named]

## Combined Example

> [!NOTE]
> This alert contains a footnote reference.[^3]

> It also has multiple paragraphs.


The document continues with normal text.[^4]


[^1]: This is the footnote content.
[^named]: This is a named footnote.
[^3]: Footnote inside an alert.
[^4]: Final footnote.

