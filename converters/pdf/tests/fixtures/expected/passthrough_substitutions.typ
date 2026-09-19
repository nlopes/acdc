#set document(
  title: "Passthrough substitutions",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Passthrough substitutions]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Passthrough substitutions")]
]
#v(1em)

#heading(level: 1)[#text("Structural forms")] <id-5f7374727563747572616c5f666f726d73>

#text("Single: ")#text("<tag> {name} *bold* (C) -> ... --")

#text("Double: ")#text("<tag> {name} *bold* (C) -> ... --")

#text("Triple: ")#text("#pagebreak() {name} *bold* (C)")

#text("Single numeric: ")#text("&#169;")

#text("Double numeric: ")#text("&#169;")

#text("Triple numeric: ")#text("©")

#text("Escaped single plain: +plain+")

#text("Escaped double plain: ")#text("+plain")#text("+")

#text("Escaped triple plain: ")#text("++plain")#text("++")

#text("Escaped single formatting: +")#strong[#text("bold")]#text("+")

#text("Escaped double formatting: ")#text("+*bold*")#text("+")

#text("Escaped triple formatting: ")#text("+")#strong[#text("bold")]#text("+")

#text("Escaped single attribute: +Ada+")

#text("Escaped double attribute: ")#text("+{name}")#text("+")

#text("Escaped triple attribute: ")#text("+")#text("Ada")#text("+")

#text("Escaped single markup: +<mark>*bold*</mark>+")

#text("Escaped double markup: ")#text("+<mark>*bold*</mark>")#text("+")

#text("Escaped triple markup: ")#text("+")#text("<mark>*bold*</mark>")#text("+")

#text("Escaped single numeric: +©+")

#text("Escaped double numeric: ")#text("+&#169;")#text("+")

#text("Escaped triple numeric: ")#text("+")#text("©+++")

#text("Macros disabled escaped single: \\+")#strong[#text("bold")]#text("+")

#text("Macros disabled escaped double: \\++")#strong[#text("bold")]#text("++")

#text("Macros disabled escaped triple: \\+++")#strong[#text("bold")]#text("+++")

#text("Macro without substitutions: ")#text("literal #pagebreak() {name} *bold* (C)")

#text("Numeric reference without substitutions: ")#text("©")

#heading(level: 1)[#text("Individual substitutions")] <id-5f696e646976696475616c5f737562737469747574696f6e73>

#text("Special characters: ")#text("<tag> & &#169;")

#text("Attributes: ")#text("Ada")#text(" *bold*")

#text("Quotes: ")#strong[#text("bold")]#text(" ")#emph[#text("italic")]#text(" ")#highlight[#text("marked")]

#text("Replacements: ")#text("© -> … — ")

#text("Macros: ")#link("https://example.com")[#text("https://example.com")]

#text("Post replacements: ")#text("first")#linebreak()#text("second")

#heading(level: 1)[#text("Substitution groups")] <id-5f737562737469747574696f6e5f67726f757073>

#text("Normal: ")#text("<tag> ")#text("Ada")#text(" ")#strong[#text("bold")]#text(" © ")#link("https://example.com")[#text("https://example.com")]

#text("Normal quotes before attributes: ")#text("*attribute bold*")

#text("Verbatim: ")#text("<tag> {name} *bold* (C) <1>")

#heading(level: 1)[#text("Ordered substitutions")] <id-5f6f7264657265645f737562737469747574696f6e73>

#text("Quotes then attributes: ")#text("*attribute bold*")

#text("Attributes then quotes: ")#strong[#text("attribute bold")]

#text("Replacements then attributes: ")#text("(C)")

#text("Attributes then replacements: ")#text("©")

#text("Macros then attributes: ")#text("https://example.com")

#text("Attributes then macros: ")#link("https://example.com")[#text("https://example.com")]

#text("Special chars then attributes markup: ")#text("<mark>inserted</mark>")

#text("Attributes then special chars markup: ")#text("&lt;mark&gt;inserted&lt;/mark&gt;")

#text("Special chars then attributes numeric: ")#text("&#169;")

#text("Attributes then special chars numeric: ")#text("&amp;#169;")

#text("Post then attributes: ")#text("first ")#text("+")#text(" second")

#text("Attributes then post: ")#text("first")#linebreak()#text("second")

#heading(level: 1)[#text("Escaping")] <id-5f6573636170696e67>

#text("No substitutions: ")#text("\\*literal*")

#text("Quotes: ")#text("*")#text("literal*")

#text("Replacement escape: ")#text("\\->")

#text("Typst source stays text: ")#text("literal #pagebreak() ) \\ \" #raw(\"injected\")")
