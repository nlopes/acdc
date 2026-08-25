#set document(
  title: "Caption and reference contexts",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Caption and reference contexts]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Caption and reference contexts")]
]
#v(1em)

#heading(level: 1)[#text("Section context")] <id-5f73656374696f6e5f636f6e74657874>

#metadata(none) <id-73656374696f6e2d6578616d706c65>
#blocktitle[#text("Example 1. ")#text("Section example")]
#examplebox[
#text("Section body.")

]

#text("Section reference: ")#link(<id-73656374696f6e2d6578616d706c65>)[#text("Example 1")#text(", “")#text("Section example")#text("”")]#text(".")

#heading(level: 1)[#text("List context")] <id-5f6c6973745f636f6e74657874>

  - #block(width: 100%)[#text("List item refers forward to ")#link(<id-6c6973742d6c697374696e67>)[#text("Listing 1")#text(", “")#text("List listing")#text("”")]#text(".")

#metadata(none) <id-6c6973742d6c697374696e67>
#blocktitle[#text("Listing 1. ")#text("List listing")]
#raw(block: true, "fn in_list() {}")

#text("List item refers back to ")#link(<id-73656374696f6e2d6578616d706c65>)[#text("Example 1")#text(", “")#text("Section example")#text("”")]#text(".")

  ]

#text("After list: ")#link(<id-6c6973742d6c697374696e67>)[#text("Listing 1")#text(", “")#text("List listing")#text("”")]#text(".")

#heading(level: 1)[#text("Table-cell context")] <id-5f7461626c655f63656c6c5f636f6e74657874>

#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Cell refers forward to ")#link(<id-63656c6c2d6578616d706c65>)[#text("Example 2")#text(", “")#text("Cell example")#text("”")]#text(".")

#metadata(none) <id-63656c6c2d6578616d706c65>
#blocktitle[#text("Example 2. ")#text("Cell example")]
#examplebox[
#text("Cell example body.")

]

#text("Cell refers back to ")#link(<id-73656374696f6e2d6578616d706c65>)[#text("Example 1")#text(", “")#text("Section example")#text("”")]#text(".")

])

#text("After table: ")#link(<id-63656c6c2d6578616d706c65>)[#text("Example 2")#text(", “")#text("Cell example")#text("”")]#text(".")

#heading(level: 1)[#text("Multi-page context")] <id-5f6d756c74695f706167655f636f6e74657874>

#text("Forward across pages: ")#link(<id-706167652d7461626c65>)[#text("Table 1")#text(", “")#text("Page table")#text("”")]#text(".")

#pagebreak(weak: true)

#block(sticky: true, above: 0pt, below: 0pt)[
#metadata(none) <id-706167652d7461626c65>
]
#block(sticky: true, above: 0pt, below: 0pt)[
#blocktitle[#text("Table 1. ")#text("Page table")]
]
#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Page target")

])

#pagebreak(weak: true)

#text("Backward across pages: ")#link(<id-706167652d7461626c65>)[#text("Table 1")#text(", “")#text("Page table")#text("”")]#text(".")
