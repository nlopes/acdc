#set document(
  title: "Callout list children",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Callout list children]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Callout list children")]
]
#v(1em)

#heading(level: 1)[#text("Bullet children")] <id-5f62756c6c65745f6368696c6472656e>

#raw(block: true, "code (1) (2)")

#metadata(none) <id-62756c6c65742d63616c6c6f757473>
#blocktitle[#text("Annotations")]
#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("First bullet parent.")

#metadata(none) <id-62756c6c65742d6368696c64>
  - #block(width: 100%)[#text("Nested bullet.")

      - #text("Deep bullet.")

  ]

],
[#text("(2)")], [#text("Second bullet parent.")],
)

#text("See ")#context link(query(<id-62756c6c65742d6368696c64>).first().location())[#text("Child list")]#text(" and ")#context link(query(<id-62756c6c65742d63616c6c6f757473>).first().location())[#text("Annotations")]#text(".")

#heading(level: 1)[#text("Ordered children after a blank line")] <id-5f6f7264657265645f6368696c6472656e5f61667465725f615f626c616e6b5f6c696e65>

#raw(block: true, "code (1) (2)")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("First ordered parent.")

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#text("Nested ordered.")

    #[
    #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("a.", ..numbers.pos())))
      + #text("Deep ordered.")
    ]

  ]
]

],
[#text("(2)")], [#text("Second ordered parent.")],
)

#heading(level: 1)[#text("Description children")] <id-5f6465736372697074696f6e5f6368696c6472656e>

#raw(block: true, "code (1) (2)")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("First description parent.")

#metadata(none) <id-6465736372697074696f6e2d6368696c64>
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Nested description.")]
]

],
[#text("(2)")], [#text("Second description parent.")],
)

#text("See ")#context link(query(<id-6465736372697074696f6e2d6368696c64>).first().location())[#text("Description list")]#text(".")

#heading(level: 1)[#text("Explicit children")] <id-5f6578706c696369745f6368696c6472656e>

#raw(block: true, "code (1) (2)")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("First explicit parent.")

#metadata(none) <id-6578706c696369742d6368696c64>
  - #text("Explicit bullet.")

],
[#text("(2)")], [#text("Second explicit parent.")

#text("Attached paragraph.")

],
)

#text("See ")#context link(query(<id-6578706c696369742d6368696c64>).first().location())[#text("Explicit list")]#text(".")

#heading(level: 1)[#text("Explicit paragraph returns to parent")] <id-5f6578706c696369745f7061726167726170685f72657475726e735f746f5f706172656e74>

#raw(block: true, "code (1) (2)")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("First paragraph parent.")

#text("Paragraph attached to first.")

],
[#text("(2)")], [#text("Second paragraph parent.")],
)

#heading(level: 1)[#text("Inline registration order")] <id-5f696e6c696e655f726567697374726174696f6e5f6f72646572>

#raw(block: true, "code (1) (2)")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Parent ")#counter(footnote).update(0)#footnote[#text("Parent note.")]#text(" and ")#metadata(none)<__indexterm-1>#text("Parent term")#text(".")

  - #text("Child ")#counter(footnote).update(1)#footnote[#text("Child note.")]#text(" and ")#metadata(none)<__indexterm-2>#text("Child term")#text(".")

],
[#text("(2)")], [#text("Last ")#counter(footnote).update(2)#footnote[#text("Last note.")]#text(".")],
)

#heading(level: 1)[#text("Callout markers before description delimiters")] <id-5f63616c6c6f75745f6d61726b6572735f6265666f72655f6465736372697074696f6e5f64656c696d6974657273>

#raw(block: true, "code (1) (2)")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("First marker parent.")],
[#text("(2)")], [#text("Name:: value remains callout text.")],
)

#heading(level: 1)[#text("Separate lists")] <id-5f73657061726174655f6c69737473>

#raw(block: true, "code (1) (2)")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("First separated parent.")],
[#text("(2)")], [#text("Second separated parent.")],
)

#metadata(none) <id-73657061726174652d6368696c64>
  - #text("Separate list.")

#text("See ")#context link(query(<id-73657061726174652d6368696c64>).first().location())[#text("Separate list")]#text(".")

#heading(level: 1)[#text("Blank metadata separates the child")] <id-5f626c616e6b5f6d657461646174615f7365706172617465735f7468655f6368696c64>

#raw(block: true, "code (1)")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Before separate child.")],
)

#metadata(none) <id-626c616e6b2d6368696c64>
  - #text("Separate child after blank metadata.")

#text("See ")#context link(query(<id-626c616e6b2d6368696c64>).first().location())[#text("Separate child")]#text(".")

#heading(level: 1)[#text("Literal controls")] <id-5f6c69746572616c5f636f6e74726f6c73>

  - #block(width: 100%)[#text("Ordinary bullet.")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Literal continuation outside a callout.")],
)

  ]

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#text("Ordinary ordered item.")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Literal ordered continuation.")],
)

  ]
]

#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Ordinary description.")

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("Literal description continuation.")],
)

]
]

#text("Escaped \\<2> marker.")
