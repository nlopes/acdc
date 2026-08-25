#set document(
  title: "Silent Metadata",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Silent Metadata]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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

#let _acdc_image_callout(label, body) = pad(left: 0pt, block(width: 100%, inset: (x: 12pt, y: 4pt), grid(columns: (auto, 1fr), column-gutter: 12pt, align: (x, _) => if x == 0 { center + horizon } else { left + top }, label, grid.cell(stroke: (left: 0.75pt + rgb("#e5e7eb")), inset: (left: 12pt), body))))
#align(center)[
#text(size: 22pt, weight: "bold")[#text("Silent Metadata")]
]
#v(1em)

#page(header: none, footer: none)[
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
        str(counter(page).at(target).first()),
      ),
    ),
  )
}
#_acdc_toc_entry(<id-5f6275696c745f696e5f7061726167726170685f726f6c6573>, 0, [#text("Built-in paragraph roles")])
#_acdc_toc_entry(<id-5f6c6973745f6d61726b65725f7374796c6573>, 0, [#text("List marker styles")])
#_acdc_toc_entry(<id-5f61646d6f6e6974696f6e5f69636f6e5f6f76657272696465>, 0, [#text("Admonition icon override")])
#_acdc_toc_entry(<id-5f696d6167655f73697a696e675f616e645f66697474696e67>, 0, [#text("Image sizing and fitting")])
#_acdc_toc_entry(<id-5f736f757263655f686967686c69676874696e675f66616c6c6261636b>, 0, [#text("Source highlighting fallback")])
#_acdc_toc_entry(<id-5f627265616b61626c655f7461626c65>, 0, [#text("Breakable table")])
#_acdc_toc_entry(<id-5f706167655f6c61796f75745f66616c6c6261636b>, 0, [#text("Page layout fallback")])
#pagebreak()

]

#heading(level: 1)[#text("Built-in paragraph roles")] <id-5f6275696c745f696e5f7061726167726170685f726f6c6573>

#text(size: 1.2em)[#text("Big paragraph role.")]

#text(size: 0.8em)[#text("Small paragraph role.")]

#text(size: 0.8em, style: "italic", fill: rgb("#999999"))[#text("Subtitle paragraph role.")]

#underline[#text("Underlined paragraph role.")]

#strike[#text("Struck paragraph role.")]

#text("Inline pre-wrap keeps ")#text("two ​ spaces ​ together")#text(" and ")#raw("two ​ monospace ​ spaces")#text(".")

#heading(level: 1)[#text("List marker styles")] <id-5f6c6973745f6d61726b65725f7374796c6573>

#[
#set list(marker: depth => if depth == 0 { box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))) } else { let markers = (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))); markers.at(calc.rem(depth, markers.len())) })
  - #block(width: 100%)[#text("Disc marker")

    #[
    #set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))), body-indent: 0.5em)
      - #text("Nested default circle marker")
    ]

  ]
]

#[
#set list(marker: depth => if depth == 0 { box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))) } else { let markers = (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))); markers.at(calc.rem(depth, markers.len())) })
  - #block(width: 100%)[#text("Circle marker")

    #[
    #set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))), body-indent: 0.5em)
      - #text("Nested default circle marker")
    ]

  ]
]

#[
#set list(marker: depth => if depth == 0 { box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280"))) } else { let markers = (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))); markers.at(calc.rem(depth, markers.len())) })
  - #block(width: 100%)[#text("Square marker")

    #[
    #set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))), body-indent: 0.5em)
      - #text("Nested default circle marker")
    ]

  ]
]

#[
#set list(marker: depth => if depth == 0 { box(width: 0.28em)[] } else { let markers = (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))); markers.at(calc.rem(depth, markers.len())) })
  - #block(width: 100%)[#text("Unordered item without a marker")

    #[
    #set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))), body-indent: 0.5em)
      - #text("Nested item with the default circle marker")
    ]

  ]
]

#[
#set list(marker: depth => if depth == 0 { none } else { let markers = (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))); markers.at(calc.rem(depth, markers.len())) })
  - #block(width: 100%)[#text("Unordered item without a marker")

    #[
    #set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))), body-indent: 0.5em)
      - #text("Nested item with the default circle marker")
    ]

  ]
]

#[
#set list(marker: depth => if depth == 0 { none } else { let markers = (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))); markers.at(calc.rem(depth, markers.len())) }, body-indent: 0pt)
  - #block(width: 100%)[#text("Unordered item without a marker or marker indentation")

    #[
    #set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))), body-indent: 0.5em)
      - #text("Nested item with the default circle marker")
    ]

  ]
]

#[
#set enum(numbering: (..numbers) => none)
  + #block(width: 100%)[#text("Ordered item without a marker")

    #[
    #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("a.", ..numbers.pos())))
      + #text("Nested item with default lower-alpha numbering")
    ]

  ]
]

#[
#set enum(numbering: (..numbers) => none)
  + #block(width: 100%)[#text("Ordered item without a marker or marker indentation")

    #[
    #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("a.", ..numbers.pos())))
      + #text("Nested item with default lower-alpha numbering")
    ]

  ]
]

#[
#set enum(numbering: (..numbers) => none, body-indent: 0pt)
  + #block(width: 100%)[#text("Ordered item without a marker or marker indentation")

    #[
    #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("a.", ..numbers.pos())), body-indent: 0.5em)
      + #text("Nested item with default lower-alpha numbering")
    ]

  ]
]

#[
#set enum(numbering: (..numbers) => box(width: 0.5em)[])
  + #block(width: 100%)[#text("Ordered item with empty marker spacing")

    #[
    #set enum(numbering: (..numbers) => text(fill: rgb("#9ca3af"), numbering("a.", ..numbers.pos())))
      + #text("Nested item with default lower-alpha numbering")
    ]

  ]
]

#heading(level: 1)[#text("Admonition icon override")] <id-5f61646d6f6e6974696f6e5f69636f6e5f6f76657272696465>

#_acdc_image_callout(image("/images/de454d7e4e1cfda7.svg", width: 36pt, alt: "note"))[
#text("This admonition uses a block-specific image icon.")

]

#heading(level: 1)[#text("Image sizing and fitting")] <id-5f696d6167655f73697a696e675f616e645f66697474696e67>

#block(width: 100%, radius: 4pt, clip: false)[#context layout(size => { let body = scale(x: 50%, y: 50%, reflow: true, image("/images/de454d7e4e1cfda7.svg", alt: "Intrinsic scale")); let body-width = measure(body).width; if body-width > size.width { let factor = size.width / body-width * 100%; scale(x: factor, y: factor, reflow: true, body) } else { body } })]

#block(width: 100%, radius: 4pt, clip: false)[#context layout(size => { let body = scale(x: 1000%, y: 1000%, reflow: true, image("/images/de454d7e4e1cfda7.svg", alt: "Oversized intrinsic scale capped to the available width")); let body-width = measure(body).width; if body-width > size.width { let factor = size.width / body-width * 100%; scale(x: factor, y: factor, reflow: true, body) } else { body } })]

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "Content width percentage", width: 40%)]

#block(width: 100%, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "Physical print width", width: 72pt)]

#context layout(available => move(dx: -here().position().x, block(width: 595.276pt, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "Page-aligned viewport image", width: 595.276pt)]
))

#sidebarbox[
#context layout(available => move(dx: -here().position().x, block(width: 595.276pt, radius: 4pt, clip: false)[#image("/images/de454d7e4e1cfda7.svg", alt: "Page-aligned image with nested content width", width: 1 * available.width)]
))

]

#text("Inline intrinsic scale ")#box(context layout(size => { let body = scale(x: 25%, y: 25%, reflow: true, image("/images/de454d7e4e1cfda7.svg", alt: "Inline scale")); let body-width = measure(body).width; if body-width > size.width { let factor = size.width / body-width * 100%; scale(x: factor, y: factor, reflow: true, body) } else { body } }))#text(" and inline content width ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "Inline content width", width: 20%))#text(".")

#text("Inline line fit ")#box(context layout(size => { let body = image("/images/de454d7e4e1cfda7.svg", alt: "Line fit"); let body-height = measure(body).height; let line-height = measure(box(height: 1em)).height; let target-height = calc.min(size.height, line-height); if body-height > target-height { let factor = target-height / body-height * 100%; scale(x: factor, y: factor, reflow: true, body) } else { body } }))#text(" and inline unrestricted fit ")#box(image("/images/de454d7e4e1cfda7.svg", alt: "No fit"))#text(".")

#heading(level: 1)[#text("Source highlighting fallback")] <id-5f736f757263655f686967686c69676874696e675f66616c6c6261636b>

#raw(block: true, lang: "php", "<p><?php echo \"mixed mode\"; ?></p>")

#heading(level: 1)[#text("Breakable table")] <id-5f627265616b61626c655f7461626c65>

#block(sticky: true, above: 0pt, below: 0pt)[
#metadata(none) <id-627265616b61626c652d7461626c65>
]
#block(sticky: true, above: 0pt, below: 0pt)[
#blocktitle[#text("Table 1. ")#text("Caption kept with the first row")]
]
#table(columns: (1fr), align: (left + top), stroke: none, table.header(repeat: true, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("First row")

]]), table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 1.25pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("Second row")

])

#heading(level: 1)[#text("Page layout fallback")] <id-5f706167655f6c61796f75745f66616c6c6261636b>

#pagebreak(weak: true)

#text("The landscape request keeps the document layout and produces a diagnostic.")

#pagebreak(weak: true)

#text("The portrait request also keeps the document layout.")
