#set document(
  title: "named-index(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[named-index(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("named-index(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("named-index - keep complete macros in index labels")

#heading(level: 1)[#text("Synopsis")] <id-5f73796e6f70736973>

#metadata(none) <id-746172676574>
#text("Destination.")

#text("N01 ")#metadata(none)<__indexterm-1>#text("Before ")#link("https://example.org/url")[#text("Link")]#text(" after")#text(" end.")

#text("N02 ")#metadata(none)<__indexterm-2>#text("Before ")#link("https://example.org/link")[#text("Link")]#text(" after")#text(" end.")

#text("N03 ")#metadata(none)<__indexterm-3>#text("Before ")#link("mailto:user@example.org")[#text("Mail")]#text(" after")#text(" end.")

#text("N04 ")#metadata(none)<__indexterm-4>#text("Before ")#context link(query(<id-746172676574>).first().location())[#text("Target")]#text(" after")#text(" end.")

#text("N05 ")#metadata(none)<__indexterm-5>#text("Before ")#counter(footnote).update(0)#footnote[#text("Only note.")]<id-666f6f746e6f74653a6f6e65>#text(" after")#text(" end. Reuse ")#footnote(<id-666f6f746e6f74653a6f6e65>)#text(".")

#text("N06 ")#metadata(none)<__indexterm-6>#text("Before [literal")#text(" after] end.")

#text("N07 ")#metadata(none)<__indexterm-7>#text("Before [literal")#text("]")#text(" after")#text(" end.")

#text("N08 ")#metadata(none)<__indexterm-8>#text("Before ")#link("https://example.org/incomplete")[#text("https://example.org/incomplete")]#text("[Link")#text(" end.")

#text("N09 ")#metadata(none)<__indexterm-9>#text("Before ")#link("https://example.org/escaped")[#text("https://example.org/escaped")]#text("[Link")#text("]")#text(" after")#text(" end.")

#text("N10 ")#metadata(none)<__indexterm-10>#text("Before ")#text("https://example.org/inactive[Link]")#text(" after")#text(" end.")

#text("N11 ")#metadata(none)<__indexterm-11>#text(" end.")

#text("N12 ")#metadata(none)<__indexterm-12>#text("Before ")#metadata(none)<id-696e6e6572>#text(" after")#text(" end. See ")#context link(query(<id-696e6e6572>).first().location())[#text("Inner")]#text(".")

#text("N13 ")#metadata(none)<__indexterm-13>#text("Before ")#strong[\[ #text("Save") \]]#text(" after")#text(" end.")

#text("N14 ")#metadata(none)<__indexterm-14>#text("Before ")#box(baseline: 15%, fill: rgb("#f3f4f6"), stroke: 0.5pt + rgb("#e5e7eb"), radius: 2pt, inset: (x: 2pt, y: 0.5pt))[#raw("Ctrl")]#text(" + ")#box(baseline: 15%, fill: rgb("#f3f4f6"), stroke: 0.5pt + rgb("#e5e7eb"), radius: 2pt, inset: (x: 2pt, y: 0.5pt))[#raw("C")]#text(" after")#text(" end.")

#text("N15 ")#metadata(none)<__indexterm-15>#text("Before ")#strong[#text("File")#text(" ")#text(size: 1.15em, fill: rgb("#374151"))[›]#text(" ")#text("Open")#text(" ")#text(size: 1.15em, fill: rgb("#374151"))[›]#text(" ")#text("Recent")]#text(" after")#text(" end.")

#text("N16 ")#metadata(none)<__indexterm-16>#text("Before ")#context link(query(<id-746172676574>).first().location())[#text("Target] text")]#text(" after")#text(" end.")

#text("N17 ")#metadata(none)<__indexterm-17>#text("Before ")#link("https://example.org/format")[#strong[#text("Bold")]]#text(" after")#text(" end.")

#text("N18 ")#metadata(none)<__indexterm-18>#text("Before ")#link("https://outer.example")[#text("Outer ")#link("mailto:inner@example.org")[#text("Inner")]#text(" tail")]#text(" after")#text(" end.")

#text("N19 ")#metadata(none)<__indexterm-19>#text("Before indexterm2:[Nested")#text(" after] end.")

#text("N20 ")#metadata(none)<__indexterm-20>#text("Before ")#footnote(<id-666f6f746e6f74653a6f6e65>)#text(" after")#text(" end.")

#text("N21 ")#metadata(none)<__indexterm-21>#text("Before ")#link("https://example.org/unicode")[#text("café")]#text(" after")#text(" end.")

#text("N22 ")#metadata(none)<__indexterm-22>#text("Before ")#link("https://example.org/bracket")[#text("Bracket")#text("]")#text(" label")]#text(" after")#text(" end.")

#text("N23 ")#metadata(none)<__indexterm-23>#text("Before ")#link("https://example.org/multiline")[#text("Multiline label")]#text(" after")#text(" end.")

#text("N24 ")#metadata(none)<__indexterm-24>#text("Before ")#link("https://one.example")[#text("One")]#text(" and ")#link("https://two.example")[#text("Two")]#text(" after")#text(" end.")

#text("N25 ")#metadata(none)<__indexterm-25>#text("Before ")#strong[#text("Protected")]#text(" after")#text(" end.")

#text("N26 ")#metadata(none)<__indexterm-26>#text(" end.")

#text("N27 ")#text("indexterm2:[Before ")#link("https://example.org/outer-escape")[#text("Link")]#text(" after]")#text(" end.")

#text("N28 ")#metadata(none)<__indexterm-27>#text("Before unknown:[Literal")#text(" after] end.")

#text("N29 ")#metadata(none)<__indexterm-28>#text("Before ")#link("https://example.org/quoted")[#text("Label, comma")]#text(" after")#text(" end.")

#text("N30 ")#metadata(none)<__indexterm-29>#text("First ")#link("https://example.org/adjacent")[#text("Link")]#metadata(none)<__indexterm-30>#text("Second")#text(".")

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
#text(weight: "bold")[#text("B")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Before <<target,Target] text>> after")#_acdc_index_pages((<__indexterm-16>,), "term")]
#par(hanging-indent: 1em)[#text("Before [literal")#_acdc_index_pages((<__indexterm-6>,), "term")]
#par(hanging-indent: 1em)[#text("Before [literal")#text("]")#text(" after")#_acdc_index_pages((<__indexterm-7>,), "term")]
#par(hanging-indent: 1em)[#text("Before \\https://example.org/inactive[Link] after")#_acdc_index_pages((<__indexterm-10>,), "term")]
#par(hanging-indent: 1em)[#text("Before anchor:inner[Ref] after")#_acdc_index_pages((<__indexterm-12>,), "term")]
#par(hanging-indent: 1em)[#text("Before ")#box(baseline: 15%, fill: rgb("#f3f4f6"), stroke: 0.5pt + rgb("#e5e7eb"), radius: 2pt, inset: (x: 2pt, y: 0.5pt))[#raw("Ctrl")]#text(" + ")#box(baseline: 15%, fill: rgb("#f3f4f6"), stroke: 0.5pt + rgb("#e5e7eb"), radius: 2pt, inset: (x: 2pt, y: 0.5pt))[#raw("C")]#text(" after")#_acdc_index_pages((<__indexterm-14>,), "term")]
#par(hanging-indent: 1em)[#text("Before ")#strong[#text("File")#text(" ")#text(size: 1.15em, fill: rgb("#374151"))[›]#text(" ")#text("Open")#text(" ")#text(size: 1.15em, fill: rgb("#374151"))[›]#text(" ")#text("Recent")]#text(" after")#_acdc_index_pages((<__indexterm-15>,), "term")]
#par(hanging-indent: 1em)[#text("Before footnote:one[] after")#_acdc_index_pages((<__indexterm-20>,), "term")]
#par(hanging-indent: 1em)[#text("Before footnote:one[Only note.] after")#_acdc_index_pages((<__indexterm-5>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://example.org/bracket[Bracket")#text("]")#text(" label] after")#_acdc_index_pages((<__indexterm-22>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://example.org/escaped[Link")#text("]")#text(" after")#_acdc_index_pages((<__indexterm-9>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://example.org/format[")#strong[#text("Bold")]#text("] after")#_acdc_index_pages((<__indexterm-17>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://example.org/incomplete[Link")#_acdc_index_pages((<__indexterm-8>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://example.org/multiline[Multiline label] after")#_acdc_index_pages((<__indexterm-23>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://example.org/quoted[\"Label, comma\",role=term] after")#_acdc_index_pages((<__indexterm-28>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://example.org/unicode[café] after")#_acdc_index_pages((<__indexterm-21>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://example.org/url[Link] after")#_acdc_index_pages((<__indexterm-1>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://one.example[One] and https://two.example[Two] after")#_acdc_index_pages((<__indexterm-24>,), "term")]
#par(hanging-indent: 1em)[#text("Before https://outer.example[Outer mailto:inner@example.org[Inner] tail] after")#_acdc_index_pages((<__indexterm-18>,), "term")]
#par(hanging-indent: 1em)[#text("Before indexterm2:[Nested")#_acdc_index_pages((<__indexterm-19>,), "term")]
#par(hanging-indent: 1em)[#text("Before link:https://example.org/link[Link] after")#_acdc_index_pages((<__indexterm-2>,), "term")]
#par(hanging-indent: 1em)[#text("Before mailto:user@example.org[Mail] after")#_acdc_index_pages((<__indexterm-3>,), "term")]
#par(hanging-indent: 1em)[#text("Before ")#strong[#text("Protected")]#text(" after")#_acdc_index_pages((<__indexterm-25>,), "term")]
#par(hanging-indent: 1em)[#text("Before ")#strong[\[ #text("Save") \]]#text(" after")#_acdc_index_pages((<__indexterm-13>,), "term")]
#par(hanging-indent: 1em)[#text("Before unknown:[Literal")#_acdc_index_pages((<__indexterm-27>,), "term")]
#par(hanging-indent: 1em)[#text("Before xref:target[Target] after")#_acdc_index_pages((<__indexterm-4>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("C")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Category")]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#text("https://example.org/comma[Link, label]")]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[#text("Tail")#_acdc_index_pages((<__indexterm-11>,), "term")]]
#pad(left: 1 * 1.25em)[#par(hanging-indent: 1em)[#text("https://example.org/unquoted[Link")]]
#pad(left: 2 * 1.25em)[#par(hanging-indent: 1em)[#text("label]")#_acdc_index_pages((<__indexterm-26>,), "term")]]
#v(0.75em)
#text(weight: "bold")[#text("F")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("First https://example.org/adjacent[Link]")#_acdc_index_pages((<__indexterm-29>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("S")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Second")#_acdc_index_pages((<__indexterm-30>,), "term")]
]
