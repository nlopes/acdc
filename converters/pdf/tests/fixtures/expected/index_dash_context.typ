#set document(
  title: "index-dash-context(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[index-dash-context(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("index-dash-context(1)")]
]
#v(1em)

#heading(level: 1)[#text("NAME")] <id-5f6e616d65>

#text("index-dash-context - neighboring index text")

#heading(level: 1)[#text("SYNOPSIS")] <id-5f73796e6f70736973>

#strong[#text("index-dash-context")]

#heading(level: 1)[#text("DESCRIPTION")] <id-5f6465736372697074696f6e>

#text("P01 prefix")#metadata(none)<__indexterm-1>#text("—​")#text("tail")

#text("P02 prefix")#metadata(none)<__indexterm-2>#text("—​tail")

#text("P03 ")#metadata(none)<__indexterm-3>#text("prefix—​")#text("tail")

#text("P04 prefix")#metadata(none)<__indexterm-4>#text("—​")#text("tail")

#text("P05 prefix")#metadata(none)<__indexterm-5>#metadata(none)<__indexterm-6>#text("—​")#text("tail")

#text("P06 prefix")#metadata(none)<__indexterm-7>#text("—​")#metadata(none)<__indexterm-8>#text("tail")

#text("P07 prefix")#metadata(none)<id-6265666f7265>#metadata(none)<__indexterm-9>#text("—​")#metadata(none)<id-6166746572>#text("tail")

#text("P08 prefix")#metadata(none)<__indexterm-10>#text("begin—​")#metadata(none)<__indexterm-11>#text("end")#text("tail")

#text("P09 prefix")#metadata(none)<__indexterm-12>#text("begin")#metadata(none)<__indexterm-13>#text("—​end")#text("tail")

#text("P10 pré")#metadata(none)<__indexterm-14>#text("—​")#text("été")

#text("P11 word_")#metadata(none)<__indexterm-15>#text("—​")#text("_tail")

#text("P12 prefix")#metadata(none)<__indexterm-16>#text("--")#text("tail")

#text("P13 prefix")#metadata(none)<__indexterm-17>#text("---")#text("tail")

#text("P14 prefix-")#metadata(none)<__indexterm-18>#text("--")#text("tail")

#text("P15 prefix")#metadata(none)<__indexterm-19>#text("--")#text("-tail")

#text("P16 prefix")#metadata(none)<__indexterm-20>#text("--")

#text("P17 ")#metadata(none)<__indexterm-21>#text("--")#text("tail")

#text("P18 prefix.")#metadata(none)<__indexterm-22>#text("--")#text("tail")

#text("P19 prefix")#metadata(none)<__indexterm-23>#text("--")#text(".tail")

#text("P20 prefix")#metadata(none)<__indexterm-24>#text(" — ")#text("tail")

#text("P21 prefix")#metadata(none)<__indexterm-25>#text("--")#strong[#text("bold")]

#text("P22 prefix")#metadata(none)<__indexterm-26>#text("--")#text("tail")

#text("P23 prefix")#metadata(none)<__indexterm-27>#text("—​")#text("tail")

#text("P24 prefix")#metadata(none)<__indexterm-28>#text("--")#text("tail")

#text("P25 prefix")#metadata(none)<__indexterm-29>#text("--")#text(" ")#link("https://next.example/prose")[#text("Next")]

#blocktitle[#text("Standalone control")]
#metadata(none)<__indexterm-30>#text(" — ")

#blocktitle[#text("Hidden standalone control")]
#metadata(none)<__indexterm-31>#metadata(none)<__indexterm-32>#text(" — ")

#text("P26 prefix—​")#metadata(none)<__indexterm-33>#text("")#text("tail")

#text("P27 prefix")#metadata(none)<__indexterm-34>#text("—​")#text("tail")

#text("P28 prefix")#metadata(none)<__indexterm-35>#text("—​")#metadata(none)<__indexterm-36>#text("")#text("tail")

#text("P29 prefix")#metadata(none)<__indexterm-37>#metadata(none)<__indexterm-38>#text(" — ")#text("tail")

#text("P30 prefix")#metadata(none)<id-73706163652d6265666f7265>#metadata(none)<__indexterm-39>#text(" — ")#metadata(none)<id-73706163652d6166746572>#text("tail")

#text("P31 prefix ")#metadata(none)<__indexterm-40>#text(" — ")#text(" tail")

#text("P32 Sa")#metadata(none)<__indexterm-41>#text("m")#text("’s")

#text("P33 ")#metadata(none)<__indexterm-42>#text("Sam’")#text("s")

#text("P34 Sam")#metadata(none)<__indexterm-43>#text("’")#text("s")

#text("P35 pré")#metadata(none)<__indexterm-44>#text("’")#text("été")

#text("P36 3")#metadata(none)<__indexterm-45>#text("'")#text("4")

#text("P37 _")#metadata(none)<__indexterm-46>#text("'")#text("s")

#text("P38 Sam")#metadata(none)<__indexterm-47>#text("'")#text("s")

#text("P39 Sa")#metadata(none)<__indexterm-48>#metadata(none)<__indexterm-49>#text("m")#text("’s")

#text("P40 Sa")#metadata(none)<id-61706f7374726f706865>#metadata(none)<__indexterm-50>#text("m")#text("’s")

#text("P41 prefix")#text("-")#metadata(none)<__indexterm-51>#text("-")#text("tail")

#text("P42 prefix")#text(" ")#metadata(none)<__indexterm-52>#text("--")#text(" tail")

#text("P43 prefix ")#metadata(none)<__indexterm-53>#text("--")#text(" ")#text("tail")

#text("P44 prefix ")#metadata(none)<__indexterm-54>#text("--")#text(" tail")

#text("P45 prefix")#metadata(none)<__indexterm-55>#text(" — ")#text("")#strong[#text("Bold")]

#text("P46 prefix-")#strong[#text("-")]#text("tail")

#text("P47 prefix")#metadata(none)<__indexterm-56>#text(" — ")#text("--")#metadata(none)<__indexterm-57>#text(" — ")#text("tail")

#text("P48 prefix")#metadata(none)<__indexterm-58>#text(" — ")#text("")#metadata(none)<__indexterm-59>#text(" — ")#text("tail")

#text("P49 prefix")#metadata(none)<__indexterm-60>#text(" — ")#text("")#link("https://next.example/spaced")[#text("Spaced")]

#text("P50 ")#metadata(none)<__indexterm-61>#text("Sam’")#text("s ")#link("https://next.example/apostrophe")[#text("Apostrophe")]

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
#par(hanging-indent: 1em)[#text("'")#_acdc_index_pages((<__indexterm-43>,<__indexterm-44>,<__indexterm-45>,<__indexterm-46>,), "term")]
#par(hanging-indent: 1em)[#text("-")#_acdc_index_pages((<__indexterm-33>,<__indexterm-34>,<__indexterm-35>,<__indexterm-36>,<__indexterm-51>,), "term")]
#par(hanging-indent: 1em)[#text("--")#_acdc_index_pages((<__indexterm-26>,), "term")]
#par(hanging-indent: 1em)[#text(" — ")#_acdc_index_pages((<__indexterm-1>,<__indexterm-4>,<__indexterm-6>,<__indexterm-7>,<__indexterm-9>,<__indexterm-14>,<__indexterm-15>,<__indexterm-18>,<__indexterm-19>,<__indexterm-20>,<__indexterm-21>,<__indexterm-22>,<__indexterm-23>,<__indexterm-24>,<__indexterm-25>,<__indexterm-27>,<__indexterm-29>,<__indexterm-30>,<__indexterm-32>,<__indexterm-38>,<__indexterm-39>,<__indexterm-40>,<__indexterm-52>,<__indexterm-53>,<__indexterm-54>,<__indexterm-55>,<__indexterm-56>,<__indexterm-57>,<__indexterm-58>,<__indexterm-59>,<__indexterm-60>,), "term")]
#par(hanging-indent: 1em)[#text("---")#_acdc_index_pages((<__indexterm-17>,), "term")]
#par(hanging-indent: 1em)[#text("--end")#_acdc_index_pages((<__indexterm-13>,), "term")]
#par(hanging-indent: 1em)[#text("--tail")#_acdc_index_pages((<__indexterm-2>,), "term")]
#par(hanging-indent: 1em)[#text("\\'")#_acdc_index_pages((<__indexterm-47>,), "term")]
#par(hanging-indent: 1em)[#text("--")#_acdc_index_pages((<__indexterm-16>,<__indexterm-28>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("B")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("begin")#_acdc_index_pages((<__indexterm-12>,), "term")]
#par(hanging-indent: 1em)[#text("begin--")#_acdc_index_pages((<__indexterm-10>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("E")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("end")#_acdc_index_pages((<__indexterm-11>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("H")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Hidden")#_acdc_index_pages((<__indexterm-5>,<__indexterm-8>,<__indexterm-37>,<__indexterm-48>,), "term")]
#par(hanging-indent: 1em)[#text("Hidden standalone")#_acdc_index_pages((<__indexterm-31>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("M")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("m")#_acdc_index_pages((<__indexterm-41>,<__indexterm-49>,<__indexterm-50>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("P")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("prefix--")#_acdc_index_pages((<__indexterm-3>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("S")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Sam'")#_acdc_index_pages((<__indexterm-42>,<__indexterm-61>,), "term")]
]
