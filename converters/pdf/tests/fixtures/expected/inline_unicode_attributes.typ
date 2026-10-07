#set document(
  title: "roles(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[roles(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("roles(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("roles - Unicode inline roles and IDs")

#heading(level: 1)[#text("Cases")] <id-5f6361736573>

#text("R01 ")#strong[#text("End")]#text(".")

#text("R02 ")#strong[#text("End")]#text(".")

#text("R03 ")#strong[#text("End")]#text(".")

#text("R04 ")#emph[#text("End")]#text(".")

#text("R05 ")#emph[#text("End")]#text(".")

#text("R06 ")#strong[#text("End")]#text(".")

#text("R07 ")#strong[#text("End")]#text(".")

#text("R08 ")#strong[#text("End")]#text(".")

#text("R09 ")#strong[#text("End")]#text(".")

#text("R10 ")#strong[#text("End")]#text(".")

#text("R11 ")#strong[#text("End")]#text(".")

#text("R12 ")#metadata(none)<id-636166c3a9>#strong[#text("End")]#text(".")

#text("R13 ")#strong[#text("End")]#text(".")

#text("R14 ")#strong[#text("End")]#text(".")

#text("R15 ")#strong[#text("End")]#text(".")

#text("R16 ")#strong[#text("End")]#text(".")

#text("R17 ")#strong[#text("End")]#text(".")

#text("R18 ")#strong[#text("End")]#text(".")

#text("R19 ")#strong[#text("End")]#text(".")

#text("R20 ")#raw("Code")#text(".")

#text("R21 ")#raw("Code")#text(".")

#text("R22 ")#text("End")#text(".")

#text("R23 ")#text("End")#text(".")

#text("R24 ")#super[#text("End")]#text(".")

#text("R25 ")#sub[#text("End")]#text(".")

#text("R26 ")#text("“")#text("End")#text("”")#text(".")

#text("R27 ")#text("‘")#text("End")#text("’")#text(".")

#text("R28 ")#metadata(none)<id-c3a9636f6c65>#strong[#text("End")]#text(".")

#text("R29 ")#metadata(none)<id-e69db1e4baac>#strong[#text("End")]#text(".")

#text("R30 ")#metadata(none)<id-63616665cc81>#strong[#text("End")]#text(".")

#text("R31 ")#metadata(none)<id-746f706963f09f9a80>#strong[#text("End")]#text(".")

#text("R32 ")#metadata(none)<id-73696d706c655f6964>#strong[#text("End")]#text(".")

#text("R33 ")#metadata(none)<id-6f746865722d6964>#strong[#text("End")]#text(".")

#text("R34 ")#strong[#text("End")]#text(".")

#text("R35 \\")#strong[#text("End")]#text(".")

#text("R36 ")#strong[#text("Before ")#emph[#text("Inside")]#text(" After")]#text(".")

#text("R37 ")#context link(query(<id-636166c3a9>).first().location())[#text("First")]#text(" ")#context link(query(<id-c3a9636f6c65>).first().location())[#text("Second")]#text(" ")#context link(query(<id-e69db1e4baac>).first().location())[#text("Third")]#text(" ")#context link(query(<id-63616665cc81>).first().location())[#text("Fourth")]#text(" ")#context link(query(<id-746f706963f09f9a80>).first().location())[#text("Fifth")]#text(".")

#text("R38 ")#strong[#text("End")]#text(".")

#text("R39 ")#strong[#text("End")]#text(".")

#text("R40 ")#strong[#text("End")]#text(".")

#text("R41 ")#strong[#text("End")]#text(".")

#text("R42 ")#strong[#text("End")]#text(".")

#text("R43 ")#strong[#text("End")]#text(".")

#text("R44 ")#strong[#text("End")]#text(".")

#text("R45 ")#strong[#text("End")]#text(".")

#text("R46 ")#strong[#text("End")]#text(".")

#text("R47 ")#strong[#text("End")]#text(".")

#text("R48 ")#strong[#text("End")]#text(".")
