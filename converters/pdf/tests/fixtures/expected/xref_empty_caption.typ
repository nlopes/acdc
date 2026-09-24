#set document(
  title: "Empty caption references",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Empty caption references]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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

#let _acdc_arabic_page_start = none
#set page(numbering: "i")
#set page(numbering: "1")
#counter(page).update(1)
#align(center)[
#text(size: 22pt, weight: "bold")[#text("Empty caption references")]
]
#v(1em)

#text("Full: ")#context link(query(<id-666967757265>).first().location())[#text("Figure ")#emph[#text("title")]]#text("; ")#context link(query(<id-7461626c65>).first().location())[#text("Table ")#strong[#text("title")]]#text("; ")#context link(query(<id-6578616d706c65>).first().location())[#text("Example title")]#text("; ")#context link(query(<id-6c697374696e67>).first().location())[#text("Listing title")]#text(".")

#text("Short: ")#context link(query(<id-666967757265>).first().location())[#text("Figure ")#emph[#text("title")]]#text("; ")#context link(query(<id-7461626c65>).first().location())[#text("Table ")#strong[#text("title")]]#text("; ")#context link(query(<id-6578616d706c65>).first().location())[#text("Example title")]#text("; ")#context link(query(<id-6c697374696e67>).first().location())[#text("Listing title")]#text(".")

#text("Basic: ")#context link(query(<id-666967757265>).first().location())[#text("Figure ")#emph[#text("title")]]#text("; ")#context link(query(<id-7461626c65>).first().location())[#text("Table ")#strong[#text("title")]]#text(".")

#text("Overrides: ")#context link(query(<id-7461626c65>).first().location())[#text("Explicit ")#strong[#text("label")]]#text("; ")#context link(query(<id-6c6162656c6c6564>).first().location())[#text("Chosen ")#strong[#text("label")]]#text("; ")#context link(query(<id-6c6162656c6c6564>).first().location())[#text("Chosen ")#strong[#text("label")]]#text(".")

#text("Untitled: ")#context link(query(<id-756e7469746c6564>).first().location())[#text("[untitled]")]#text("; ")#context link(query(<id-756e7469746c6564>).first().location())[#text("[untitled]")]#text(".")

#text("Controls: ")#context link(query(<id-737061636564>).first().location())[#text(" ")]#text("; ")#context link(query(<id-737061636564>).first().location())[#text(" ")#text(", “")#text("Space caption")#text("”")]#text("; ")#context link(query(<id-637573746f6d>).first().location())[#text("Exhibit")]#text("; ")#context link(query(<id-637573746f6d>).first().location())[#text("Exhibit")#text(", “")#text("Custom caption")#text("”")]#text("; ")#context link(query(<id-6e756d6265726564>).first().location())[#text("Table 1")]#text("; ")#context link(query(<id-6e756d6265726564>).first().location())[#text("Table 1")#text(", “")#text("Numbered table")#text("”")]#text(".")

#metadata(none) <id-666967757265>
#block(width: 100%, breakable: false)[
#docimage("/images/de454d7e4e1cfda7.svg", alt: "Figure", width: 30pt)
#imagecaption[#text("")#text("Figure ")#emph[#text("title")]]
]

#block(sticky: true, above: 0pt, below: 0pt)[
#metadata(none) <id-7461626c65>
]
#block(sticky: true, above: 0pt, below: 0pt)[
#blocktitle[#text("")#text("Table ")#strong[#text("title")]]
]
#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Cell")

])

#metadata(none) <id-6578616d706c65>
#blocktitle[#text("")#text("Example title")]
#examplebox[
#text("Body.")

]

#metadata(none) <id-6c697374696e67>
#blocktitle[#text("")#text("Listing title")]
#raw(block: true, "Code.")

#metadata(none) <id-6c6162656c6c6564>
#blocktitle[#text("")#text("Unused title")]
#examplebox[
#text("Body.")

]

#metadata(none) <id-756e7469746c6564>
#examplebox[
#text("Body.")

]

#metadata(none) <id-737061636564>
#blocktitle[#text(" ")#text("Space caption")]
#examplebox[
#text("Body.")

]

#metadata(none) <id-637573746f6d>
#blocktitle[#text("Exhibit")#text("Custom caption")]
#examplebox[
#text("Body.")

]

#block(sticky: true, above: 0pt, below: 0pt)[
#metadata(none) <id-6e756d6265726564>
]
#block(sticky: true, above: 0pt, below: 0pt)[
#blocktitle[#text("Table 1. ")#text("Numbered table")]
]
#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Cell")

])

#metadata(none) <id-67656e65726963>
#blocktitle[#text("")#text("Document caption")]
#examplebox[
#text("Body.")

]

#metadata(none) <id-636f6c6c6170736564>
#block(width: 100%, below: 0.8em)[
#grid(columns: (0.8em, 1fr), column-gutter: 0.2em, align: top, [#box(width: 0.8em, height: 0.8em, baseline: 0.1em, align(center + horizon, rotate(90deg, origin: center, text(weight: "bold", size: 0.8em, ">"))))], [#captiontext[#text("Collapsible example")]])
#block(inset: (left: 1em), above: 0.3em)[
#text("Body.")


]
]

#text("Full automatic empty captions: ")#context link(query(<id-67656e65726963>).first().location())[#text("Document caption")]#text("; ")#context link(query(<id-636f6c6c6170736564>).first().location())[#text("Collapsible example")]#text(".")

#text("Short automatic empty captions: ")#context link(query(<id-67656e65726963>).first().location())[#text("Document caption")]#text("; ")#context link(query(<id-636f6c6c6170736564>).first().location())[#text("Collapsible example")]#text("; ")#context link(query(<id-7461626c65>).first().location())[#text("Table ")#strong[#text("title")]]#text(".")

#text("Default: ")#context link(query(<id-7461626c65>).first().location())[#text("Table ")#strong[#text("title")]]#text(".")
