#set document(
  title: "List pagination",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[List pagination]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("List pagination")]
]
#v(1em)

#heading(level: 1)[#text("Continued block across pages")] <id-5f636f6e74696e7565645f626c6f636b5f6163726f73735f7061676573>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#text("The owner starts before the continued block.")

#raw(block: true, "continued line 01\ncontinued line 02\ncontinued line 03\ncontinued line 04\ncontinued line 05\ncontinued line 06\ncontinued line 07\ncontinued line 08\ncontinued line 09\ncontinued line 10\ncontinued line 11\ncontinued line 12\ncontinued line 13\ncontinued line 14\ncontinued line 15\ncontinued line 16\ncontinued line 17\ncontinued line 18\ncontinued line 19\ncontinued line 20\ncontinued line 21\ncontinued line 22\ncontinued line 23\ncontinued line 24\ncontinued line 25\ncontinued line 26\ncontinued line 27\ncontinued line 28\ncontinued line 29\ncontinued line 30\ncontinued line 31\ncontinued line 32\ncontinued line 33\ncontinued line 34\ncontinued line 35\ncontinued line 36\ncontinued line 37\ncontinued line 38\ncontinued line 39\ncontinued line 40\ncontinued line 41\ncontinued line 42\ncontinued line 43\ncontinued line 44\ncontinued line 45\ncontinued line 46\ncontinued line 47\ncontinued line 48\ncontinued line 49\ncontinued line 50\ncontinued line 51\ncontinued line 52\ncontinued line 53\ncontinued line 54\ncontinued line 55\ncontinued line 56\ncontinued line 57\ncontinued line 58\ncontinued line 59\ncontinued line 60\ncontinued line 61\ncontinued line 62\ncontinued line 63\ncontinued line 64\ncontinued line 65\ncontinued line 66\ncontinued line 67\ncontinued line 68\ncontinued line 69\ncontinued line 70")

#text("The owner continues after the page-spanning block.")

      - #block(width: 100%)[#text("An unordered child remains inside the ordered item.")

          - #block(width: 100%)[#text("Its unordered grandchild keeps the deeper indentation.")

            #[
            #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
              + #text("An ordered child follows inside the same continuation.")
            ]

          ]

      ]

  ]
  + #text("The following ordered sibling remains outside the continuation.")
]

#heading(level: 1)[#text("Unordered list across pages")] <id-5f756e6f7264657265645f6c6973745f6163726f73735f7061676573>

  - #text("Unordered item 01.")
  - #text("Unordered item 02.")
  - #text("Unordered item 03.")
  - #text("Unordered item 04.")
  - #text("Unordered item 05.")
  - #text("Unordered item 06.")
  - #text("Unordered item 07.")
  - #text("Unordered item 08.")
  - #text("Unordered item 09.")
  - #text("Unordered item 10.")
  - #text("Unordered item 11.")
  - #text("Unordered item 12.")
  - #text("Unordered item 13.")
  - #text("Unordered item 14.")
  - #text("Unordered item 15.")
  - #text("Unordered item 16.")
  - #text("Unordered item 17.")
  - #text("Unordered item 18.")
  - #text("Unordered item 19.")
  - #text("Unordered item 20.")
  - #text("Unordered item 21.")
  - #text("Unordered item 22.")
  - #text("Unordered item 23.")
  - #text("Unordered item 24.")
  - #text("Unordered item 25.")
  - #text("Unordered item 26.")
  - #text("Unordered item 27.")
  - #text("Unordered item 28.")
  - #text("Unordered item 29.")
  - #text("Unordered item 30.")
  - #text("Unordered item 31.")
  - #text("Unordered item 32.")
  - #text("Unordered item 33.")
  - #text("Unordered item 34.")
  - #text("Unordered item 35.")
  - #text("Unordered item 36.")
  - #text("Unordered item 37.")
  - #text("Unordered item 38.")
  - #text("Unordered item 39.")
  - #text("Unordered item 40.")

#heading(level: 1)[#text("Description list across pages")] <id-5f6465736372697074696f6e5f6c6973745f6163726f73735f7061676573>

#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 01")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 01.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 02")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 02.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 03")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 03.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 04")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 04.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 05")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 05.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 06")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 06.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 07")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 07.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 08")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 08.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 09")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 09.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 10")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 10.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 11")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 11.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 12")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 12.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 13")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 13.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 14")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 14.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 15")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 15.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 16")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 16.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 17")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 17.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 18")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 18.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 19")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 19.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 20")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 20.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 21")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 21.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 22")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 22.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 23")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 23.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 24")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 24.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 25")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 25.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 26")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 26.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 27")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 27.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 28")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 28.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 29")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 29.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Term 30")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Description 30.")]
]
