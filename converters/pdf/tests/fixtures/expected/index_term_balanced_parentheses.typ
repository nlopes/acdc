#set document(
  title: "index-parentheses(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[index-parentheses(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("index-parentheses(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("index-parentheses - complete index labels")

#heading(level: 1)[#text("Synopsis")] <id-5f73796e6f70736973>

#strong[#text("index-parentheses")]

#heading(level: 1)[#text("Description")] <id-5f6465736372697074696f6e>

#text("P01 ")#metadata(none)<__indexterm-1>#text("Term ®")#text(" after.")

#text("P02 ")#metadata(none)<__indexterm-2>#text("function(argument)")#text(" after.")

#text("P03 Before ")#metadata(none)<__indexterm-3>#text("function(argument)")#text(" after.")

#text("P04 ")#metadata(none)<__indexterm-4>#text("fn((x)) tail")#text(" after.")

#text("P05 Before ")#metadata(none)<__indexterm-5>#text("fn((x)) tail")#text(" after.")

#text("P06 ")#metadata(none)<__indexterm-6>#text("word")#text(") after.")

#text("P07 ")#metadata(none)<__indexterm-7>#text("word")#text(")) after.")

#text("P08 ")#metadata(none)<__indexterm-8>#text("function(argument)")#text(") after.")

#text("P09 ")#metadata(none)<__indexterm-9>#text("first")#text(" and ")#metadata(none)<__indexterm-10>#text("second")#text(".")

#text("P10 ")#metadata(none)<__indexterm-11>#text("Open (literal")#text(" and ")#metadata(none)<__indexterm-12>#text("Next")#text(".")

#text("P11 ")#metadata(none)<__indexterm-13>#text("Literal ) text")#text(" after.")

#text("P12 ")#metadata(none)<__indexterm-14>#text("\"function(x), detail\"")#text(" after.")

#text("P13 ")#metadata(none)<__indexterm-15>#text("\"quoted")#text(" tail\")) after.")

#text("P14 ")#metadata(none)<__indexterm-16>#text("Escaped \\( literal")#text(" after.")

#text("P15 ")#metadata(none)<__indexterm-17>#text("Escaped \\) literal")#text(" after.")

#text("P16 ")#text("((function(argument)))")#text(" after.")

#text("P17 ")#text("(")#metadata(none)<__indexterm-18>#text("function(argument)")#text(") after.")

#text("P18 ")#metadata(none)<__indexterm-19>#text(" after.")

#text("P19 ")#metadata(none)<__indexterm-20>#text("function(argument)")#text(" after.")

#text("P20 ")#metadata(none)<__indexterm-21>#text("Term")#text(" after.")

#text("P21 ")#metadata(none)<__indexterm-22>#text("Term")#text(" after.")

#text("P22 ")#metadata(none)<__indexterm-23>#text("café (déjà vu)")#text(" after.")

#text("P23 ")#metadata(none)<__indexterm-24>#text("First (one) and (two)")#text(" after.")

#text("P24 ")#metadata(none)<__indexterm-25>#text("Before ")#text("((literal))")#text(" after")#text(" end.")

#text("P25 ")#link("https://example.org")[#metadata(none)<__indexterm-26>#text("Linked ®")#text(" label")]#text(" after.")

#text("P26 ((Unclosed (label)")

#text("P27 Empty (()) after.")

#text("P28 ")#metadata(none)<__indexterm-27>#text(" after.")

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
#text(weight: "bold")[#text("@")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("\"function(x), detail\"")#_acdc_index_pages((<__indexterm-14>,), "term")]
#par(hanging-indent: 1em)[#text("\"quoted")#_acdc_index_pages((<__indexterm-15>,), "term")]
#par(hanging-indent: 1em)[#text("(nested)")#_acdc_index_pages((<__indexterm-27>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("B")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Before ")#text("((literal))")#text(" after")#_acdc_index_pages((<__indexterm-25>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("C")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("café (déjà vu)")#_acdc_index_pages((<__indexterm-23>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("E")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Escaped \\( literal")#_acdc_index_pages((<__indexterm-16>,), "term")]
#par(hanging-indent: 1em)[#text("Escaped \\) literal")#_acdc_index_pages((<__indexterm-17>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("F")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("first")#_acdc_index_pages((<__indexterm-9>,), "term")]
#par(hanging-indent: 1em)[#text("First (one) and (two)")#_acdc_index_pages((<__indexterm-24>,), "term")]
#par(hanging-indent: 1em)[#text("fn((x)) tail")#_acdc_index_pages((<__indexterm-4>,<__indexterm-5>,), "term")]
#metadata(none) <__indextermdef-66756e6374696f6e28617267756d656e7429002374657874282266756e6374696f6e28617267756d656e74292229>
#par(hanging-indent: 1em)[#text("function(argument)")#_acdc_index_pages((<__indexterm-2>,<__indexterm-3>,<__indexterm-8>,<__indexterm-18>,<__indexterm-19>,<__indexterm-20>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("L")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Linked ®")#_acdc_index_pages((<__indexterm-26>,), "term")]
#par(hanging-indent: 1em)[#text("Literal ) text")#_acdc_index_pages((<__indexterm-13>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("N")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Next")#_acdc_index_pages((<__indexterm-12>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("O")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Open (literal")#_acdc_index_pages((<__indexterm-11>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("S")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("second")#_acdc_index_pages((<__indexterm-10>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("T")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Term") (see #link(<__indextermdef-66756e6374696f6e28617267756d656e7429002374657874282266756e6374696f6e28617267756d656e74292229>)[#text("function(argument)")])]
#par(hanging-indent: 1em)[#text("Term ®")#_acdc_index_pages((<__indexterm-1>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("W")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("word")#_acdc_index_pages((<__indexterm-6>,<__indexterm-7>,), "term")]
]
