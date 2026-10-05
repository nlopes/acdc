#set document(
  title: "Anchor macros",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Anchor macros]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Anchor macros")]
]
#v(1em)

#metadata(none)<id-636166c3a9>#text("Unicode. ")#metadata(none)<id-e69db1e4baac>#text("More Unicode.")

#metadata(none)<id-63616665cc81>#text("Combining mark. ")#metadata(none)<id-61e280bf62>#text("Connector punctuation.")

#metadata(none)<id-3a636f6c6f6e>#metadata(none)<id-5f756e646572>#metadata(none)<id-612d622e633a64>#text("Targets.")

#text("before")#metadata(none)<id-6a6f696e6564>#text("after.")

#text("Before")#metadata(none)<id-73686f727468616e64>#text("after.")

#metadata(none)<id-636f6d6d61>#metadata(none)<id-71756f746564>#metadata(none)<id-657175616c73>#text("Labels are text.")

#metadata(none)<id-627261636b6574>#metadata(none)<id-6f70656e>#text("Bracket labels.")

#text("See ")#context link(query(<id-636166c3a9>).first().location())[#text("Cafe")]#text(", ")#context link(query(<id-e69db1e4baac>).first().location())[#text("Tokyo")]#text(", ")#context link(query(<id-63616665cc81>).first().location())[#text("Decomposed")]#text(", ")#context link(query(<id-61e280bf62>).first().location())[#text("Connector")]#text(", ")#context link(query(<id-3a636f6c6f6e>).first().location())[#text("Colon")]#text(", ")#context link(query(<id-5f756e646572>).first().location())[#text("Underscore")]#text(", ")#context link(query(<id-612d622e633a64>).first().location())[#text("Punctuation")]#text(", ")#context link(query(<id-6a6f696e6564>).first().location())[#text("Joined")]#text(", ")#context link(query(<id-636f6d6d61>).first().location())[#text("Plain, label")]#text(", ")#context link(query(<id-71756f746564>).first().location())[#text("\"Quoted\"")]#text(", ")#context link(query(<id-657175616c73>).first().location())[#text("reftext=Literal,role=red")]#text(", ")#context link(query(<id-627261636b6574>).first().location())[#text("Close ] label")]#text(", ")#context link(query(<id-6f70656e>).first().location())[#text("Open [ bracket")]#text(".")

#text("anchor:escaped[Escaped]")#text(" \\")#text("anchor:twice[Twice]")#text(" \\anchor:1bad[Invalid].")

#text("anchor:[] anchor:1bad[Digit] anchor:bad/id[Slash] anchor:bad id[Space] anchor:bad@id[At] anchor:bad%id[Percent] anchor:bad,id[Comma] anchor:bad#id[Hash]")

#text("anchor:unfinished[Unclosed")

#text("anchor:wrapped[Two lines]End.")

#text("anchor:single[Single]")#text(" ")#text("anchor:double[Double]")#text(" ")#text("anchor:raw[Raw")#text("]")

#raw(block: true, "anchor:code[Code]Literal.")

#raw(block: true, "anchor:indented[Indented]Literal.")

#metadata(none)<id-746f706963f09f9a80>#text("Symbol in an ID. ")#metadata(none)<id-61c2b762>#text("Punctuation in an ID.")

#metadata(none)<id-61e2808d62>#text("Join control in an ID.")

#text("See ")#context link(query(<id-746f706963f09f9a80>).first().location())[#text("Rocket")]#text(", ")#context link(query(<id-61c2b762>).first().location())[#text("Middle dot")]#text(", and ")#context link(query(<id-61e2808d62>).first().location())[#text("Joiner")]#text(".")

#text("anchor:🚀bad[Symbol start] anchor:bad id[Nonbreaking space]Remain literal.")
