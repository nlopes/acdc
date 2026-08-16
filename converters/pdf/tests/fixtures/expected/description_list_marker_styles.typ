#set document(
  title: "Ordered and unordered description lists",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Ordered and unordered description lists]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Ordered and unordered description lists")]
]
#v(1em)

#heading(level: 1)[#text("Ordered style")] <id-5f6f7264657265645f7374796c65>

#metadata(none) <id-6f7264657265642d7465726d73>
#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#strong[#text("First term")#text(":")] #text("First definition.")]
  + #block(width: 100%)[#strong[#text("Question?")] #text("The punctuation is not duplicated.")]
  + #block(width: 100%)[#strong[#text("Third term")#text(":")] #text("The answer has ")#strong[#text("strong")]#text(" and ")#raw("code")#text(" text.")]
]

#text("See ")#link(<id-6f7264657265642d7465726d73>)[#text("Ordered terms")]#text(".")

#heading(level: 1)[#text("Unordered style")] <id-5f756e6f7264657265645f7374796c65>

#[
#set list(marker: box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))))
  - #block(width: 100%)[#strong[#text("Alpha")#text(":")] #text("Value A.")]
  - #block(width: 100%)[#strong[#text("Beta")#text(":")] #text("Value B.")]
]

#heading(level: 1)[#text("Stacked answer")] <id-5f737461636b65645f616e73776572>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#strong[#text("Stacked term")]#linebreak()
#text("The answer starts on the next line.")]
  + #block(width: 100%)[#strong[#text("Second stacked term")]#linebreak()
#text("Another answer.")]
]

#heading(level: 1)[#text("Stacked custom subject stop")] <id-5f737461636b65645f637573746f6d5f7375626a6563745f73746f70>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#strong[#text("Explicit stop")#text(";")]#linebreak()
#text("The answer is stacked after a semicolon.")]
]

#heading(level: 1)[#text("Custom subject stop")] <id-5f637573746f6d5f7375626a6563745f73746f70>

#[
#set list(marker: box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))))
  - #block(width: 100%)[#strong[#text("Custom stop")#text(";")] #text("The term ends with a semicolon.")]
  - #block(width: 100%)[#strong[#text("Already punctuated!")] #text("The existing punctuation remains.")]
]

#heading(level: 1)[#text("Ignored alignment role")] <id-5f69676e6f7265645f616c69676e6d656e745f726f6c65>

#[
#set list(marker: box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))))
  - #block(width: 100%)[#strong[#text("Left aligned")#text(":")] #text("The ordinary role does not change list alignment.")]
]

#heading(level: 1)[#text("Shared terms")] <id-5f7368617265645f7465726d73>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#strong[#text("Short name")#text(":")] #text("Both terms share this answer.")]
]

#heading(level: 1)[#text("Attached content")] <id-5f61747461636865645f636f6e74656e74>

#[
#set list(marker: box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))))
  - #block(width: 100%)[#strong[#text("Steps")#text(":")] #text("Start here.")

  - #text("First supporting point")
  - #text("Second supporting point")

]
  - #block(width: 100%)[#strong[#text("More detail")#text(":")]

#text("An attached paragraph.")

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#strong[#text("Nested term")#text(":")] #text("Nested answer.")]
]

]
]

#heading(level: 1)[#text("Ignored numbering controls")] <id-5f69676e6f7265645f6e756d626572696e675f636f6e74726f6c73>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#strong[#text("One")#text(":")] #text("Starts at one.")]
  + #block(width: 100%)[#strong[#text("Two")#text(":")] #text("Continues at two.")]
]
