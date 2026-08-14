#set document(
  title: "AsciiDoc-style table cells",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[AsciiDoc-style table cells]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
#set text(font: ("IBM Plex Serif", "Noto Color Emoji"), size: 11pt, weight: 400, fill: rgb("#111111"), tracking: 0em, lang: "en")
#set par(leading: 0.65em, spacing: 19.15pt, justify: false)
#set block(spacing: 19.15pt)
#set smartquote(enabled: false)
#show heading: set text(font: ("IBM Plex Serif", "Noto Color Emoji"), weight: 700, fill: rgb("#000000"))
#show heading.where(level: 1): set text(size: 24pt)
#show heading.where(level: 2): set text(size: 18pt)
#show heading.where(level: 3): set text(size: 14pt)
#show heading.where(level: 4): set text(size: 12pt)
#show heading.where(level: 5): set text(size: 11pt)
#show heading.where(level: 6): set text(size: 10pt)
#show link: set text(fill: rgb("#2563eb"))
#show strong: set text(fill: rgb("#000000"), weight: 700)
#show raw: set text(font: ("IBM Plex Mono", "Noto Color Emoji"))
#set raw(theme: "/assets/highlight.tmTheme")
#show raw.where(block: false): set text(fill: rgb("#000000"))
#let tablemonospace(body) = text(font: ("IBM Plex Mono", "Noto Color Emoji"), fill: rgb("#000000"), body)
#show raw.where(block: true): it => block(width: 100%, fill: rgb("#1e1e1e"), radius: 4pt, inset: 10pt, text(fill: rgb("#d4d4d4"), it))
#let captiontext(body) = {
  show strong: set text(fill: rgb("#333333"), weight: 700, style: "normal")
  text(size: 0.91em, weight: 400, style: "italic", fill: rgb("#333333"), body)
}
#let blocktitle(body) = {
  block(width: 100%, above: 19.15pt, below: 0pt, align(left, captiontext(body)))
  block(height: 8pt, above: 0pt, below: 0pt)
}
#let imagecaption(body) = {
  block(height: 8pt, above: 0pt, below: 0pt)
  block(width: 100%, above: 0pt, below: 19.15pt, align(left, captiontext(body)))
}
#let admonitiontitle(body) = {
  block(width: 100%, above: 0pt, below: 0pt, align(left, captiontext(body)))
  block(height: 8pt, above: 0pt, below: 0pt)
}
#let abstract(body) = block(width: 100%, text(size: 13.75pt, style: "italic", fill: rgb("#4b5563"), body))
#let abstracttitle(body) = block(width: 100%, below: 0.5em, align(center, text(size: 12pt, weight: 700, fill: rgb("#000000"), body)))
#let blockquote(body) = block(width: 100%, inset: (left: 12pt), stroke: (left: 3pt + rgb("#d1d5db")), text(style: "italic", fill: rgb("#4b5563"), body))
#let examplebox(body) = block(width: 100%, fill: rgb("#f3f4f6"), radius: 4pt, inset: (x: 12pt, y: 10pt), body)
#let sidebarbox(body) = block(width: 100%, fill: rgb("#f3f4f6"), stroke: 0.75pt + rgb("#e5e7eb"), radius: 4pt, inset: (x: 12pt, y: 10pt), body)
#let sidebartitle(body) = align(center, text(weight: "bold", body))
#let verse(body) = block(inset: (left: 12pt), text(fill: rgb("#4b5563"), body))
#let attribution(body) = block(inset: (left: 12pt), above: 0.6em, text(size: 0.9em, fill: rgb("#4b5563"))[— #body])
#let callout(kind, body) = pad(left: 0pt, block(width: 100%, inset: (x: 12pt, y: 4pt), grid(columns: (auto, 1fr), column-gutter: 12pt, align: (x, _) => if x == 0 { center + horizon } else { left + top }, text(fill: rgb("#111111"), weight: 700, upper(kind)), grid.cell(stroke: (left: 0.75pt + rgb("#e5e7eb")), inset: (left: 12pt), body))))
#let checkbox(checked) = box(height: 0.85em, width: 0.85em, baseline: 0.15em, radius: 2pt, stroke: 0.75pt + rgb("#9ca3af"), fill: if checked { rgb("#374151") } else { white })
#let hr() = block(above: 1.2em, below: 1.2em, line(length: 100%, stroke: 0.75pt + rgb("#e5e7eb")))
#let docimage(path, alt: none, width: none, ratio: none, destination: none) = block(width: 100%, radius: 4pt, clip: true, layout(size => {
  let resolved-width = if ratio != none { ratio * size.width } else if width != none { calc.min(width, size.width) } else { auto }
  let content = image(path, alt: alt, width: resolved-width)
  if destination == none { content } else { link(destination, content) }
}))
#set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))))
#set enum(numbering: (..n) => text(fill: rgb("#9ca3af"))[#numbering("1.", ..n.pos())])
#set table(stroke: (_, y) => (bottom: 0.75pt + rgb("#e5e7eb")), inset: (x: 0.6em, y: 0.45em))
#let tableemphasis(body) = {
  show strong: set text(style: "normal")
  text(style: "italic", body)
}
#let tablestrong(body) = {
  show emph: set text(weight: 400)
  text(weight: 700, body)
}
#let tableheader(body) = {
  show emph: set text(weight: 400)
  text(weight: 700, body)
}

#align(center)[
#text(size: 22pt, weight: "bold")[#text("AsciiDoc-style table cells")]
]
#v(1em)

#heading(outlined: false, bookmarked: false)[#text("Table of Contents")]
#let _acdc_toc_entry(target, depth, body) = context {
  link(
    target,
    pad(
      left: depth * 1.25em,
      grid(
        columns: (auto, 1fr, auto),
        column-gutter: 0.5em,
        body,
        repeat[.],
        str(counter(page).at(target).first()),
      ),
    ),
  )
}
#_acdc_toc_entry(<id-5f6e65737465645f626c6f636b5f636f6e74656e74>, 0, [#text("1. ")#text("Nested block content")])
#_acdc_toc_entry(<id-5f63656c6c5f616e645f636f6c756d6e5f6f7665727269646573>, 0, [#text("2. ")#text("Cell and column overrides")])
#_acdc_toc_entry(<id-5f7461626c655f73656374696f6e735f616e645f7370616e73>, 0, [#text("3. ")#text("Table sections and spans")])
#_acdc_toc_entry(<id-5f6e65737465645f68656164696e67735f616e645f7265666572656e636573>, 0, [#text("4. ")#text("Nested headings and references")])
#_acdc_toc_entry(<id-5f61667465725f7468655f7461626c65>, 0, [#text("5. ")#text("After the table")])
#pagebreak()

#heading(level: 1)[#text("1. ")#text("Nested block content")] <id-5f6e65737465645f626c6f636b5f636f6e74656e74>

#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("First paragraph with ")#strong[#text("bold")]#text(", ")#emph[#text("italic")]#text(", ")#link("https://example.com")[#text("a link")]#text(", and inherited.")

#text("Second paragraph.")

  - #text("Unordered item one")
  - #text("Unordered item two")

#callout("note")[
#text("A simple admonition.")

]

#raw(block: true, "fn main() {}")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Plain neighbour")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #text("Ordered item one")
  + #text("Ordered item two")
]

#text(weight: "bold")[#text("Term")]
#text("Description")

#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Nested A")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Nested B")

])

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Second plain neighbour")

])

#heading(level: 1)[#text("2. ")#text("Cell and column overrides")] <id-5f63656c6c5f616e645f636f6c756d6e5f6f7665727269646573>

#table(columns: (1fr, 1fr, 1fr), align: (left + top, left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Column-level AsciiDoc paragraph.")

  - #text("Nested item")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Cell-level AsciiDoc paragraph.")

#callout("tip")[
#text("Nested tip.")

]

], table.cell(x: 2, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Cell-level AsciiDoc overrides literal.")

  - #text("Another nested item")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Default overrides AsciiDoc.")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Default remains simple.")

], table.cell(x: 2, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#raw(block: false, "Literal overrides AsciiDoc: *bold* {outer-value}")], table.cell(x: 0, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#raw(block: false, "Literal overrides AsciiDoc: *bold* {outer-value}")], table.cell(x: 1, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("AsciiDoc overrides default.")

#raw(block: true, "literal block")

], table.cell(x: 2, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Default overrides literal.")

])

#heading(level: 1)[#text("3. ")#text("Table sections and spans")] <id-5f7461626c655f73656374696f6e735f616e645f7370616e73>

#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.header(repeat: true, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Header ")#strong[#text("bold")]

]], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Header inherited")

]]), table.cell(x: 0, y: 1, colspan: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 1.25pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Colspan paragraph.")

  - #text("Colspan item")

], table.cell(x: 0, y: 2, rowspan: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Rowspan paragraph.")

#callout("warning")[
#text("Rowspan warning.")

]

], table.cell(x: 1, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Beside rowspan")

], table.cell(x: 1, y: 3, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Below rowspan")

], table.footer(repeat: false, table.cell(x: 0, y: 4, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f0f0f0"))[#text("Footer paragraph one.")

#text("Footer paragraph two.")

], table.cell(x: 1, y: 4, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f0f0f0"))[#text("Footer paragraph.")

  - #text("Footer item")

]))

#heading(level: 1)[#text("4. ")#text("Nested headings and references")] <id-5f6e65737465645f68656164696e67735f616e645f7265666572656e636573>

#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Cell sees inherited.")

#heading(level: 1, outlined: false, bookmarked: false)[#text("1. ")#text("Same-level cell heading")] <id-63656c6c2d68656164696e67>

#text("Cell heading text.")

#heading(level: 2, outlined: false, bookmarked: false)[#text("1.1. ")#text("Generated cell child")] <id-5f67656e6572617465645f63656c6c5f6368696c64>

#text("Cell child text.")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Sibling sees inherited.")

])

#heading(level: 1)[#text("5. ")#text("After the table")] <id-5f61667465725f7468655f7461626c65>

#text("Outer content sees inherited.")

#text("See ")#link(<id-63656c6c2d68656164696e67>)[#text("Same-level cell heading")]#text(" and ")#link(<id-5f67656e6572617465645f63656c6c5f6368696c64>)[#text("Generated cell child")]#text(".")
