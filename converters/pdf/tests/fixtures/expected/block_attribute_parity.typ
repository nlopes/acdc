#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Block Attribute Parity]], footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Block Attribute Parity")]
]
#v(1em)

#heading(level: 1)[#text("Delimiters and slots")] <id-5f64656c696d69746572735f616e645f736c6f7473>

#blockquote[
#text("Balanced brackets.")

]

#attribution[#text("A [x] B")#text(", ")#text("Notes")]

#blockquote[
#text("Unmatched opening bracket.")

]

#attribution[#text("A [x")#text(", ")#text("Notes")]

#blockquote[
#text("Extra closing bracket.")

]

#attribution[#text("A ] B")#text(", ")#text("Notes")]

#blockquote[
#text("Unquoted comma.")

]

#attribution[#text("https://example.com[Someone")#text(", ")#text("Part 1]")]

#blockquote[
#text("Quoted comma.")

]

#attribution[#text("https://example.com[Someone, Part 1]")#text(", ")#text("Final")]

#blockquote[
#text("Quote-delimited slot.")

]

#attribution[#text("Quoted author")#text(", ")#text("Adjacent citation")]

#blockquote[
#text("Empty slot.")

]

#blockquote[
#text("Named slot.")

]

#blockquote[
#text("Named author.")

]

#attribution[#text("Named Author")#text(", ")#text("Positional Citation")]

#blockquote[
#text("Named citation.")

]

#attribution[#text("Positional Author")#text(", ")#text("Named Citation")]

#blockquote[
#text("Both fields named.")

]

#attribution[#text("Named Author")#text(", ")#text("Named Citation")]

#blockquote[
#text("Unset None value.")

]

#blockquote[
#text("Quoted None value.")

]

#attribution[#text("None")#text(", ")#text("Quoted Literal")]

#blockquote[
#text("Positional None value.")

]

#attribution[#text("None")#text(", ")#text("Positional Literal")]

#blockquote[
#text("Spaced equals signs.")

]

#attribution[#text("Comma, Author")#text(", ")#text("Work")]

#blockquote[
#text("Expanded slots.")

]

#attribution[#text("Ada Lovelace")#text(", ")#text("Notes")]

#heading(level: 1)[#text("Stacked lists")] <id-5f737461636b65645f6c69737473>

#blockquote[
#text("Named-only stack.")

]

#attribution[#text("Original Author")#text(", ")#text("Named Citation")]

#blockquote[
#text("Positional overlay stack.")

]

#attribution[#text("Replacement Author")#text(", ")#text("Original Citation")]

#verse[#text("Style replacement stack.")]

#attribution[#text("Replacement Poet")#text(", ")#text("Replacement Poem")]

#heading(level: 1)[#text("Substitutions")] <id-5f737562737469747574696f6e73>

#blockquote[
#text("Unquoted values.")

]

#attribution[#text("Ada *Lovelace*")#text(", ")#text("`Notes`")]

#blockquote[
#text("Double-quoted values.")

]

#attribution[#text("Grace *Hopper*")#text(", ")#text("`Compiler`")]

#blockquote[
#text("Single-quoted formatting.")

]

#attribution[#text("Margaret ")#strong[#text("Hamilton")]#text(", ")#raw("Apollo")]

#blockquote[
#text("Escaped single quote.")

]

#attribution[#text("A’B")#text(", ")#text("Notes")]

#blockquote[
#text("Unquoted macros.")

]

#attribution[#text("https://example.com[Literal Author]")#text(", ")#text("https://example.org[Literal Work]")]

#blockquote[
#text("Single-quoted macros.")

]

#attribution[#link("https://example.com")[#text("Linked Author")]#text(", ")#link("https://example.org")[#text("Linked Work")]]

#blockquote[
#text("Named substitutions.")

]

#attribution[#text("Named ")#strong[#text("Author")]#text(", ")#raw("Named Work")]

#heading(level: 1)[#text("Quote and verse forms")] <id-5f71756f74655f616e645f76657273655f666f726d73>

#blockquote[
#text("Styled quote paragraph.")
]

#attribution[#text("Paragraph ")#strong[#text("Author")]#text(", ")#raw("Paragraph Work")]

#blockquote[
#text("Citation-only quote paragraph.")
]

#verse[#text("Styled verse paragraph.")]

#attribution[#text("Paragraph ")#strong[#text("Poet")]#text(", ")#raw("Paragraph Poem")]

#verse[#text("Delimited verse block.")]

#attribution[#text("Block ")#strong[#text("Poet")]#text(", ")#raw("Block Poem")]

#heading(level: 1)[#text("Styles and context slots")] <id-5f7374796c65735f616e645f636f6e746578745f736c6f7473>

#metadata(none) <id-7374796c65642d71756f7465>
#blockquote[
#text("Adjacent shorthand.")

]

#attribution[#text("Ada")]

#text("Spaces disable shorthand.")

#text("Bracketed unknown style.")

#text("[[not an anchor]]\nDouble-bracket anchor syntax with spaces remains text.")

#metadata(none) <id-6f6e6c792d6964>
#text("Shorthand-only metadata.")

#raw(block: true, "fn main() {}")

#raw(block: true, "slot three is not a language")

#raw(block: true, "empty language slot")

#raw(block: true, "puts \"styled source paragraph\"")

#block[#text("sqrt(4) = 2")]

