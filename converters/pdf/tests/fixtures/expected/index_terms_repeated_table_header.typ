#set document(
  title: "Index Terms in Repeated Table Headers",
)
#set page(paper: "a5", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Index Terms in Repeated Table Headers]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Index Terms in Repeated Table Headers")]
]
#v(1em)

#text(size: 1.25em)[#text("A control term before the table ")#metadata(none) <__indexterm-1>#text(".")]

#block(sticky: true, above: 0pt, below: 0pt)[
#blocktitle[#text("Table 1. ")#metadata(none) <__indexterm-2>#text("Cataloged table")]
]
#table(columns: (1fr, 3fr), align: (left + top, left + top), stroke: none, table.header(repeat: true, table.cell(x: 0, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#text("Key ")#metadata(none) <__indexterm-3>#text(" ")#metadata(none) <__indexterm-4>

]], table.cell(x: 1, y: 0, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 1.25pt + rgb("#dddddd"), ))[#tableheader[#metadata(none) <__indexterm-5>#text("Description with ")#metadata(none) <__indexterm-6>#text("visible ")#strong[#text("header")]#text(", ")#metadata(none) <__indexterm-7>#text("shared term")#text(", and related material")

]]), table.cell(x: 0, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 1.25pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("1")

], table.cell(x: 1, y: 1, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 1.25pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("The first body row contains ")#metadata(none) <__indexterm-8>#text(" and enough text to exercise normal table-cell wrapping.")

], table.cell(x: 0, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("2")

], table.cell(x: 1, y: 2, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 3, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("3")

], table.cell(x: 1, y: 3, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 4, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("4")

], table.cell(x: 1, y: 4, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 5, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("5")

], table.cell(x: 1, y: 5, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 6, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("6")

], table.cell(x: 1, y: 6, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 7, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("7")

], table.cell(x: 1, y: 7, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 8, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("8")

], table.cell(x: 1, y: 8, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 9, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("9")

], table.cell(x: 1, y: 9, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 10, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("10")

], table.cell(x: 1, y: 10, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 11, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("11")

], table.cell(x: 1, y: 11, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 12, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("12")

], table.cell(x: 1, y: 12, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 13, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("13")

], table.cell(x: 1, y: 13, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 14, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("14")

], table.cell(x: 1, y: 14, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 15, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("15")

], table.cell(x: 1, y: 15, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 16, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("16")

], table.cell(x: 1, y: 16, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 17, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("17")

], table.cell(x: 1, y: 17, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 18, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("18")

], table.cell(x: 1, y: 18, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 19, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("19")

], table.cell(x: 1, y: 19, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.cell(x: 0, y: 20, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("20")

], table.cell(x: 1, y: 20, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ))[#text("A deliberately long body row that helps the table continue onto another page while keeping the fixture deterministic.")

], table.footer(repeat: false, table.cell(x: 0, y: 21, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f0f0f0"))[#text("Footer ")#metadata(none) <__indexterm-9>

], table.cell(x: 1, y: 21, stroke: (left: 0.5pt + rgb("#dddddd"), right: 0.5pt + rgb("#dddddd"), top: 0.5pt + rgb("#dddddd"), bottom: 0.5pt + rgb("#dddddd"), ), fill: rgb("#f0f0f0"))[#text("The non-repeating footer contains a concealed term.")

]))

#text("An occurrence after the table ")#metadata(none) <__indexterm-10>#text(".")

#heading(level: 1)[#text("Index")] <id-5f696e646578>

#let _acdc_index_pages(targets, sequence) = context {
  let occurrences = targets
    .map(target => {
      let location = query(target).last().location()
      (location, counter(page).at(location).first())
    })
    .sorted(key: occurrence => occurrence.first().page())
  if sequence == "page" or sequence == "range" {
    occurrences = occurrences.dedup(key: occurrence => occurrence.last())
  }
  let linked = occurrence => link(
    occurrence.first(),
    counter(page).display(at: occurrence.first()),
  )
  let pages = if sequence == "range" {
    let ranges = ()
    for occurrence in occurrences {
      if ranges.len() > 0 and occurrence.last() == ranges.last().last().last() + 1 {
        let previous = ranges.pop()
        ranges.push((previous.first(), occurrence))
      } else {
        ranges.push((occurrence, occurrence))
      }
    }
    ranges.map(range => if range.first().last() == range.last().last() {
      linked(range.first())
    } else {
      linked(range.first()) + [-] + linked(range.last())
    })
  } else {
    occurrences.map(linked)
  }
  if pages.len() > 0 {
    [, ] + pages.join[, ]
  }
}
#columns(2, gutter: 12pt)[
#text(weight: "bold")[#text("A")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("after table")#_acdc_index_pages((<__indexterm-10>,), "page")]
#v(0.75em)
#text(weight: "bold")[#text("B")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("before table")#_acdc_index_pages((<__indexterm-1>,), "page")]
#par(hanging-indent: 1em)[#text("body term")#_acdc_index_pages((<__indexterm-8>,), "page")]
#v(0.75em)
#text(weight: "bold")[#text("F")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("footer term")#_acdc_index_pages((<__indexterm-9>,), "page")]
#v(0.75em)
#text(weight: "bold")[#text("R")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Related header")#_acdc_index_pages((<__indexterm-5>,), "page")]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[(see also #link(<__indextermdef-5461626c657300237465787428225461626c65732229>)[#text("Tables")])]]
#v(0.75em)
#text(weight: "bold")[#text("S")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("shared term")#_acdc_index_pages((<__indexterm-4>,<__indexterm-7>,), "page")]
#v(0.75em)
#text(weight: "bold")[#text("T")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("table title")#_acdc_index_pages((<__indexterm-2>,), "page")]
#metadata(none) <__indextermdef-5461626c657300237465787428225461626c65732229>
#par(hanging-indent: 1em)[#text("Tables")]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#text("Headers")]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[#text("concealed")#_acdc_index_pages((<__indexterm-3>,), "page")]]
#v(0.75em)
#text(weight: "bold")[#text("V")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("visible ")#strong[#text("header")]#_acdc_index_pages((<__indexterm-6>,), "page")]
]
