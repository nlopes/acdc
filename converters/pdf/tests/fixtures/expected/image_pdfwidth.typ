#set document(
  title: "Image pdfwidth",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Image pdfwidth]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
  let resolved-width = if ratio != none { ratio * size.width } else if width != none { calc.min(width, size.width) } else { size.width }
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

#align(center)[
#text(size: 22pt, weight: "bold")[#text("Image pdfwidth")]
]
#v(1em)

#heading(level: 1)[#text("Block images")] <id-5f626c6f636b5f696d61676573>

#text("No local dimensions; the document attribute does not provide a default:")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Document attribute only")

#text("Generic positional width, 40px:")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Generic positional", width: 30pt)

#text("Generic named width, 40px:")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Generic named", width: 30pt)

#text("PDF width, 40pt:")

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "PDF points", width: 40pt)]

#text("PDF width overrides generic width:")

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "Precedence", width: 60pt)]

#text("Generic percentage, capped at the content width:")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Generic percentage", ratio: 1)

#text("PDF percentage, allowed to exceed the content width:")

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "PDF percentage", width: 200%)]

#text("Generic unit-suffixed width is ignored:")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Generic units")

#text("PDF width accepts CSS pixels:")

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "PDF pixels", width: 30pt)]

#text("PDF width accepts physical units:")

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "PDF inches", width: 72pt)]

#text("PDF width can scale the intrinsic width:")

#block(width: 100%, radius: 4pt, clip: false)[#scale(x: 50%, y: 50%, reflow: true, image("/images/de454d7e4e1cfda7.svg", alt: "Intrinsic ratio"))]

#text("PDF width can use the page width:")

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "Viewport ratio", width: 297.638pt)]

#text("An invalid PDF width still overrides the generic width:")

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "Invalid PDF width", width: 0pt)]

#heading(level: 1)[#text("Inline images")] <id-5f696e6c696e655f696d61676573>

#text("No local dimensions: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Document attribute only"))#text(" after.")

#text("Generic positional width: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Generic positional", width: 30pt))#text(" after.")

#text("Generic named width: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Generic named", width: 30pt))#text(" after.")

#text("PDF width: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "PDF points", width: 40pt))#text(" after.")

#text("PDF width overrides generic width: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Precedence", width: 60pt))#text(" after.")

#text("Generic oversized percentage: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Generic percentage", width: 100%))#text(" after.")

#text("PDF oversized percentage: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "PDF percentage", width: 100%))#text(" after.")

#text("Generic unit-suffixed width: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Generic units"))#text(" after.")

#text("PDF width in CSS pixels: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "PDF pixels", width: 30pt))#text(" after.")

#text("PDF width in inches: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "PDF inches", width: 72pt))#text(" after.")

#text("PDF intrinsic ratio: before ")#box(scale(x: 50%, y: 50%, reflow: true, image("/images/de454d7e4e1cfda7.svg", alt: "Intrinsic ratio")))#text(" after.")

#text("Inline PDF viewport units fall back to points: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Viewport fallback", width: 50pt))#text(" after.")

#text("Invalid PDF width: before ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Invalid PDF width", width: 0pt))#text(" after.")
