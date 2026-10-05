#set document(
  title: "callouts(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[callouts(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("callouts(1)")]
]
#v(1em)

#heading(level: 1)[#text("NAME")] <id-5f6e616d65>

#text("callouts - test standalone callouts")

#heading(level: 1)[#text("DESCRIPTION")] <id-5f6465736372697074696f6e>

#heading(level: 2)[#text("Paragraph adjacent")] <id-5f7061726167726170685f61646a6163656e74>

#text("Prose. <1> Alpha. <2> Beta.")

#heading(level: 2)[#text("Paragraph blank")] <id-5f7061726167726170685f626c616e6b>

#text("Prose.")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Alpha.")],
[#text("(2)")], [#text("Beta.")],
)

#heading(level: 2)[#text("Empty item")] <id-5f656d7074795f6974656d>

#text("<1> <2> Beta.")

#heading(level: 2)[#text("Empty rest")] <id-5f656d7074795f72657374>

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Alpha. <2>")],
[#text("(2)")], [#text("Gamma.")],
)

#heading(level: 2)[#text("No space")] <id-5f6e6f5f7370616365>

#text("<1>Alpha. <.>Beta. <1>")

#heading(level: 2)[#text("Not markers")] <id-5f6e6f745f6d61726b657273>

#text("←1> Negative. <+1> Plus. <1.> Dot. <..> Dots.")

#heading(level: 2)[#text("Escaped")] <id-5f65736361706564>

#text("\\<1> Alpha. \\<.> Beta.")

#heading(level: 2)[#text("Inline")] <id-5f696e6c696e65>

#text("Inline <1> Alpha and <.> Beta.")

#heading(level: 2)[#text("Indented")] <id-5f696e64656e746564>

#raw(block: true, "<1> Alpha.\n<2> Beta.")

#heading(level: 2)[#text("Indented after listing")] <id-5f696e64656e7465645f61667465725f6c697374696e67>

#raw(block: true, "code (1) (2)")

#raw(block: true, "<1> Alpha.\n<2> Beta.")

#heading(level: 2)[#text("Tab indent")] <id-5f7461625f696e64656e74>

#text(" <1> Alpha. <2> Beta.")

#heading(level: 2)[#text("Tab separator")] <id-5f7461625f736570617261746f72>

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Alpha.")],
[#text("(2)")], [#text("Beta.")],
)

#heading(level: 2)[#text("Listing text")] <id-5f6c697374696e675f74657874>

#raw(block: true, "<1> Alpha.\n<2> Beta.")

#heading(level: 2)[#text("Literal style")] <id-5f6c69746572616c5f7374796c65>

#raw(block: true, "<1> Alpha.\n<2> Beta.")

#heading(level: 2)[#text("Source style")] <id-5f736f757263655f7374796c65>

#raw(block: true, "<1> Alpha.\n<2> Beta.")

#heading(level: 2)[#text("Verse style")] <id-5f76657273655f7374796c65>

#verse[#text("<1> Alpha.\n<2> Beta.")]

#heading(level: 2)[#text("Compound literal")] <id-5f636f6d706f756e645f6c69746572616c>

#text("Prose. <1> Alpha.")

#heading(level: 2)[#text("Table asciidoc")] <id-5f7461626c655f6173636969646f63>

#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Alpha.")],
[#text("(2)")], [#text("Beta.")],
)

])

#heading(level: 2)[#text("Table normal")] <id-5f7461626c655f6e6f726d616c>

#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("<1> Alpha. <2> Beta.")

])

#heading(level: 2)[#text("Blank-separated lists")] <id-5f626c616e6b5f7365706172617465645f6c69737473>

  - #text("Parent bullet.")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Separate after bullet.")],
)

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #text("Parent numbered item.")
]

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Separate after numbered item.")],
)

#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Parent description.")]
]

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Separate after description.")],
)
