#set document(
  title: "anchor-spacing(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[anchor-spacing(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("anchor-spacing(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("anchor-spacing - invisible targets preserve text spacing")

#heading(level: 1)[#text("Description")] <id-5f6465736372697074696f6e>

#text("P01 x")#metadata(none)<id-613031>#text("y.")

#text("P02 ")#link("https://example.org")[#text("Label")]#metadata(none)<id-613032>#text(".")

#text("P03 x ")#metadata(none)<id-613033>#text("y.")

#text("P04 x")#metadata(none)<id-613034>#text(" y.")

#text("P05 x ")#metadata(none)<id-613035>#text(" y.")

#text("P06 ")#metadata(none)<id-613036>#text("Start.")

#text("P07 End")#metadata(none)<id-613037>#text(".")

#text("P08 ")#metadata(none)<id-613038>#metadata(none)<id-613039>#text("Adjacent.")

#text("P09 ")#strong[#text("x")#metadata(none)<id-613130>#text("y")]#text(".")

#text("P10 ")#metadata(none)<id-613131>#raw("xy")#text(".")

#text("P11 ")#underline[#text("x")#metadata(none)<id-613132>#text("y")]#text(".")

#text("P12 x")#metadata(none)<id-613133>#text("y.")

#text("P13 Before ")#metadata(none)<id-7370616e>#strong[#text("Bold")]#text(" after.")

#text("P14 ")#text("anchor:escaped[]")#text(" and ")#text("anchor:raw[")#text("].")

#text("P15 café")#metadata(none)<id-613134>#text("fin.")

#text("P16 x")#metadata(none)<id-73686f7274>#text("y.")

#text("P17 x")#metadata(none)<id-77726170706564>#text(" y.")

  - #text("P18 x")#metadata(none)<id-62756c6c6574>#text("y.")

#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("P19 x")#metadata(none)<id-63656c6c>#text("y.")

])

#text("P20 Note")#counter(footnote).update(0)#footnote[#text("F20 x")#metadata(none)<id-6e6f7465>#text("y.")]#text(".")

#pagebreak(weak: true)

#text("See ")#context link(query(<id-613031>).first().location())[#text("One")]#text(", ")#context link(query(<id-613032>).first().location())[#text("Two")]#text(", ")#context link(query(<id-613033>).first().location())[#text("Three")]#text(", ")#context link(query(<id-613034>).first().location())[#text("Four")]#text(", ")#context link(query(<id-613035>).first().location())[#text("Five")]#text(", ")#context link(query(<id-613036>).first().location())[#text("Six")]#text(", ")#context link(query(<id-613037>).first().location())[#text("Seven")]#text(", ")#context link(query(<id-613038>).first().location())[#text("Eight")]#text(", ")#context link(query(<id-613039>).first().location())[#text("Nine")]#text(", ")#context link(query(<id-613130>).first().location())[#text("Ten")]#text(", ")#context link(query(<id-613131>).first().location())[#text("Eleven")]#text(", ")#context link(query(<id-613132>).first().location())[#text("Twelve")]#text(", ")#context link(query(<id-613133>).first().location())[#text("Thirteen")]#text(", ")#context link(query(<id-613134>).first().location())[#text("Fourteen")]#text(", ")#context link(query(<id-7370616e>).first().location())[#text("Span")]#text(", ")#context link(query(<id-73686f7274>).first().location())[#text("Short")]#text(", ")#context link(query(<id-77726170706564>).first().location())[#text("Wrapped")]#text(", ")#context link(query(<id-62756c6c6574>).first().location())[#text("Bullet")]#text(", ")#context link(query(<id-63656c6c>).first().location())[#text("Cell")]#text(", and ")#context link(query(<id-6e6f7465>).first().location())[#text("Note")]#text(".")
