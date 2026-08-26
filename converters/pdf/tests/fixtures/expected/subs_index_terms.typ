#set document(
  title: "Index term rendering",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Index term rendering]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Index term rendering")]
]
#v(1em)

#heading(level: 1)[#text("First occurrences")] <id-5f66697273745f6f6363757272656e636573>

#text("Visible ")#metadata(none) <__indexterm-1>#text("Zebra")#text(" and ")#metadata(none) <__indexterm-2>#text("apple")#text(".")

#text("Concealed ")#metadata(none) <__indexterm-3>#metadata(none) <__indexterm-4>#text(" and ")#metadata(none) <__indexterm-5>#text(".")

#pagebreak(weak: true)

#heading(level: 1)[#text("Repeated occurrences")] <id-5f72657065617465645f6f6363757272656e636573>

#text("Repeated ")#metadata(none) <__indexterm-6>#text("Zebra")#text(", lower-case ")#metadata(none) <__indexterm-7>#text("animal")#text(", upper-case ")#metadata(none) <__indexterm-8>#text("Animal")#text(", and ")#metadata(none) <__indexterm-9>#text(".")

#pagebreak(weak: true)

#heading(level: 1)[#text("Same-page occurrences")] <id-5f73616d655f706167655f6f6363757272656e636573>

#text("Same-page repeats: ")#metadata(none) <__indexterm-10>#text("Zebra")#text(" and ")#metadata(none) <__indexterm-11>#text("Zebra")#text(".")

#heading(level: 1)[#text("Formatted and substituted terms")] <id-5f666f726d61747465645f616e645f73756273746974757465645f7465726d73>

#text("Direct formatting: ")#metadata(none) <__indexterm-12>#text("bold ")#strong[#text("primary")]#text(", ")#metadata(none) <__indexterm-13>#text("italic ")#emph[#text("primary")]#text(", and ")#metadata(none) <__indexterm-14>#text("mono ")#raw("primary")#text(".")

#text("Attributes: ")#metadata(none) <__indexterm-15>#text("plain Ada")#text(", ")#metadata(none) <__indexterm-16>#text("literal *attribute bold*")#text(", and ")#metadata(none) <__indexterm-17>#text("linked ")#link("https://example.com")[#text("Ada")]#text(".")

#text("Direct link: ")#metadata(none) <__indexterm-18>#text("direct ")#link("https://example.com")[#text("Ada")]#text(".")

#text("Replacements: ")#metadata(none) <__indexterm-19>#text("copyright © — arrow →")#text(".")

#text("Formatted hierarchy: ")#metadata(none) <__indexterm-20>#text(".")

#text("Formatting is identity: ")#metadata(none) <__indexterm-21>#text("identity")#text(", ")#metadata(none) <__indexterm-22>#strong[#text("identity")]#text(", and ")#metadata(none) <__indexterm-23>#emph[#text("identity")]#text(".")

#text("Ordered substitutions: ")#metadata(none) <__indexterm-24>#text("ordered ")#strong[#text("attribute bold")]#text(".")

#text("Late substitutions: ")#metadata(none) <__indexterm-25>#text("late Ada and ")#strong[#text("attribute bold")]#text(".")

#text("Disabled quotes: ")#metadata(none) <__indexterm-26>#text("literal *markers*")#text(".")

#heading(level: 1)[#text("Generated index")] <id-5f67656e6572617465645f696e646578>

#let _acdc_index_pages(targets, sequence) = context {
  let occurrences = targets.map(target => (target, counter(page).at(target).first()))
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
#text(weight: "bold")[#text("@")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("42 tools")#_acdc_index_pages((<__indexterm-5>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("A")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Animal")#_acdc_index_pages((<__indexterm-8>,), "term")]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#text("Bird")#_acdc_index_pages((<__indexterm-9>,), "term")]]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#text("Mammal")]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[#text("Cat")#_acdc_index_pages((<__indexterm-4>,), "term")]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[#text("Zebra")#_acdc_index_pages((<__indexterm-3>,), "term")]]
#par(hanging-indent: 1em)[#text("animal")#_acdc_index_pages((<__indexterm-7>,), "term")]
#par(hanging-indent: 1em)[#strong[#text("Animals")]]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#emph[#text("Mammals")]]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[#raw("Cats")#_acdc_index_pages((<__indexterm-20>,), "term")]]
#par(hanging-indent: 1em)[#text("apple")#_acdc_index_pages((<__indexterm-2>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("B")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("bold ")#strong[#text("primary")]#_acdc_index_pages((<__indexterm-12>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("C")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("copyright © — arrow →")#_acdc_index_pages((<__indexterm-19>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("D")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("direct ")#link("https://example.com")[#text("Ada")]#_acdc_index_pages((<__indexterm-18>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("I")]
#v(0.25em)
#par(hanging-indent: 1em)[#emph[#text("identity")]#_acdc_index_pages((<__indexterm-23>,), "term")]
#par(hanging-indent: 1em)[#strong[#text("identity")]#_acdc_index_pages((<__indexterm-22>,), "term")]
#par(hanging-indent: 1em)[#text("identity")#_acdc_index_pages((<__indexterm-21>,), "term")]
#par(hanging-indent: 1em)[#text("italic ")#emph[#text("primary")]#_acdc_index_pages((<__indexterm-13>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("L")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("late Ada and ")#strong[#text("attribute bold")]#_acdc_index_pages((<__indexterm-25>,), "term")]
#par(hanging-indent: 1em)[#text("linked ")#link("https://example.com")[#text("Ada")]#_acdc_index_pages((<__indexterm-17>,), "term")]
#par(hanging-indent: 1em)[#text("literal *attribute bold*")#_acdc_index_pages((<__indexterm-16>,), "term")]
#par(hanging-indent: 1em)[#text("literal *markers*")#_acdc_index_pages((<__indexterm-26>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("M")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("mono ")#raw("primary")#_acdc_index_pages((<__indexterm-14>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("O")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("ordered ")#strong[#text("attribute bold")]#_acdc_index_pages((<__indexterm-24>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("P")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("plain Ada")#_acdc_index_pages((<__indexterm-15>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("Z")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Zebra")#_acdc_index_pages((<__indexterm-1>,<__indexterm-6>,<__indexterm-10>,<__indexterm-11>,), "term")]
]
#heading(level: 1)[#text("Later terms are not in the earlier index")] <id-5f6c617465725f7465726d735f6172655f6e6f745f696e5f7468655f6561726c6965725f696e646578>

#text("Later ")#metadata(none) <__indexterm-27>#text("After")#text(".")
