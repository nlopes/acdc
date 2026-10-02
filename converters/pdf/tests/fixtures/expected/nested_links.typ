#set document(
  title: "nested-links(1)",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[nested-links(1)]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("nested-links(1)")]
]
#v(1em)

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
        counter(page).display(at: target),
      ),
    ),
  )
}
#_acdc_toc_entry(<id-5f6e616d65>, 0, [#text("Name")])
#_acdc_toc_entry(<id-5f73796e6f70736973>, 0, [#text("Synopsis")])
#_acdc_toc_entry(<id-5f6465736372697074696f6e>, 0, [#text("Description")])
#_acdc_toc_entry(<id-746172676574>, 1, [#text("Target ")#link("mailto:title@example.org")[#text("Title link")]])
#_acdc_toc_entry(<id-5f696e646578>, 0, [#text("Index")])
#pagebreak()

#heading(level: 1)[#text("Name")] <id-5f6e616d65>

#text("nested-links - links inside labels")

#heading(level: 1)[#text("Synopsis")] <id-5f73796e6f70736973>

#text("Nested link examples.")

#heading(level: 1)[#text("Description")] <id-5f6465736372697074696f6e>

#link("https://outer.example")[#text("Before ")#link("mailto:test@example.org")[#text("Inner")]#text(" after")]#text(".")

#text(fill: rgb("#006000"))[#link("https://quoted.example")[#text("Outer ")#link("mailto:test@example.org")[#text("\\\"Quoted\\\"")]#text(" tail")]]

#link("https://outer.example")[#text("Before ")#strong[#text("bold ")#link("mailto:bold@example.org")[#emph[#text("inner")]]#text(" end")]#text(" after")]#text(".")

#link("https://outer.example")[#text("Before ")#link("https://middle.example")[#text("Middle ")#link("mailto:deep@example.org")[#text("Deep")]#text(" tail")]#text(" after")]#text(".")

#link("mailto:outer@example.org")[#text("Before ")#link("https://inner.example")[#text("Inner")]#text(" after")]#text(".")

#link("https://outer.example")[#text("Before https://bare.example after")]#text(".")

#link("https://outer.example")[#text("Before ")#link("mailto:empty@example.org")[#text("empty@example.org")]#text(" after")]#text(".")

#metadata(none)<id-6f757465722d6964>#text(fill: rgb("#006000"))[#link("https://outer.example")[#link("mailto:first@example.org")[#text("First")]#text(" after")]]

#metadata(none)<id-6f6e6c792d6964>#link("https://outer.example")[#link("mailto:only@example.org")[#text("Only")]]

#link("https://outer.example")[#strong[#link("mailto:styled@example.org")[#text("Styled only")]]]

#underline[#link("https://outer.example")[#link("mailto:role@example.org")[#text("Role only")]]]

#link("https://outer.example")[#text("Before ")#link("mailto:test@example.org")[#text("Inner")]#text(",after")]#text(".")

#link("https://outer.example")[#text("Before ")#link("mailto:test@example.org")[#text("Inner")]#strong[#link("https://adjacent.example")[#text("Adjacent")]]#text(" after")]#text(".")

#link("https://outer.example")[#text("Before ")#context link(query(<id-746172676574>).first().location())[#text("Explicit")]#text(" and ")#context link(query(<id-746172676574>).first().location())[#text("Target ")#link("mailto:title@example.org")[#text("Title link")]]#text(" after")]#text(".")

#context link(query(<id-746172676574>).first().location())[#text("Before ")#link("mailto:ref@example.org")[#text("Inner")]#text(" after")]

#link("https://outer.example")[#text("Before ")#counter(footnote).update(0)#footnote[#text("Footnote with ")#link("https://note.example")[#text("Note")]#text(".")]<id-666f6f746e6f74653a6e6573746564>#text(" ")#metadata(none)<id-6c6162656c2d616e63686f72>#link("mailto:test@example.org")[#text("Inner")]#text(" after")]#text(".")

#link("https://outer.example")[#text("Début ")#link("https://unicode.example")[#text("Été & soleil")]#text(" fin")]#text(".")

#heading(level: 2)[#text("Target ")#link("mailto:title@example.org")[#text("Title link")]] <id-746172676574>

#text("See ")#context link(query(<id-746172676574>).first().location())[#text("Target ")#link("mailto:title@example.org")[#text("Title link")]]#text(", ")#context link(query(<id-6f757465722d6964>).first().location())[#text("Outer")]#text(", ")#context link(query(<id-6f6e6c792d6964>).first().location())[#text("Only")]#text(", and ")#context link(query(<id-6c6162656c2d616e63686f72>).first().location())[#text("Anchor")]#text(".")

#link("https://outer.example")[#text("Before ")#box(link("https://image.example")[#image("/images/de454d7e4e1cfda7.svg", alt: "Picture")])#text(" after")]#text(".")

#link("https://outer.example")[#text("Before ")#strong[#text("bold ")#metadata(none)<__indexterm-1>#text("Index ")#link("mailto:index@example.org")[#text("Inner")]]#text(" ")#metadata(none)<__indexterm-2>#text(" after")]#text(".")

#text("Ordinary ")#link("https://control.example")[#text("Control")]#text(" and ")#link("mailto:control@example.org")[#text("Mail")]#text(".")

#raw(block: true, "link:https://literal.example[Outer mailto:literal@example.org[Inner] tail]")

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
#text(weight: "bold")[#text("H")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Hidden")#_acdc_index_pages((<__indexterm-2>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("I")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Index mailto:index@example.org[Inner]")#_acdc_index_pages((<__indexterm-1>,), "term")]
]
