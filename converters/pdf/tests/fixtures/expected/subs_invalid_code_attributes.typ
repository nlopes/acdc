#set document(
  title: "code-attributes(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[code-attributes(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("code-attributes(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("code-attributes - expand attributes beside invalid code delimiters")

#heading(level: 1)[#text("Description")] <id-5f6465736372697074696f6e>

#text("B01 A`café`B.")

#text("B02 A`café`.")

#text("B03 `café`B.")

#text("B04 é`café`日.")

#text("B05 ")#emph[#text("`café`")]#text(".")

#text("B06 :`café`; ;`café`; }`café`.")

#text("B07 `café `.")

#text("B08 ` café`.")

#text("B09 ")#text("`")#text("café`.")

#text("B10 ")#text("`")#text("café`.")

#text("B11 `café`\".")

#text("B12 ")#raw("\"café\"")#text(".")

#text("B13 A``B | A` `B | A`{unknown}`B.")

#text("B14 A`")#text("{word}")#text("`B | A`")#text("{word}")#text("`B.")

#text("B15 A")#raw("café")#text("B.")

#text("B16 ")#raw("")#text(" | ")#raw(" ")#text(" | ")#raw("café ")#text(".")

#text("B17 ")#raw("café`B then *Bold*")#text(".")

#text("B18 ")#raw("café ` then *Bold*")#text(".")

#text("B19 A`café` then ")#raw("café")#text(".")

#text("B20 A`+café+")#raw("B | A")#text("{word}")#text("`B.")

#text("B21 ")#raw("café ` then {word}")#text(".")

#text("B22 ")#raw("{word}")#text(" | ")#raw("{word}")#text(".")

#text("B23 ")#text(fill: rgb("#bf0000"))[#raw("café`X | [red]")]#text("café`.")

#text("B24 ")#strong[#text("A`café`B")]#text(" | ")#emph[#text("A`café`B")]#text(".")

#text("B25 A`café`B | ")#raw("café ` | `café")#text(".")

#text("B26 A`café`B | ")#raw("café ` | \\")#text("café`.")

#text("B27 A`{word}`B.")

#text("B28 A`café`B.")

#text("B29 A`café more`B.")

#text("B30 <tag>")#raw("café")#text(" | «")#raw("café")#text("» | (")#raw("café")#text(").")

#text("B31 ")#strong[#raw("café")]#text(" | ")#emph[#text("`café`")]#text(" | ")#text(fill: rgb("#bf0000"))[#raw("café")]#text(".")

#text("B32 A`{word}`B | `{word} ` | \\`{word}`.")

#text("B33 A`café`B.")

#text("B34 A`café`B.")

#text("B35 A`")#raw("B | A")#text(" `B.")

#text("B36 A``B | A` `B.")

#text("B37 ")#link("https://example.org")[#text("A`café`B")]#text(".")

#text("B38 ")#counter(footnote).update(0)#footnote[#text("A`café`B")]#text(".")

#text("B39 ")#metadata(none)<id-746172676574>#text("See ")#context link(query(<id-746172676574>).first().location())[#text("A`café`B")]#text(".")

#blocktitle[#text("A`café`B")]
#text("Example with a title.")

#heading(level: 1)[#text("B40 A`café`B")] <id-5f6234305f61636166c3a962>

#raw(block: true, "B41 A`café`B | café | `café `.")

#raw(block: true, "B42 A`café`B | café.")

#table(columns: (1fr), align: (left + top), stroke: none, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("B43 A`café`B.")

])

#text("B44 A`late`B.")
