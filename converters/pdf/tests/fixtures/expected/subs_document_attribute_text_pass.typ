#set document(
  title: "attribute-text-pass(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[attribute-text-pass(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("attribute-text-pass(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("attribute-text-pass - text-only attribute substitutions")

#heading(level: 1)[#text("Description")] <id-5f6465736372697074696f6e>

#text("P01 Early *Bold*")

#text("P02 ")#text("<")#text("x")#text(">")#text(" ")#text("&")#text(" {name} *Bold*")

#text("P03 ")#text("<")#text("raw")#text(">")#text(" ")#text("&")

#text("P04 ")#text("<raw>")#text(" ")#text("&")

#text("P05 Early ")#text("<")#text("x")#text(">")#text(" ")#text("&")

#text("P06 {name} ")#text("&")

#text("P07 {name} {name} {name} \\{name}")

#text("P08 pass:[Early]")

#text("P09 Before after.")

#text("P10 Before {not-set} after.")

#text("P11 Early a\\]b")

#text("P12 prefix ")#text("<")#text("x")#text(">")#text(" ")#text("&")#text(" {name} *Bold* / Early *Bold*")

#text("P13 ")#text("<")#text("ordinary")#text(">")#text(" ")#text("&")

#text("P14 ")#text("<")#text("ordinary")#text(">")#text(" ")#text("&")

#text("P15 ")#text("<")#text("ordinary")#text(">")#text(" ")#text("&")

#text("P16 ")#text("<raw>")#text(" ")#text("&")

#text("P17 ")#text("<")#text("x")#text(">")#text(" ")#text("&")#text(" {name} | ")#text("<")#text("x")#text(">")#text(" ")#text("&")#text(" {name}")

#text("P18 ")#text("©")#text(" ")#text("&")

#text("P19 café ")#text("<")#text("α")#text(">")#text(" ")#text("&")

#text("P20 ")#text("<>")#text("&")

#text("P21 ")#text("<")#text("&")#text(">")

#text("P22 pass:c[Early]")

#raw(block: true, "C01 Early *Bold*\nC02 <x> & {name} *Bold*\nC03 <raw> &\nC05 Early <x> &\nC07 {name} {name} {name} \\{name}\nC08 pass:[Early]\nC09 Before  after.\nC10 Before {not-set} after.\nC11 Early a\\]b\nC12 prefix <x> & {name} *Bold* / Early *Bold*\nC13 <ordinary> &\nC14 &lt;ordinary&gt; &amp;\nC15 <ordinary> &\nC17 <x> & {name} | <x> & {name}\nC18 &#169; &amp;\nC19 café <α> &\nC22 pass:c[Early]")

#raw(block: true, "C06 {name} &")

#raw(block: true, "N01 <x> & {name} *Bold*\nN02 &lt;ordinary&gt; &amp;\nN03 &#169; &amp;")

#raw(block: true, "R01 &lt;x&gt; &amp; {name} *Bold*\nR02 &amp;lt;ordinary&amp;gt; &amp;amp;\nR03 &amp;#169; &amp;amp;")

#raw(block: true, "O01 &lt;x&gt; &amp; {name} *Bold*\nO02 &lt;ordinary&gt; &amp;\nO03 &#169; &amp;")

#raw(block: true, "O04 <x> & {name} *Bold*\nO05 &lt;ordinary&gt; &amp;\nO06 &#169; &amp;")

#raw(block: true, "D01 {a} | {c}")

#text("I01 ")#text("&#169;")

#text("I02 ")#text("©")

#text("I03 ")#text("&#169;")
