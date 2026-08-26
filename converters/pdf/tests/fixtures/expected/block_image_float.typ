#set document(
  title: "Block image floats",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Block image floats]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Block image floats")]
]
#v(1em)

#heading(level: 1)[#text("Left float")] <id-5f6c6566745f666c6f6174>

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Left float", width: 150pt)

#text("The paragraph after a left float contains enough text to make its position relative to the image easy to inspect. It continues across several phrases and extends beyond the height of the image.")

#heading(level: 1)[#text("Right float")] <id-5f72696768745f666c6f6174>

#block(width: 100%, radius: 4pt, clip: true)[#align(right)[#image("/images/de454d7e4e1cfda7.svg", alt: "Right float", width: 150pt)]]

#text("The paragraph after a right float contains enough text to make its position relative to the image easy to inspect. It continues across several phrases and extends beyond the height of the image.")

#heading(level: 1)[#text("Consecutive paragraphs")] <id-5f636f6e73656375746976655f70617261677261706873>

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Consecutive paragraphs", width: 210pt)

#text("The first paragraph contains enough text to occupy part of the image height.")

#text("The second consecutive paragraph makes it possible to inspect how long the image affects following paragraph layout.")

#heading(level: 1)[#text("Non-paragraph boundary")] <id-5f6e6f6e5f7061726167726170685f626f756e64617279>

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Listing boundary", width: 150pt)

#raw(block: true, "This listing starts below the image.")

#heading(level: 1)[#text("Full-width image")] <id-5f66756c6c5f77696474685f696d616765>

#block(width: 100%, radius: 4pt, clip: true)[#align(right)[#image("/images/de454d7e4e1cfda7.svg", alt: "Full width", width: 100%)]]

#text("This paragraph starts below an image that occupies the full available width.")

#heading(level: 1)[#text("Float precedence")] <id-5f666c6f61745f707265636564656e6365>

#block(width: 100%, radius: 4pt, clip: true)[#align(right)[#image("/images/de454d7e4e1cfda7.svg", alt: "Float overrides align", width: 150pt)]]

#text("The valid right float takes precedence over the conflicting left alignment.")

#heading(level: 1)[#text("Invalid float fallback")] <id-5f696e76616c69645f666c6f61745f66616c6c6261636b>

#block(width: 100%, radius: 4pt, clip: true)[#align(center)[#image("/images/de454d7e4e1cfda7.svg", alt: "Invalid float", width: 150pt)]]

#text("An invalid float falls through to the valid centered alignment.")

#heading(level: 1)[#text("Inline control")] <id-5f696e6c696e655f636f6e74726f6c>

#text("Before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Inline float", width: 30pt))#text(" after.")
