#set document(
  title: "escaped-macros(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[escaped-macros(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("escaped-macros(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("escaped-macros - literal macro labels")

#heading(level: 1)[#text("Description")] <id-5f6465736372697074696f6e>

#text("E01 ")#text("indexterm2:[One \\]")#text(" term]")

#text("E02 ")#text("indexterm:[Hidden \\\\]")#text(" term]")

#text("E03 ")#text("((Short \\] term))")

#text("E04 ")#text("https://example.org[URL \\]")#text(" text]")

#text("E05 ")#text("mailto:user@example.org[Mail \\]")#text(" text]")

#text("E06 ")#text("xref:target[Cross \\]")#text(" text]")

#text("E07 ")#text("footnote:[Note \\]")#text(" text]")

#text("E08 ")#text("anchor:target[Anchor \\] text]")

#text("E09 ")#text("indexterm2:[Open \\[ text]")

#text("E10 ")#text("indexterm2:[")#strong[#text("Bold")]#text(" — Attribute \\]")#text(" text]")

#text("E11 ")#text("indexterm2:[Repeated \\\\\\]")#text(" term]")

#text("E12 Ordinary \\] and \\[ and \\\\] text.")

#text("E13 ")#text("((Nested ")#strong[#text("bold")]#text(" \\] term))")

#text("E14 ")#metadata(none) <__indexterm-1>#text("Active ")#text("]")#text(" term")

#text("E15 ")#link("https://example.org")[#text("Active ")#text("]")#text(" label")]

#text("E16 ")#counter(footnote).update(0)#footnote[#text("Active ")#text("]")#text(" note")]#text(" ")#counter(footnote).update(1)#footnote[#text("Repeated ")#text("\\]")#text(" note")]

#text("E17 ")#metadata(none) <__indexterm-2>#text("Short active ")#text("\\]")#text(" term")

#text("E18 ")#link("https://example.org")[#text("Repeated ")#text("\\]")#text(" label")]

#text("E19 ")#link("https://example.org")[#metadata(none) <__indexterm-3>#text("Nested active ")#text("]")#text(" term")]

#text("E20 ")#text("[[escaped-target]]")#text(" and ")#text("<<escaped-target>>")#text(".")

#text("E21 ")#text("*literal* \\] text")#text(" and ")#text("<em>raw HTML</em>")#text(".")

#blockquote[
#text("E22 ")#metadata(none) <__indexterm-4>#text("Quoted ")#text("\\]")#text(" term")#text(" and ")#text("*literal* \\] text")#text(".")

]

#metadata(none) <id-746172676574>
#text("E23 ")#context link(query(<id-746172676574>).first().location())[#text("Cross ")#text("]")#text(" label")]#text(" and ")#context link(query(<id-746172676574>).first().location())[#text("Repeated ")#text("\\]")#text(" label")]#text(".")

#text("E24 ")#counter(footnote).update(2)#footnote[#text("Opening \\[ note")]#text(" and ")#context link(query(<id-746172676574>).first().location())[#text("Opening \\[ label")]#text(".")

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
#par(hanging-indent: 1em)[#text("Active ")#text("]")#text(" term")#_acdc_index_pages((<__indexterm-1>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("N")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Nested active ")#text("\\]")#text(" term")#_acdc_index_pages((<__indexterm-3>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("Q")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Quoted ")#text("\\]")#text(" term")#_acdc_index_pages((<__indexterm-4>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("S")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Short active ")#text("\\]")#text(" term")#_acdc_index_pages((<__indexterm-2>,), "term")]
]
