#set document(
  title: "Combined table span formatting",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Combined table span formatting]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Combined table span formatting")]
]
#v(1em)

#heading(level: 1)[#text("Column defaults")] <id-5f636f6c756d6e5f64656661756c7473>

#blocktitle[#text("Defaults follow source cells")]
#table(columns: (1fr, 1fr, 1fr), align: (right + bottom, left + top, center + horizon), table.cell(x: 0, y: 0, colspan: 2, rowspan: 2, align: center + horizon)[#tableemphasis[#text("Combined ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 2, y: 0, align: left + top)[#tableemphasis[#text("First companion ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 2, y: 1, align: right + bottom)[#tablestrong[#text("Second companion ")#emph[#text("italic")]#text(" Ada")

]], table.cell(x: 0, y: 2)[#tablestrong[#text("First ")#strong[#text("bold")]

]], table.cell(x: 1, y: 2)[#tableemphasis[#text("Second ")#strong[#text("bold")]

]], table.cell(x: 2, y: 2)[#tableheader[#text("Third ")#emph[#text("italic")]

]])

#blocktitle[#text("Duplicates use each column")]
#table(columns: (1fr, 1fr, 1fr), align: (right + bottom, left + top, center + horizon), table.cell(x: 0, y: 0)[#tablestrong[#text("Repeated ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 1, y: 0)[#tableemphasis[#text("Repeated ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 2, y: 0)[#tableheader[#text("Repeated ")#strong[#text("bold")]#text(" Ada")

]])

#heading(level: 1)[#text("Explicit span formats")] <id-5f6578706c696369745f7370616e5f666f726d617473>

#table(columns: (1fr, 1fr, 1fr), align: (left + top, left + top, left + top), table.cell(x: 0, y: 0, colspan: 2, rowspan: 2, align: left + top)[#tablestrong[#text("Strong ")#emph[#text("italic")]

]], table.cell(x: 2, y: 0)[#text("Beside strong")

], table.cell(x: 2, y: 1)[#text("Below strong")

], table.cell(x: 0, y: 2, colspan: 2, rowspan: 2, align: center + horizon)[#tableemphasis[#text("Emphasis ")#strong[#text("bold")]

]], table.cell(x: 2, y: 2)[#text("Beside emphasis")

], table.cell(x: 2, y: 3)[#text("Below emphasis")

], table.cell(x: 0, y: 4, colspan: 2, rowspan: 2, align: right + bottom)[#tableheader[#text("Header ")#emph[#text("italic")]

]], table.cell(x: 2, y: 4)[#text("Beside header")

], table.cell(x: 2, y: 5)[#text("Below header")

], table.cell(x: 0, y: 6, colspan: 2, rowspan: 2, align: center + top)[#raw(block: false, "Literal *bold* {span-name}")], table.cell(x: 2, y: 6)[#text("Beside literal")

], table.cell(x: 2, y: 7)[#text("Below literal")

], table.cell(x: 0, y: 8, colspan: 2, rowspan: 2, align: right + horizon)[#tablemonospace[#text("Monospace ")#strong[#text("bold")]#text(" Ada")

]], table.cell(x: 2, y: 8)[#text("Beside monospace")

], table.cell(x: 2, y: 9)[#text("Below monospace")

])

#heading(level: 1)[#text("AsciiDoc span")] <id-5f6173636969646f635f7370616e>

#table(columns: (1fr, 1fr, 1fr), align: (left + top, left + top, left + top), table.cell(x: 0, y: 0, colspan: 2, rowspan: 2, align: left + bottom)[#text("Paragraph in a combined cell.")

  - #text("Nested item one")
  - #text("Nested item two")

], table.cell(x: 2, y: 0)[#text("Beside AsciiDoc")

], table.cell(x: 2, y: 1)[#text("Below AsciiDoc")

])
