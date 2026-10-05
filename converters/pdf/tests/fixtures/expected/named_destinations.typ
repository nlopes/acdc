#set document(
  title: "named-targets(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[named-targets(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#metadata(none) <id-646f63756d656e742d746f70>
#set page(numbering: "1")
#counter(page).update(1)
#align(center)[
#text(size: 22pt, weight: "bold")[#text("named-targets(1)")]
]
#v(1em)

#heading(outlined: false, bookmarked: false)[#text("Table of Contents")]
#let _acdc_toc_entry(target, depth, body) = context {
  link(
    target,
    pad(
      left: depth * 1.25em,
      grid(
        columns: (auto, 1fr, auto),
        column-gutter: 0.5em,
        body,
        repeat[.],
        counter(page).display(at: target),
      ),
    ),
  )
}
#_acdc_toc_entry(<id-5f6e616d65>, 0, [#text("Name")])
#_acdc_toc_entry(<id-63686170746572>, 0, [#text("Description")])
#_acdc_toc_entry(<id-5f6269626c696f677261706879>, 0, [#text("Bibliography")])
#pagebreak()

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("named-targets - public PDF destinations")

#heading(level: 1)[#text("Description")] <id-63686170746572>

#text("P01 ")#metadata(none)<id-696e6c696e65>#text("Text and ")#metadata(none)<id-73686f727468616e64>#text("short text.")

#text("P02 ")#metadata(none)<id-626f6c64>#strong[#text("Bold")]#text(", ")#metadata(none)<id-6974616c6963>#emph[#text("Italic")]#text(", ")#metadata(none)<id-6d6f6e6f>#raw("Mono")#text(", ")#metadata(none)<id-6d61726b>#text("Mark")#text(", ")#metadata(none)<id-737562>#sub[#text("Sub")]#text(", ")#metadata(none)<id-7375706572>#super[#text("Sup")]#text(", ")#metadata(none)<id-646f75626c65>#text("“")#text("Double")#text("”")#text(", and ")#metadata(none)<id-73696e676c65>#text("‘")#text("Single")#text("’")#text(".")

#text("P03 ")#metadata(none)<id-656d707479>#strong[]#text(" Empty and ")#raw("x")#metadata(none)<id-636f6465>#raw("y")#text(".")

#text("P04 ")#metadata(none)<id-636166c3a9>#text("Accent and ")#metadata(none)<id-746f706963f09f9a80>#text("Rocket.")

#text("P05 ")#metadata(none)<id-613a62>#text("Colon and ")#metadata(none)<id-69642d3633363836313730373436353732>#text("Encoded-name collision.")

#text("P06 ")#metadata(none)<id-75726c2d6964>#link("https://example.org")[#text("URL")]#text(", ")#metadata(none)<id-6c696e6b2d6964>#link("https://example.org")[#text("Link")]#text(", and ")#metadata(none)<id-6d61696c2d6964>#link("mailto:person@example.org")[#text("Mail")]#text(".")

#metadata(none) <id-706172616772617068>
#text("P07 Paragraph target.")

#metadata(none) <id-6c697374696e67>
#raw(block: true, "P08 Listing target.")

#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("P09 ")#metadata(none)<id-63656c6c>#text("Cell target.")

])

  - #text("P10 ")#metadata(none)<id-6974656d>#text("List target.")

#text("P11 ")#metadata(none)<id-6475706c6963617465>#text("First duplicate.")

#text("P12 ")#text("anchor:escaped[]")#text("Literal and ")#text("anchor:raw[")#text("] raw text.")

#metadata(none) <id-6469736372657465>
#heading(level: 2, outlined: false)[#text("Discrete heading")]

#text("P13 Discrete body.")

#heading(level: 1)[#text("Bibliography")] <id-5f6269626c696f677261706879>

#[
#set list(marker: box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280"))))
  - #block(width: 100%)[#metadata(none)<id-626f6f6b>#text("[book]")#text(" P14 Book target.")]
]

#pagebreak(weak: true)

#text("P15 ")#text("Second duplicate.")

#text("See ")#context link(query(<id-646f63756d656e742d746f70>).first().location())[#text("Document")]#text(", ")#context link(query(<id-63686170746572>).first().location())[#text("Chapter")]#text(", ")#context link(query(<id-696e6c696e65>).first().location())[#text("Inline")]#text(", ")#context link(query(<id-636166c3a9>).first().location())[#text("Accent")]#text(", ")#context link(query(<id-746f706963f09f9a80>).first().location())[#text("Rocket")]#text(", ")#context link(query(<id-69642d3633363836313730373436353732>).first().location())[#text("Collision")]#text(", ")#context link(query(<id-626f6f6b>).first().location())[#text("Book")]#text(", and ")#context link(query(<id-6475706c6963617465>).first().location())[#text("First")]#text(".")
