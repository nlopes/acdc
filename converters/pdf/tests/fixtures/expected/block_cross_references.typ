#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Cross-reference coverage]], footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
#set text(font: ("IBM Plex Serif", "Noto Color Emoji"), size: 11pt, weight: 400, fill: rgb("#111111"), tracking: 0em, lang: "en")
#set par(leading: 0.65em, justify: false)
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
#show raw.where(block: true): it => block(width: 100%, fill: rgb("#1e1e1e"), radius: 4pt, inset: 10pt, text(fill: rgb("#d4d4d4"), it))
#let blockquote(body) = block(inset: (left: 12pt), stroke: (left: 3pt + rgb("#d1d5db")), text(style: "italic", fill: rgb("#4b5563"), body))
#let _cbadge(body) = box(circle(radius: 0.6em, fill: rgb("#111111"), inset: 0pt, align(center + horizon, body)))
#let _cico(glyph) = _cbadge(text(fill: white, weight: 700, size: 0.82em)[#glyph])
#let _ccheck = _cbadge(box(width: 0.62em, height: 0.62em, place(curve(stroke: (paint: white, thickness: 1.5pt, cap: "round", join: "round"), curve.move((0em, 0.34em)), curve.line((0.21em, 0.55em)), curve.line((0.58em, 0.08em))))))
#let _cicon(kind) = ("note": _cico("i"), "tip": _cico("i"), "important": _cico("!"), "warning": _cico("!"), "caution": _cico("!"), "success": _ccheck).at(kind, default: _cico("i"))
#let callout(kind, body) = pad(left: 0pt, block(width: 100%, fill: rgb("#f3f4f6"), radius: 4pt, inset: (x: 12pt, y: 10pt), grid(columns: (auto, 1fr), column-gutter: 9.600000000000001pt, align: top, _cicon(kind), body)))
#let checkbox(checked) = box(height: 0.85em, width: 0.85em, baseline: 0.15em, radius: 2pt, stroke: 0.75pt + rgb("#9ca3af"), fill: if checked { rgb("#374151") } else { white })
#let hr() = block(above: 1.2em, below: 1.2em, line(length: 100%, stroke: 0.75pt + rgb("#e5e7eb")))
#let docimage(path) = block(radius: 4pt, clip: true, image(path, width: 100%))
#set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))))
#set enum(numbering: (..n) => text(fill: rgb("#9ca3af"))[#numbering("1.", ..n.pos())])
#set table(stroke: (_, y) => (bottom: 0.75pt + rgb("#e5e7eb")), inset: (x: 0.6em, y: 0.45em))
#let tableheader(body) = text(weight: 700, body)

#align(center)[
#text(size: 22pt, weight: "bold")[#text("Cross-reference coverage")]
]
#v(1em)

#text("See ")#link(<id-73656374696f6e2d6964>)[#text("Section Title")]#text(", ")#link(<id-7061726167726170682d6964>)[#text("Paragraph Title")]#text(", ")#link(<id-6c697374696e672d6964>)[#text("Listing Title")]#text(", ")#link(<id-6c69746572616c2d6964>)[#text("Literal Title")]#text(", ")#link(<id-736f757263652d6964>)[#text("Source Title")]#text(", ")#link(<id-6578616d706c652d6964>)[#text("Example Title")]#text(", ")#link(<id-6f70656e2d6964>)[#text("Open Title")]#text(", ")#link(<id-736964656261722d6964>)[#text("Sidebar Title")]#text(", ")#link(<id-71756f74652d6964>)[#text("Quote Title")]#text(", ")#link(<id-76657273652d6964>)[#text("Verse Title")]#text(", ")#link(<id-706173737468726f7567682d6964>)[#text("Passthrough Title")]#text(", ")#link(<id-7374656d2d6964>)[#text("Stem Title")]#text(", ")#link(<id-7461626c652d6964>)[#text("Table ")#strong[#text("Title")]]#text(", ")#link(<id-6f7264657265642d6964>)[#text("Ordered List Title")]#text(", ")#link(<id-756e6f7264657265642d6964>)[#text("Unordered List Title")]#text(", ")#link(<id-6465736372697074696f6e2d6964>)[#text("Description List Title")]#text(", ")#link(<id-63616c6c6f75742d6964>)[#text("Callout List Title")]#text(", ")#link(<id-61646d6f6e6974696f6e2d6964>)[#text("Admonition ")#strong[#text("Title")]]#text(", ")#link(<id-696d6167652d6964>)[#text("Image Title")]#text(", ")#link(<id-766964656f2d6964>)[#text("Video Title")]#text(", ")#link(<id-617564696f2d6964>)[#text("Audio Title")]#text(", ")#link(<id-7468656d617469632d6964>)[#text("Thematic Title")]#text(", ")#link(<id-706167652d6964>)[#text("Page Title")]#text(", and ")#link(<id-64697363726574652d6964>)[#text("Discrete Title")]#text(".")

#text("See also ")#link(<id-637573746f6d2d6c697374696e672d6964>)[#text("Custom Listing")]#text(" and ")#link(<id-7465726d2d6964>)[#text("[term-id]")]#text(".")

#heading(level: 1)[#text("Section Title")] <id-73656374696f6e2d6964>

#metadata(none) <id-7061726167726170682d6964>
#text(weight: "bold")[#text("Paragraph Title")]
#text("Paragraph body.")

#metadata(none) <id-6c697374696e672d6964>
#text(weight: "bold")[#text("Listing Title")]
#raw(block: true, "listing")

#metadata(none) <id-637573746f6d2d6c697374696e672d6964>
#text(weight: "bold")[#text("Ignored Listing Title")]
#raw(block: true, "custom listing")

#metadata(none) <id-736f757263652d6964>
#text(weight: "bold")[#text("Source Title")]
#raw(block: true, "fn main() {}")

#metadata(none) <id-6c69746572616c2d6964>
#text(weight: "bold")[#text("Literal Title")]
#raw(block: true, "literal")

#metadata(none) <id-6578616d706c652d6964>
#text(weight: "bold")[#text("Example Title")]
#block(fill: luma(248), inset: 8pt, width: 100%)[
#text("Example body.")

]

#metadata(none) <id-6f70656e2d6964>
#text(weight: "bold")[#text("Open Title")]
#block(fill: luma(248), inset: 8pt, width: 100%)[
#text("Open body.")

]

#metadata(none) <id-736964656261722d6964>
#text(weight: "bold")[#text("Sidebar Title")]
#block(fill: luma(248), inset: 8pt, width: 100%)[
#text("Sidebar body.")

]

#metadata(none) <id-71756f74652d6964>
#text(weight: "bold")[#text("Quote Title")]
#blockquote[
#text("Quote body.")

]

#metadata(none) <id-76657273652d6964>
#text(weight: "bold")[#text("Verse Title")]
#raw(block: true, "Verse line.")

#metadata(none) <id-706173737468726f7567682d6964>
#text(weight: "bold")[#text("Passthrough Title")]
#raw(block: true, "passthrough")

#metadata(none) <id-7374656d2d6964>
#text(weight: "bold")[#text("Stem Title")]
#block[#text("x = 1")]

#metadata(none) <id-7461626c652d6964>
#text(weight: "bold")[#text("Table ")#strong[#text("Title")]]
#table(columns: 1, table.cell(x: 0, y: 0)[#text("Cell")

])

#metadata(none) <id-6f7264657265642d6964>
#text(weight: "bold")[#text("Ordered List Title")]
  + #text("ordered")

#metadata(none) <id-756e6f7264657265642d6964>
#text(weight: "bold")[#text("Unordered List Title")]
  - #text("unordered")

#metadata(none) <id-6465736372697074696f6e2d6964>
#text(weight: "bold")[#text("Description List Title")]
#text(weight: "bold")[#text("Term")]
#text("definition")

#raw(block: true, "code <1>")

#metadata(none) <id-63616c6c6f75742d6964>
#text(weight: "bold")[#text("Callout List Title")]
- #text("(1) ")#text("callout")

#metadata(none) <id-61646d6f6e6974696f6e2d6964>
#text(weight: "bold")[#text("Admonition ")#strong[#text("Title")]]
#callout("note")[
#text("note")

]

#metadata(none) <id-696d6167652d6964>
#text(weight: "bold")[#text("Image Title")]
#text("Image Title")

#metadata(none) <id-766964656f2d6964>
#text(weight: "bold")[#text("Video Title")]
#text("[video: video.mp4]")

#metadata(none) <id-617564696f2d6964>
#text(weight: "bold")[#text("Audio Title")]
#text("[audio: audio.mp3]")

#metadata(none) <id-7468656d617469632d6964>
#hr()

#metadata(none) <id-706167652d6964>
#pagebreak()

#metadata(none) <id-64697363726574652d6964>
#heading(level: 2, outlined: false)[#text("Discrete Title")]

#metadata(none) <id-7465726d2d6964>
#text(weight: "bold")[#text("Term")]
#text("Definition")

#text("Paragraph with ")#metadata(none) <id-696e6c696e652d6669727374>#text("one anchor and ")#metadata(none) <id-696e6c696e652d7365636f6e64>#text("a second anchor.")

  - #text("List item with ")#metadata(none) <id-696e6c696e652d6c697374>#text("an anchor.")

#text("See ")#link(<id-696e6c696e652d6669727374>)[#text("Inline First")]#text(", ")#link(<id-696e6c696e652d7365636f6e64>)[#text("Inline Second")]#text(", and ")#link(<id-696e6c696e652d6c697374>)[#text("Inline List")]#text(".")

