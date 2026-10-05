#set document(
  title: "link-formatting(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[link-formatting(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("link-formatting(1)")]
]
#v(1em)

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("link-formatting - preserve formatting and complete links")

#heading(level: 1)[#text("Description")] <id-5f6465736372697074696f6e>

#text("L01 ")#strong[#text("Before ")#link("https://example.org/bold")[#strong[#text("Bold")]#text(" after")]#text(" end")]#text(".")

#text("L02 ")#emph[#text("Before ")#link("https://example.org/italic")[#emph[#text("Italic")]#text(" after")]#text(" end")]#text(".")

#text("L03 ")#raw("Before ")#link("https://example.org/code")[#raw("Code after")]#raw(" end")#text(".")

#text("L04 ")#highlight[#text("Before ")#link("https://example.org/mark")[#highlight[#text("Marked")]#text(" after")]#text(" end")]#text(".")

#text("L05 ")#strong[#text("Before ")#link("manual.html")[#strong[#text("Manual")]#text(" after")]#text(" end")]#text(".")

#text("L06 ")#emph[#text("Before ")#link("mailto:user@example.org")[#emph[#text("Mail")]#text(" after")]#text(" end")]#text(".")

#text("L07 ")#emph[#text("Before ")#context link(query(<id-746172676574>).first().location())[#emph[#text("Reference")]#text(" after")]#text(" end")]#text(".")

#text("L08 ")#strong[#text("Before ")#context link(query(<id-746172676574>).first().location())[#strong[#text("Reference")]#text(" after")]#text(" end")]#text(".")

#text("L09 ")#strong[#text("Before ")#link("https://outer.example")[#text("Before ")#strong[#text("bold ")#link("mailto:inner@example.org")[#text("Inner")]#text(" tail")]#text(" after")]#text(" end")]#text(".")

#text("L10 ")#strong[#text("Before ")#link("https://example.org/note")[#strong[#text("Note ")#counter(footnote).update(0)#footnote[#text("One note.")]<id-666f6f746e6f74653a6f6e65>#text(" ")#metadata(none)<__indexterm-1>#text("Indexed")]#text(" after")]#text(" end")]#text(". Reuse ")#footnote(<id-666f6f746e6f74653a6f6e65>)#text(".")

#text("L11 ")#strong[#text("Before ")#link("https://one.example")[#strong[#text("One")]]#text(" and ")#link("https://two.example")[#strong[#text("Two")]]#text(" end")]#text(".")

#text("L12 ")#strong[#text("Before ")#link("https://example.org/bracket")[#strong[#text("Bracket")#text("]")#text(" text")]#text(" after")]#text(" end")]#text(".")

#text("L13 ")#strong[#text("Before ")#text("https://escaped.example[*Literal* after]")#text(" end")]#text(".")

#text("L14 ")#strong[#text("Before ")#link("https://example.org/quoted")[#strong[#text("Quoted")]#text(" after")]#text(" end")]#text(".")

#text("L15 ")#strong[#text("Before ")#link("https://example.org/attribute")[#text("*Expanded* after")]#text(" end")]#text(".")

#text("L16 ")#emph[#link("https://example.org/edge")[#emph[#text("Edge")]]]#text(".")

#text("L17 *Before ")#link("https://example.org/unclosed")[#text("Unclosed* after")]#text(".")

#text("L18 ")#strong[#text("Before ")#link("https://example.org/inner")[#text("*Unclosed after")]#text(" end")]#text(".")

#text("L19 ")#strong[#text("Before ")#link("https://example.org/incomplete")[#text("https://example.org/incomplete")]#text("[Incomplete")]#text(" after.")

#text("L20 ")#strong[#text("Éva ")#link("https://example.org/unicode")[#strong[#text("café")]#text(" after")]#text(" τέλος")]#text(".")

#text("L21 ")#strong[#text("Before ")#link("https://example.org/escaped")[#text("*")#text("Literal")#text("*")#text(" after")]#text(" end")]#text(".")

#text("L22 ")#strong[#text("Before ")#link("https://example.org/multiline")[#strong[#text("Multiline label")]#text(" after")]#text(" end")]#text(".")

#text("L23 ")#strong[#text("Before ")#link("https://example.org/query?pattern=*")[#strong[#text("Query")]#text(" after")]#text(" end")]#text(".")

#text("L24 ")#strong[#text("Before ")#link("https://example.org/role")[#strong[#text("Role")]#text(" after")]#text(" end")]#text(".")

#text("L25 ")#strong[#text("Before ")#link("https://example.org/brackets")[#text("Before [")#strong[#text("nested")]#text("] after")]#text(" end")]#text(".")

#text("L26 ")#strong[#text("Before ")#link("https://example.org/control")[#text("Plain")]#text(" end")]#text(".")

#text("L27 ")#link("https://example.org/control-label")[#strong[#text("Bold")]#text(" after")]#text(".")

#text("L28 ")#strong[#text("First")]#text(" ")#link("https://example.org/separate")[#text("Label")]#text(" ")#strong[#text("Second")]#text(".")

#heading(level: 1)[#text("Target")] <id-746172676574>

#text("Target text.")

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
#text(weight: "bold")[#text("I")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Indexed")#_acdc_index_pages((<__indexterm-1>,), "term")]
]
