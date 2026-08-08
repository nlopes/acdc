#set document(
  title: "Paragraph metadata",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Paragraph metadata]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#let abstract(body) = block(width: 100%, text(size: 13.75pt, style: "italic", fill: rgb("#4b5563"), body))
#let abstracttitle(body) = block(width: 100%, below: 0.5em, align(center, text(size: 12pt, weight: 700, fill: rgb("#000000"), body)))
#let blockquote(body) = block(width: 100%, inset: (left: 12pt), stroke: (left: 3pt + rgb("#d1d5db")), text(style: "italic", fill: rgb("#4b5563"), body))
#let examplebox(body) = block(width: 100%, fill: rgb("#f3f4f6"), radius: 4pt, inset: (x: 12pt, y: 10pt), body)
#let sidebarbox(body) = block(width: 100%, fill: rgb("#f3f4f6"), stroke: 0.75pt + rgb("#e5e7eb"), radius: 4pt, inset: (x: 12pt, y: 10pt), body)
#let sidebartitle(body) = align(center, text(weight: "bold", body))
#let verse(body) = block(inset: (left: 12pt), text(fill: rgb("#4b5563"), body))
#let attribution(body) = block(inset: (left: 12pt), above: 0.6em, text(size: 0.9em, fill: rgb("#4b5563"))[— #body])
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
#text(size: 22pt, weight: "bold")[#text("Paragraph metadata")]
]
#v(1em)

#text("See ")#link(<id-6e6f726d616c2d7269676874>)[#text("Normal ")#strong[#text("right")]#text(" title")]#text(", ")#link(<id-6e6f726d616c2d63656e746572>)[#text("Normal ")#strong[#text("center")]#text(" title")]#text(", ")#link(<id-6e6f726d616c2d6a757374696679>)[#text("Normal ")#strong[#text("justified")]#text(" title")]#text(", ")#link(<id-6e6f726d616c2d6c656674>)[#text("Normal ")#strong[#text("left")]#text(" title")]#text(", ")#link(<id-71756f7465>)[#text("Quote ")#strong[#text("title")]]#text(", ")#link(<id-7665727365>)[#text("Verse ")#strong[#text("title")]]#text(", ")#link(<id-6c69746572616c>)[#text("Literal ")#strong[#text("title")]]#text(", ")#link(<id-6c697374696e67>)[#text("Listing ")#strong[#text("title")]]#text(", ")#link(<id-736f75726365>)[#text("Source ")#strong[#text("title")]]#text(", ")#link(<id-6578616d706c65>)[#text("Example ")#strong[#text("title")]]#text(", and ")#link(<id-6162737472616374>)[#text("Abstract ")#strong[#text("title")]]#text(".")

#metadata(none) <id-6e6f726d616c2d7269676874>
#block(below: 0.5em)[#text(weight: "bold")[#text("Normal ")#strong[#text("right")]#text(" title")]]
#align(right)[
#text("The last applicable role aligns this paragraph to the right.")
]

#metadata(none) <id-6e6f726d616c2d63656e746572>
#block(below: 0.5em)[#text(weight: "bold")[#text("Normal ")#strong[#text("center")]#text(" title")]]
#align(center)[
#text("This paragraph is centred.")
]

#metadata(none) <id-6e6f726d616c2d6a757374696679>
#block(below: 0.5em)[#text(weight: "bold")[#text("Normal ")#strong[#text("justified")]#text(" title")]]
#par(justify: true)[
#text("This is a long paragraph with enough words to wrap onto more than one line so the explicit justify role has a visible effect in the generated PDF output.")
]

#metadata(none) <id-6e6f726d616c2d6c656674>
#block(below: 0.5em)[#text(weight: "bold")[#text("Normal ")#strong[#text("left")]#text(" title")]]
#align(left)[
#text("This paragraph is explicitly aligned to the left.")
]

#metadata(none) <id-71756f7465>
#block(below: 0.5em)[#text(weight: "bold")[#text("Quote ")#strong[#text("title")]]]
#blockquote[
#align(right)[
#text("The quote body is aligned to the right while its attribution keeps the default alignment.")
]
#text(style: "normal")[
#attribution[#text("Ada Lovelace")#text(", ")#text("Notes")]

]
]

#metadata(none) <id-7665727365>
#block(below: 0.5em)[#text(weight: "bold")[#text("Verse ")#strong[#text("title")]]]
#align(right)[
#verse[#text("The verse body is aligned to the right\nacross both of its lines.")]
]

#attribution[#text("Emily Dickinson")#text(", ")#text("Poem 1")]

#metadata(none) <id-6c69746572616c>
#block(below: 0.5em)[#text(weight: "bold")[#text("Literal ")#strong[#text("title")]]]
#raw(block: true, "literal paragraph ignores alignment roles")

#metadata(none) <id-6c697374696e67>
#block(below: 0.5em)[#text(weight: "bold")[#text("Listing ")#strong[#text("title")]]]
#raw(block: true, "listing paragraph ignores alignment roles")

#metadata(none) <id-736f75726365>
#block(below: 0.5em)[#text(weight: "bold")[#text("Source ")#strong[#text("title")]]]
#raw(block: true, "let alignment = \"ignored\";")

#metadata(none) <id-6578616d706c65>
#block(below: 0.5em)[#text(weight: "bold")[#text("Example 1. ")#text("Example ")#strong[#text("title")]]]
#align(right)[
#examplebox[
#text("The example body is aligned to the right.")
]
]

#metadata(none) <id-6162737472616374>
#abstracttitle[#text("Abstract ")#strong[#text("title")]]
#abstract[
#align(right)[
#text("The abstract body is aligned to the right.")
]
]

