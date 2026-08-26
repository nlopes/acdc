#set document(
  title: "Index Term Relationships",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Index Term Relationships]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Index Term Relationships")]
]
#v(1em)

#heading(level: 1)[#text("Terms")] <id-5f7465726d73>

#text("Visible shorthand redirect: ")#metadata(none) <__indexterm-1>#text("Flash")#text(".")

#text("Named visible redirect: ")#metadata(none) <__indexterm-2>#text("Shockwave")#text(".")

#text("The ")#metadata(none) <__indexterm-3>#text("HTML 5")#text(" term is the redirect target.")

#text("Primary related terms: ")#metadata(none) <__indexterm-4>#text("Desserts")#text(".")

#metadata(none) <__indexterm-5>#text("The Cakes entry is nested.")

#metadata(none) <__indexterm-6>#text("Cookies")#text(" and ")#metadata(none) <__indexterm-7>#text("Candies")#text(" are related primary entries.")

#metadata(none) <__indexterm-8>#text("The Cougars entry has a relationship.")

#metadata(none) <__indexterm-9>#text("Puma")#text(" is another primary entry.")

#text("Named concealed relationship: ")#metadata(none) <__indexterm-10>#text(".")

#text("Missing delimiter spacing stays literal: ")#metadata(none) <__indexterm-11>#text("A>>B")#text(" and ")#metadata(none) <__indexterm-12>#text("C &>D")#text(".")

#heading(level: 1)[#text("Index")] <id-5f696e646578>

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
#text(weight: "bold")[#text("A")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("A>>B")#_acdc_index_pages((<__indexterm-11>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("B")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Big Cats")]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#text("Cougars")#_acdc_index_pages((<__indexterm-8>,), "term")]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[(see also #link(<__indextermdef-50756d61002374657874282250756d612229>)[#text("Puma")])]]
#v(0.75em)
#text(weight: "bold")[#text("C")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("C &>D")#_acdc_index_pages((<__indexterm-12>,), "term")]
#metadata(none) <__indextermdef-43616e64696573002374657874282243616e646965732229>
#par(hanging-indent: 1em)[#text("Candies")#_acdc_index_pages((<__indexterm-7>,), "term")]
#metadata(none) <__indextermdef-436f6f6b6965730023746578742822436f6f6b6965732229>
#par(hanging-indent: 1em)[#text("Cookies")#_acdc_index_pages((<__indexterm-6>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("D")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Desserts")#_acdc_index_pages((<__indexterm-4>,), "term")]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[(see also #link(<__indextermdef-43616e64696573002374657874282243616e646965732229>)[#text("Candies")])]]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[(see also #link(<__indextermdef-436f6f6b6965730023746578742822436f6f6b6965732229>)[#text("Cookies")])]]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#text("Cakes")#_acdc_index_pages((<__indexterm-5>,), "term")]]
#v(0.75em)
#text(weight: "bold")[#text("F")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Flash") (see #link(<__indextermdef-48544d4c2035002374657874282248544d4c20352229>)[#text("HTML 5")])]
#v(0.75em)
#text(weight: "bold")[#text("H")]
#v(0.25em)
#metadata(none) <__indextermdef-48544d4c2035002374657874282248544d4c20352229>
#par(hanging-indent: 1em)[#text("HTML 5")#_acdc_index_pages((<__indexterm-3>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("P")]
#v(0.25em)
#metadata(none) <__indextermdef-50756d61002374657874282250756d612229>
#par(hanging-indent: 1em)[#text("Puma")#_acdc_index_pages((<__indexterm-9>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("S")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Shockwave") (see #link(<__indextermdef-48544d4c2035002374657874282248544d4c20352229>)[#strong[#text("HTML 5")]])]
#par(hanging-indent: 1em)[#text("Standards")]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#text("Web")#_acdc_index_pages((<__indexterm-10>,), "term")]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[(see also #link(<__indextermdef-48544d4c2035002374657874282248544d4c20352229>)[#text("HTML 5")])]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[(see also #text("Missing Term"))]]
]
