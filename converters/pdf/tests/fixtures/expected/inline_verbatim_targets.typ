#set document(
  title: "verbatim-targets(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[verbatim-targets(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
#set text(font: ("IBM Plex Serif", "Noto Color Emoji"), size: 11pt, weight: 400, fill: rgb("#111111"), tracking: 0em, lang: "en")
#set par(leading: 0.65em, spacing: 19.15pt, justify: false)
#set block(spacing: 19.15pt)
#set smartquote(enabled: false)
#show regex("[`´]"): set text(features: ("mark": 0))
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
#text(size: 22pt, weight: "bold")[#text("verbatim-targets(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("verbatim-targets - targets inside inline monospace text")

#heading(level: 1)[#text("Description")] <id-5f6465736372697074696f6e>

#text("P01 ")#raw("x")#metadata(none)<id-6d6964646c65>#raw("y")#text(".")

#text("P02 ")#metadata(none)<id-7374617274>#raw("Start")#text(".")

#text("P03 ")#raw("End")#metadata(none)<id-656e64>#raw("")#text(".")

#text("P04 ")#metadata(none)<id-6669727374>#metadata(none)<id-7365636f6e64>#raw("Adjacent")#text(".")

#text("P05 ")#raw("x ")#metadata(none)<id-6265666f72652d7370616365>#raw("y")#text(".")

#text("P06 ")#raw("x")#metadata(none)<id-61667465722d7370616365>#raw(" y")#text(".")

#text("P07 ")#raw("x ")#metadata(none)<id-626f74682d737061636573>#raw("y")#text(".")

#text("P08 ")#raw("café")#metadata(none)<id-756e69636f6465>#raw("fin")#text(".")

#text("P09 ")#raw("x")#metadata(none)<id-73686f727468616e64>#raw("y")#text(".")

#text("P10 ")#metadata(none)<id-656d707479>#raw("")#text(".")

#text("P11 ")#raw("Before ")#metadata(none)<id-6e65737465642d7370616e>#raw("Nested after")#text(".")

#text("P12 ")#raw("Before ")#metadata(none)<id-656d7074792d7370616e>#raw("after")#text(".")

#text("P13 ")#metadata(none)<id-6c696e6b2d6964>#link("https://example.org")[#raw("Label")]#raw("")#text(".")

#text("P14 ")#link("https://example.org")[#raw("Before ")]#metadata(none)<id-6c6162656c2d746172676574>#link("https://example.org")[#raw("after")]#raw("")#text(".")

#text("P15 ")#raw("x")#metadata(none)<id-6174747269627574652d746172676574>#raw("y")#text(".")

#text("P16 ")#raw("First ")#metadata(none)<id-77726170706564>#raw("second")#text(".")

  - #text("P17 ")#raw("x")#metadata(none)<id-6c6973742d746172676574>#raw("y")#text(".")

#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("P18 ")#raw("x")#metadata(none)<id-63656c6c2d746172676574>#raw("y")#text(".")

])

#text("P19 ")#raw("anchor:escaped[]")#text(" and ")#raw("anchor:raw[]")#text(".")

#text("P20 ")#metadata(none)<id-6f757465722d7370616e>#raw("x")#metadata(none)<id-696e6e65722d746172676574>#raw("y")#text(".")

#text("P21 ")#raw(". # <tag> {text} \\path (C)")#text(".")

#text("P22 ")#metadata(none)<id-6974616c6963>#raw("Italic ")#metadata(none)<id-6e65737465642d6d6f6e6f>#raw("Mono ")#metadata(none)<id-6d61726b>#raw("Marked ")#metadata(none)<id-7375706572>#raw("Sup ")#metadata(none)<id-737562>#raw("Sub ")#metadata(none)<id-646f75626c65>#raw("Double ")#metadata(none)<id-73696e676c65>#raw("Single")#text(".")

#pagebreak(weak: true)

#text("See ")#context link(query(<id-6d6964646c65>).first().location())[#text("Middle")]#text(", ")#context link(query(<id-7374617274>).first().location())[#text("Start")]#text(", ")#context link(query(<id-656e64>).first().location())[#text("End")]#text(", ")#context link(query(<id-6669727374>).first().location())[#text("First")]#text(", ")#context link(query(<id-7365636f6e64>).first().location())[#text("Second")]#text(", ")#context link(query(<id-6265666f72652d7370616365>).first().location())[#text("Before")]#text(", ")#context link(query(<id-61667465722d7370616365>).first().location())[#text("After")]#text(", ")#context link(query(<id-626f74682d737061636573>).first().location())[#text("Both")]#text(", ")#context link(query(<id-756e69636f6465>).first().location())[#text("Unicode")]#text(", ")#context link(query(<id-73686f727468616e64>).first().location())[#text("Short")]#text(", ")#context link(query(<id-656d707479>).first().location())[#text("Empty")]#text(", ")#context link(query(<id-6e65737465642d7370616e>).first().location())[#text("Nested")]#text(", ")#context link(query(<id-656d7074792d7370616e>).first().location())[#text("Empty span")]#text(", ")#context link(query(<id-6c696e6b2d6964>).first().location())[#text("Link ID")]#text(", ")#context link(query(<id-6c6162656c2d746172676574>).first().location())[#text("Label")]#text(", ")#context link(query(<id-6174747269627574652d746172676574>).first().location())[#text("Attribute")]#text(", ")#context link(query(<id-77726170706564>).first().location())[#text("Wrapped")]#text(", ")#context link(query(<id-6c6973742d746172676574>).first().location())[#text("List")]#text(", ")#context link(query(<id-63656c6c2d746172676574>).first().location())[#text("Cell")]#text(", ")#context link(query(<id-6f757465722d7370616e>).first().location())[#text("Outer")]#text(", ")#context link(query(<id-696e6e65722d746172676574>).first().location())[#text("Inner")]#text(", ")#context link(query(<id-6974616c6963>).first().location())[#text("Italic")]#text(", ")#context link(query(<id-6e65737465642d6d6f6e6f>).first().location())[#text("Mono")]#text(", ")#context link(query(<id-6d61726b>).first().location())[#text("Marked")]#text(", ")#context link(query(<id-7375706572>).first().location())[#text("Sup")]#text(", ")#context link(query(<id-737562>).first().location())[#text("Sub")]#text(", ")#context link(query(<id-646f75626c65>).first().location())[#text("Double")]#text(", and ")#context link(query(<id-73696e676c65>).first().location())[#text("Single")]#text(".")
