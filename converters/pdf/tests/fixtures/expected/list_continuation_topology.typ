#set document(
  title: "List continuation and topology",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[List continuation and topology]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("List continuation and topology")]
]
#v(1em)

#heading(level: 1)[#text("Repeated description-list continuations")] <id-5f72657065617465645f6465736372697074696f6e5f6c6973745f636f6e74696e756174696f6e73>

#layout(size => {
let term-width = calc.min(calc.max(0pt,
measure([#text(weight: "bold")[#text("Owner")]]).width,
measure([#text(weight: "bold")[#text("Sibling")]]).width,
), size.width * 50%)
grid(columns: (term-width, 1fr), column-gutter: 20pt, row-gutter: 0.5em, align: top,
[#text(weight: "bold")[#text("Owner")]], [#text("Principal description text.")

#text("An attached paragraph owned by Owner.")

#callout("note")[
#text("An attached admonition owned by Owner.")

]

#raw(block: true, "fn owned() {\n    println!(\"owner\");\n}")

],
[#text(weight: "bold")[#text("Sibling")]], [#text("This sibling remains a separate row.")],
)
})

#heading(level: 1)[#text("Nested description lists")] <id-5f6e65737465645f6465736372697074696f6e5f6c69737473>

#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Root one")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Root one description.")

  #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Child one")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Child one description.")

    #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Grandchild one")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Grandchild one description.")]
    ]

]
  ]
  #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Child two")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Child two description.")]
  ]

]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Root two")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Root two description.")]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Protocols")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[  #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("HTTP")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Transfers web resources.")

    #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Headers")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Carry request metadata.")]
    ]

]
  ]
  #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("SSH")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Provides a secure shell.")]
  ]

]
]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Final")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("A root sibling after the nested topology.")]
]

#heading(level: 1)[#text("Delimiter transitions")] <id-5f64656c696d697465725f7472616e736974696f6e73>

#blocktitle[#text("Shorter second delimiter")]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("First deep term")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("First deep description.")

  #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Root term")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Root description.")]
  ]

]
]

#blocktitle[#text("Skipped delimiter depth")]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Root jump")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Root jump description.")

  #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Deep jump")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Deep jump description.")]
  ]

]
]

#blocktitle[#text("Semicolon to colon")]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Semicolon term")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Semicolon description.")

  #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Colon child")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Colon child description.")]
  ]

]
]

#blocktitle[#text("Colon to semicolon")]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Colon term")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Colon description.")

  #block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Semicolon child")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Semicolon child description.")]
  ]

]
]

#heading(level: 1)[#text("Formatted terms")] <id-5f666f726d61747465645f7465726d73>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())), spacing: 1em)
  + #block(width: 100%)[#block(width: 100%, breakable: false)[#emph[#text("What has ")#strong[#text("strong")]#text(" and ")#emph[#text("emphasized")]#text(" words?")]#linebreak()
#text("The mixed term preserves both formats.")]]
  + #block(width: 100%)[#block(width: 100%, breakable: false)[#emph[#strong[#text("Whole strong question?")]]#linebreak()
#text("A whole-term format.")]]
  + #block(width: 100%)[#block(width: 100%, breakable: false)[#emph[#text("What contains ")#raw("code")#text(" and ")#link("https://example.com")[#text("a link")]#text("?")]#linebreak()
#text("Code and link formats.")]]
]

#heading(level: 1)[#text("Titled list boundaries")] <id-5f7469746c65645f6c6973745f626f756e646172696573>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())), spacing: 1em)
  + #block(width: 100%)[#block(width: 100%, breakable: false)[#emph[#text("First list question?")]#linebreak()
#text("First list answer.")]]
]

#blocktitle[#text("Second list title")]
#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())), spacing: 1em)
  + #block(width: 100%)[#block(width: 100%, breakable: false)[#emph[#text("Second list question?")]#linebreak()
#text("Second list answer.")]]
]

#blocktitle[#text("Plain description-list title")]
#block(width: 100%, above: 0pt, below: 0.5em)[
#text(weight: "bold")[#text("Plain term")]
#block(above: 0pt, below: 0pt, inset: (left: 1.5em))[#text("Plain definition.")]
]

#heading(level: 1)[#text("Effective marker styles")] <id-5f6566666563746976655f6d61726b65725f7374796c6573>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
  + #block(width: 100%)[#strong[#text("Named ordered")#text(":")] #text("Named style is effective.")]
]

#[
#set list(marker: box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))))
  - #block(width: 100%)[#strong[#text("Named unordered")#text(":")] #text("Named style is effective.")]
]

#heading(level: 1)[#text("Continued mixed lists")] <id-5f636f6e74696e7565645f6d697865645f6c69737473>

  - #text("Outer unordered owner")
    #[
    #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
      + #text("First ordered child")
      + #text("Second ordered child")
          - #text("Unordered grandchild under the second ordered child")
          - #text("Second unordered grandchild")

    ]

  - #text("Following unordered sibling")
    #[
    #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())))
      + #text("Outer ordered owner")
          - #text("First unordered child")
          - #text("Second unordered child")
            #[
            #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("a.", ..numbers.pos())))
              + #text("Ordered grandchild under the second unordered child")
              + #text("Second ordered grandchild")
            ]


      + #text("Following ordered sibling")
    ]


#heading(level: 1)[#text("Trailing unanswered Q&A")] <id-5f747261696c696e675f756e616e7377657265645f7161>

#[
#set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("1.", ..numbers.pos())), spacing: 1em)
  + #block(width: 100%)[#block(width: 100%, breakable: false)[#emph[#text("Answered question?")]#linebreak()
#text("This question has an answer.")]]
  + #block(width: 100%)[#block(width: 100%, breakable: false)[#emph[#text("Trailing unanswered question?")]]]
]
