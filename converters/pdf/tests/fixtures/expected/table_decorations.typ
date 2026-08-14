#set document(
  title: "Table decorations",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Table decorations]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#let docimage(path, width: none, ratio: none) = block(width: 100%, radius: 4pt, clip: true, layout(size => {
  let resolved-width = if ratio != none { ratio * size.width } else if width != none { calc.min(width, size.width) } else { size.width }
  image(path, width: resolved-width)
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
#text(size: 22pt, weight: "bold")[#text("Table decorations")]
]
#v(1em)

#heading(level: 1)[#text("Frame and grid")] <id-5f6672616d655f616e645f67726964>

#blocktitle[#text("Table 1. ")#text("Default all")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("B")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("C")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("D")

])

#blocktitle[#text("Table 2. ")#text("Ends and rows")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: none, right: none, top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: none, right: none, top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("B")

], table.cell(x: 0, y: 1, stroke: (left: none, right: none, top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("C")

], table.cell(x: 1, y: 1, stroke: (left: none, right: none, top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("D")

])

#blocktitle[#text("Table 3. ")#text("Legacy topbot and columns")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: none, right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: none, ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: none, top: 0.5pt + rgb("#dddddd"), bottom: none, ))[#text("B")

], table.cell(x: 0, y: 1, stroke: (left: none, right: 0.5pt + rgb("#dddddd"), top: none, bottom: 0.5pt + rgb("#dddddd"), ))[#text("C")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: none, top: none, bottom: 0.5pt + rgb("#dddddd"), ))[#text("D")

])

#blocktitle[#text("Table 4. ")#text("Sides and no grid")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: none, top: none, bottom: none, ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: none, right: 0.5pt + rgb("#dddddd"), top: none, bottom: none, ))[#text("B")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: none, top: none, bottom: none, ))[#text("C")

], table.cell(x: 1, y: 1, stroke: (left: none, right: 0.5pt + rgb("#dddddd"), top: none, bottom: none, ))[#text("D")

])

#blocktitle[#text("Table 5. ")#text("No frame and no grid")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: none, right: none, top: none, bottom: none, ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: none, right: none, top: none, bottom: none, ))[#text("B")

], table.cell(x: 0, y: 1, stroke: (left: none, right: none, top: none, bottom: none, ))[#text("C")

], table.cell(x: 1, y: 1, stroke: (left: none, right: none, top: none, bottom: none, ))[#text("D")

])

#blocktitle[#text("Table 6. ")#text("Invalid values disable decoration")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: none, right: none, top: none, bottom: none, ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: none, right: none, top: none, bottom: none, ))[#text("B")

], table.cell(x: 0, y: 1, stroke: (left: none, right: none, top: none, bottom: none, ))[#text("C")

], table.cell(x: 1, y: 1, stroke: (left: none, right: none, top: none, bottom: none, ))[#text("D")

])

#blocktitle[#text("Table 7. ")#text("Header divider remains without frame or grid")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.header(repeat: true, table.cell(x: 0, y: 0, stroke: (left: none, right: none, top: none, bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Name")

]], table.cell(x: 1, y: 0, stroke: (left: none, right: none, top: none, bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Value")

]]), table.cell(x: 0, y: 1, stroke: (left: none, right: none, top: 1.25pt + rgb("#dddddd"), bottom: none, ))[#text("A")

], table.cell(x: 1, y: 1, stroke: (left: none, right: none, top: 1.25pt + rgb("#dddddd"), bottom: none, ))[#text("1")

])

#heading(level: 1)[#text("Stripes and sections")] <id-5f737472697065735f616e645f73656374696f6e73>

#blocktitle[#text("Table 8. ")#text("Odd body rows")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.header(repeat: true, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Name")

]], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Value")

]]), table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 1.25pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("A")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 1.25pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("1")

], table.cell(x: 0, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("B")

], table.cell(x: 1, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("2")

], table.cell(x: 0, y: 3, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("C")

], table.cell(x: 1, y: 3, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("3")

], table.footer(repeat: false, table.cell(x: 0, y: 4, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f0f0f0"))[#text("Total")

], table.cell(x: 1, y: 4, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f0f0f0"))[#text("6")

]))

#blocktitle[#text("Table 9. ")#text("Even body rows")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.header(repeat: true, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Name")

]], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Value")

]]), table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 1.25pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 1.25pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("1")

], table.cell(x: 0, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("B")

], table.cell(x: 1, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("2")

], table.cell(x: 0, y: 3, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("C")

], table.cell(x: 1, y: 3, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("3")

], table.footer(repeat: false, table.cell(x: 0, y: 4, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f0f0f0"))[#text("Total")

], table.cell(x: 1, y: 4, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f0f0f0"))[#text("6")

]))

#blocktitle[#text("Table 10. ")#text("All body rows")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("1")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("B")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("2")

])

#blocktitle[#text("Table 11. ")#text("Hover has no static effect")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("1")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("B")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("2")

])

#heading(level: 1)[#text("Spans")] <id-5f7370616e73>

#blocktitle[#text("Table 12. ")#text("Merged cells keep rule gaps and their origin stripe")]
#table(columns: (1fr, 1fr, 1fr), align: (left + top, left + top, left + top), stroke: none, table.cell(x: 0, y: 0, colspan: 2, rowspan: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("Combined")

], table.cell(x: 2, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("Top")

], table.cell(x: 2, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Bottom")

], table.cell(x: 0, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("Left")

], table.cell(x: 1, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("Middle")

], table.cell(x: 2, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("Right")

])

#heading(level: 1)[#text("Source-order document defaults")] <id-5f736f757263655f6f726465725f646f63756d656e745f64656661756c7473>

#blocktitle[#text("Table 13. ")#text("Initial document values")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: none, top: none, bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: none, right: 0.5pt + rgb("#dddddd"), top: none, bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("1")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: none, top: 0.5pt + rgb("#dddddd"), bottom: none, ))[#text("B")

], table.cell(x: 1, y: 1, stroke: (left: none, right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: none, ))[#text("2")

])

#blocktitle[#text("Table 14. ")#text("Changed document values")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: none, right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: none, ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: none, top: 0.5pt + rgb("#dddddd"), bottom: none, ))[#text("1")

], table.cell(x: 0, y: 1, stroke: (left: none, right: 0.5pt + rgb("#dddddd"), top: none, bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("B")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: none, top: none, bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("2")

])

#blocktitle[#text("Table 15. ")#text("Local values override document values")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("1")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("B")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f9f9f9"))[#text("2")

])

#blocktitle[#text("Table 16. ")#text("Unset document values restore defaults")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A")

], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("1")

], table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("B")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("2")

])
