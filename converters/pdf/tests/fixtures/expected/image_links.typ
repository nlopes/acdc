#set document(
  title: "Image links",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Image links]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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

#align(center)[
#text(size: 22pt, weight: "bold")[#text("Image links")]
]
#v(1em)

#heading(level: 1)[#text("Link target")] <id-6c6f63616c2d746172676574>

#heading(level: 2)[#text("Block images")] <id-5f626c6f636b5f696d61676573>

#docimage("/images/de454d7e4e1cfda7.svg", alt: "External link", destination: "https://example.com/image?one=1&two=2")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Email link", destination: "mailto:docs@example.com")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Document fragment", destination: "#local-target")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Plain target", destination: "local-target")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Named none", destination: "none")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Empty link")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Malformed link", destination: "not a valid uri")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Substituted link", destination: "https://example.com/from-attribute")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Positional value", width: 75pt)

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Block attribute link", destination: "https://example.com/block-attribute")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Macro attribute wins", destination: "https://example.com/macro")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Repeated link", destination: "https://example.com/last")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Empty link")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Browser controls", destination: "https://example.com/controls")

#docimage("/images/de454d7e4e1cfda7.svg", alt: "Linked width", width: 75pt, destination: "https://example.com/width")

#block(width: 100%, radius: 4pt, clip: true)[#align(right)[#link("https://example.com/positioned")[#image("/images/de454d7e4e1cfda7.svg", alt: "Positioned link", width: 75pt)]]]

#block(width: 100%, breakable: false)[
#docimage("/images/de454d7e4e1cfda7.svg", alt: "Captioned link", destination: "https://example.com/captioned")
#imagecaption[#text("Figure 1. ")#text("Caption outside link")]
]

#link("https://example.com/missing")[#text("[Missing linked image]")]#text(" | missing.svg")

#heading(level: 2)[#text("Inline images")] <id-5f696e6c696e655f696d61676573>

#text("External: ")#box(link("https://example.com/inline?one=1&two=2")[#image("/images/de454d7e4e1cfda7.svg", alt: "External link")])

#text("Email: ")#box(link("mailto:docs@example.com")[#image("/images/de454d7e4e1cfda7.svg", alt: "Email link")])

#text("Document fragment: ")#box(link("#local-target")[#image("/images/de454d7e4e1cfda7.svg", alt: "Document fragment")])

#text("Plain target: ")#box(link("local-target")[#image("/images/de454d7e4e1cfda7.svg", alt: "Plain target")])

#text("Empty link: ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Empty inline link"))

#text("Repeated link: ")#box(link("https://example.com/last")[#image("/images/de454d7e4e1cfda7.svg", alt: "Repeated inline link")])

#text("Substituted link: ")#box(link("https://example.com/from-attribute")[#image("/images/de454d7e4e1cfda7.svg", alt: "Substituted inline link")])

#text("Positional value: ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Positional inline link", width: 75pt))

#text("Browser controls: ")#box(link("https://example.com/inline-controls")[#image("/images/de454d7e4e1cfda7.svg", alt: "Browser controls")])

#text("Outer link: ")#link("https://example.com/outer")[#box(image("/images/de454d7e4e1cfda7.svg", alt: "Outer inline link"))]

#text("Inner link precedence: ")#box(link("https://example.com/inner")[#image("/images/de454d7e4e1cfda7.svg", alt: "Inner inline link")])

#text("Named none: ")#box(link("none")[#image("/images/de454d7e4e1cfda7.svg", alt: "Named none")])

#text("Malformed link: ")#box(link("not a valid uri")[#image("/images/de454d7e4e1cfda7.svg", alt: "Malformed link")])

#text("Sized link: ")#box(link("https://example.com/sized")[#image("/images/de454d7e4e1cfda7.svg", alt: "Sized link", width: 75pt)])

#text("Scaled link: ")#box(link("https://example.com/scaled")[#scale(x: 50%, y: 50%, reflow: true, image("/images/de454d7e4e1cfda7.svg", alt: "Scaled link"))])

#link("https://example.com/missing-inline")[#text("Missing linked inline image")]
