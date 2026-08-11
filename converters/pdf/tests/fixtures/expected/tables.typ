#set document(
  title: "Table coverage",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Table coverage]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#let docimage(path) = block(radius: 4pt, clip: true, image(path, width: 100%))
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
#text(size: 22pt, weight: "bold")[#text("Table coverage")]
]
#v(1em)

#blocktitle[#text("Declared header")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), table.header(repeat: true, table.cell(x: 0, y: 0)[#tableheader[#text("Name")

]], table.cell(x: 1, y: 0)[#tableheader[#text("Value")

]]), table.cell(x: 0, y: 1)[#text("one")

], table.cell(x: 1, y: 1)[#text("two")

])

#blocktitle[#text("Body only")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), table.cell(x: 0, y: 0)[#text("one")

], table.cell(x: 1, y: 0)[#text("two")

])

#blocktitle[#text("Spans")]
#table(columns: (1fr, 1fr, 1fr, 1fr), align: (left + top, left + top, left + top, left + top), table.header(repeat: true, table.cell(x: 0, y: 0, colspan: 2)[#tableheader[#text("Header")

]], table.cell(x: 2, y: 0)[#tableheader[#text("H3")

]], table.cell(x: 3, y: 0)[#tableheader[#text("H4")

]]), table.cell(x: 0, y: 1, colspan: 2, rowspan: 2)[#text("Combined")

], table.cell(x: 2, y: 1)[#text("Top 3")

], table.cell(x: 3, y: 1)[#text("Top 4")

], table.cell(x: 2, y: 2)[#text("Bottom 3")

], table.cell(x: 3, y: 2)[#text("Bottom 4")

], table.footer(repeat: false, table.cell(x: 0, y: 3, colspan: 4)[#text("Full width footer")

]))

#blocktitle[#text("Inferred equal widths")]
#table(columns: (1fr, 1fr, 1fr), align: (left + top, left + top, left + top), table.cell(x: 0, y: 0)[#text("one")

], table.cell(x: 1, y: 0)[#text("two")

], table.cell(x: 2, y: 0)[#text("three")

])

#blocktitle[#text("Proportional widths")]
#table(columns: (1fr, 3fr), align: (left + top, left + top), table.cell(x: 0, y: 0)[#text("one")

], table.cell(x: 1, y: 0)[#text("three")

])

#blocktitle[#text("Percentage widths")]
#table(columns: (25fr, 75fr), align: (left + top, left + top), table.cell(x: 0, y: 0)[#text("twenty-five")

], table.cell(x: 1, y: 0)[#text("seventy-five")

])

#blocktitle[#text("Mixed numeric and percentage widths")]
#table(columns: (1fr, 25fr), align: (left + top, left + top), table.cell(x: 0, y: 0)[#text("one")

], table.cell(x: 1, y: 0)[#text("twenty-five")

])

#blocktitle[#text("Automatic widths")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), table.cell(x: 0, y: 0)[#text("one")

], table.cell(x: 1, y: 0)[#text("two")

])

#blocktitle[#text("Automatic width with fixed widths")]
#table(columns: (1fr, 20%, 30%), align: (left + top, left + top, left + top), table.cell(x: 0, y: 0)[#text("fifty")

], table.cell(x: 1, y: 0)[#text("twenty")

], table.cell(x: 2, y: 0)[#text("thirty")

])

#blocktitle[#text("Content-sized table")]
#table(columns: 2, align: (left + top, left + top), table.cell(x: 0, y: 0)[#text("one")

], table.cell(x: 1, y: 0)[#text("three")

])

#blocktitle[#text("Horizontal column alignment")]
#table(columns: (1fr, 1fr, 1fr), align: (left + top, center + top, right + top), table.cell(x: 0, y: 0)[#text("Left")

], table.cell(x: 1, y: 0)[#text("Center")

], table.cell(x: 2, y: 0)[#text("Right")

])

#blocktitle[#text("Vertical column alignment")]
#table(columns: (1fr, 1fr, 1fr, 1fr), align: (left + top, left + horizon, left + bottom, left + top), table.cell(x: 0, y: 0)[#text("Top")

], table.cell(x: 1, y: 0)[#text("Middle")

], table.cell(x: 2, y: 0)[#text("Bottom")

], table.cell(x: 3, y: 0)[#text("This deliberately long cell makes the row tall enough to show each vertical alignment.")

])

#blocktitle[#text("Cell alignment overrides")]
#table(columns: (1fr, 1fr, 1fr, 1fr), align: (right + bottom, right + bottom, right + bottom, right + bottom), table.cell(x: 0, y: 0, align: left)[#text("Left")

], table.cell(x: 1, y: 0, align: center)[#text("Center")

], table.cell(x: 2, y: 0, align: right)[#text("Right")

], table.cell(x: 3, y: 0)[#text("This deliberately long cell makes the row tall enough to show horizontal overrides while retaining bottom alignment.")

], table.cell(x: 0, y: 1, align: top)[#text("Top")

], table.cell(x: 1, y: 1, align: horizon)[#text("Middle")

], table.cell(x: 2, y: 1, align: bottom)[#text("Bottom")

], table.cell(x: 3, y: 1)[#text("This deliberately long cell makes the row tall enough to show vertical overrides while retaining right alignment.")

], table.cell(x: 0, y: 2, align: left + top)[#text("Left and top")

], table.cell(x: 1, y: 2, align: center + horizon)[#text("Center and middle")

], table.cell(x: 2, y: 2, align: right + bottom)[#text("Right and bottom")

], table.cell(x: 3, y: 2)[#text("This deliberately long cell makes the row tall enough to show combined alignment overrides.")

])

#blocktitle[#text("Cell alignment after spans")]
#table(columns: (1fr, 1fr, 1fr), align: (right + top, left + top, center + top), table.cell(x: 0, y: 0, colspan: 2)[#text("Spans logical columns one and two")

], table.cell(x: 2, y: 0, align: left + bottom)[#text("Bottom in logical column three")

], table.cell(x: 0, y: 1, rowspan: 2)[#text("Spans two rows")

], table.cell(x: 1, y: 1)[#text("First row, logical column two")

], table.cell(x: 2, y: 1)[#text("First row, logical column three")

], table.cell(x: 1, y: 2, align: right + bottom)[#text("Bottom in logical column two")

], table.cell(x: 2, y: 2, align: left + top)[#text("This deliberately long cell makes the second row tall enough to show its first real cell at the bottom.")

])

#blocktitle[#text("Cell alignment in table sections")]
#table(columns: (1fr, 1fr), align: (right + bottom, left + top), table.header(repeat: true, table.cell(x: 0, y: 0, align: center + horizon)[#tableheader[#text("Header center and middle")

]], table.cell(x: 1, y: 0, align: bottom)[#tableheader[#text("Header bottom")

]]), table.cell(x: 0, y: 1, align: left + top)[#text("Body left and top")

], table.cell(x: 1, y: 1)[#text("This deliberately long cell makes the body row tall enough to show its first cell at the top.")

], table.footer(repeat: false, table.cell(x: 0, y: 2, align: center + bottom)[#text("Footer center and bottom")

], table.cell(x: 1, y: 2, align: top)[#text("Footer top")

]))

#blocktitle[#text("Column cell styles")]
#table(columns: (1fr, 1fr, 1fr, 1fr, 1fr, 1fr), align: (left + top, left + top, left + top, left + top, left + top, left + top), table.cell(x: 0, y: 0)[#text("Default ")#strong[#text("bold")]#text(" Ada")

], table.cell(x: 1, y: 0)[#tableemphasis[#text("Emphasis ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 2, y: 0)[#tableheader[#text("Header ")#emph[#text("italic")]#text(" Ada")

]], table.cell(x: 3, y: 0)[#raw(block: false, "Literal *bold* {table-name}")], table.cell(x: 4, y: 0)[#tablemonospace[#text("Monospace ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 5, y: 0)[#tablestrong[#text("Strong ")#emph[#text("italic")]#text(" Ada")

]])

#blocktitle[#text("Cell style overrides")]
#table(columns: (1fr, 1fr, 1fr), align: (left + top, left + top, left + top), table.cell(x: 0, y: 0)[#text("Default")

], table.cell(x: 1, y: 0)[#tableemphasis[#text("Emphasis")

]], table.cell(x: 2, y: 0)[#tableheader[#text("Header")

]], table.cell(x: 0, y: 1)[#raw(block: false, "Literal *bold* {table-name}")], table.cell(x: 1, y: 1)[#tablemonospace[#text("Monospace ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 2, y: 1)[#tablestrong[#text("Strong ")#emph[#text("italic")]#text(" Ada")

]])

#blocktitle[#text("Cell styles in table sections")]
#table(columns: (1fr, 1fr, 1fr), align: (left + top, left + top, left + top), table.header(repeat: true, table.cell(x: 0, y: 0)[#tableheader[#text("Header ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 1, y: 0)[#tableheader[#text("Header ")#emph[#text("italic")]#text(" Ada")

]], table.cell(x: 2, y: 0)[#tableheader[#text("Header ")#emph[#text("italic")]#text(" Ada")

]]), table.cell(x: 0, y: 1)[#raw(block: false, "Body literal *bold* {table-name}")], table.cell(x: 1, y: 1)[#tableemphasis[#text("Body emphasis ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 2, y: 1)[#tablestrong[#text("Body strong ")#emph[#text("italic")]#text(" Ada")

]], table.footer(repeat: false, table.cell(x: 0, y: 2)[#raw(block: false, "Footer literal *bold* {table-name}")], table.cell(x: 1, y: 2)[#tablemonospace[#text("Footer monospace ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 2, y: 2)[#tableheader[#text("Footer header ")#emph[#text("italic")]#text(" Ada")

]]))

#blocktitle[#text("Literal and monospace whitespace")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), table.cell(x: 0, y: 0)[#raw(block: false, "Literal  keeps\n  spaces and *marks* {table-name}")], table.cell(x: 1, y: 0)[#tablemonospace[#text("Monospace collapses spaces and ")#strong[#text("formats")]#text(" Ada")

]])
