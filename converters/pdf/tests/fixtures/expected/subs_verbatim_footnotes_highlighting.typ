#set document(
  title: "Verbatim footnotes",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Verbatim footnotes]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Verbatim footnotes")]
]
#v(1em)

#text(size: 1.25em)[#text("Before ")#counter(footnote).update(0)#footnote[#text("Shared note.")]<id-666f6f746e6f74653a736861726564>]

#{
  let index-anchors = (
    [],
    [],
    [],
    [],
    [],
    [#metadata(none)<__indexterm-2>],
  )
  // Slice highlighted text without discarding its syntax styles.
  let code-slice(body, start, end) = {
    if body.has("text") {
      let size = body.text.len()
      (text(body.text.slice(calc.min(start, size), calc.min(end, size))), size)
    } else if body.has("children") {
      let offset = 0
      let parts = []
      for child in body.children {
        let (part, size) = code-slice(child, calc.max(0, start - offset), calc.max(0, end - offset))
        parts += part
        offset += size
      }
      (parts, offset)
    } else if body.has("child") {
      let (child, size) = code-slice(body.child, start, end)
      (body.func()(child, body.styles), size)
    } else {
      ([], 0)
    }
  }
  let links = (
    ((11, 14, body => [#counter(footnote).update(1)#footnote[#text("Code note with *bold* and ")#metadata(none) <__indexterm-1>#text("InsideNote")#text(".")]<id-666f6f746e6f74653a636f6465>]), ),
    ((6, 9, body => [#footnote(<id-666f6f746e6f74653a636f6465>)]), (14, 17, body => [#footnote(<id-666f6f746e6f74653a736861726564>)]), ),
    ((9, 12, body => [#footnote(<id-666f6f746e6f74653a636f6465>)]), ),
    ((9, 12, body => [#counter(footnote).update(2)#footnote[#text("Adjacent one.")]]), (12, 15, body => [#counter(footnote).update(3)#footnote[#text("Adjacent two.")]]), ),
    (),
    (),
  )
  let code-links(line) = {
    let start = 0
    let body = []
    for (from, to, make-link) in links.at(line.number - 1, default: ()) {
      body += code-slice(line.body, start, from).first()
      body += make-link(code-slice(line.body, from, to).first())
      start = to
    }
    body + code-slice(line.body, start, line.text.len()).first()
  }
  show raw.line: line => index-anchors.at(line.number - 1, default: []) + code-links(line)
  raw(block: true, lang: "rust", "let x = 1; ￼\nReuse ￼ and ￼\nRepeated ￼\nAdjacent ￼￼\nMissing [missing] and empty footnote:[]\nEscaped footnote:[Literal [brackets] EscapedBodyTerm]")
}
#{
  // Slice highlighted text without discarding its syntax styles.
  let code-slice(body, start, end) = {
    if body.has("text") {
      let size = body.text.len()
      (text(body.text.slice(calc.min(start, size), calc.min(end, size))), size)
    } else if body.has("children") {
      let offset = 0
      let parts = []
      for child in body.children {
        let (part, size) = code-slice(child, calc.max(0, start - offset), calc.max(0, end - offset))
        parts += part
        offset += size
      }
      (parts, offset)
    } else if body.has("child") {
      let (child, size) = code-slice(body.child, start, end)
      (body.func()(child, body.styles), size)
    } else {
      ([], 0)
    }
  }
  let links = (
    ((10, 13, body => [#counter(footnote).update(4)#footnote[#text("Literal block note.")]]), ),
  )
  let code-links(line) = {
    let start = 0
    let body = []
    for (from, to, make-link) in links.at(line.number - 1, default: ()) {
      body += code-slice(line.body, start, from).first()
      body += make-link(code-slice(line.body, from, to).first())
      start = to
    }
    body + code-slice(line.body, start, line.text.len()).first()
  }
  show raw.line: line => code-links(line)
  raw(block: true, "  Literal ￼")
}
#{
  // Slice highlighted text without discarding its syntax styles.
  let code-slice(body, start, end) = {
    if body.has("text") {
      let size = body.text.len()
      (text(body.text.slice(calc.min(start, size), calc.min(end, size))), size)
    } else if body.has("children") {
      let offset = 0
      let parts = []
      for child in body.children {
        let (part, size) = code-slice(child, calc.max(0, start - offset), calc.max(0, end - offset))
        parts += part
        offset += size
      }
      (parts, offset)
    } else if body.has("child") {
      let (child, size) = code-slice(body.child, start, end)
      (body.func()(child, body.styles), size)
    } else {
      ([], 0)
    }
  }
  let links = (
    ((7, 10, body => [#counter(footnote).update(5)#footnote[#text("Styled source note.")]]), ),
  )
  let code-links(line) = {
    let start = 0
    let body = []
    for (from, to, make-link) in links.at(line.number - 1, default: ()) {
      body += code-slice(line.body, start, from).first()
      body += make-link(code-slice(line.body, from, to).first())
      start = to
    }
    body + code-slice(line.body, start, line.text.len()).first()
  }
  show raw.line: line => code-links(line)
  raw(block: true, "Styled ￼")
}
#{
  // Slice highlighted text without discarding its syntax styles.
  let code-slice(body, start, end) = {
    if body.has("text") {
      let size = body.text.len()
      (text(body.text.slice(calc.min(start, size), calc.min(end, size))), size)
    } else if body.has("children") {
      let offset = 0
      let parts = []
      for child in body.children {
        let (part, size) = code-slice(child, calc.max(0, start - offset), calc.max(0, end - offset))
        parts += part
        offset += size
      }
      (parts, offset)
    } else if body.has("child") {
      let (child, size) = code-slice(body.child, start, end)
      (body.func()(child, body.styles), size)
    } else {
      ([], 0)
    }
  }
  let links = (
    ((15, 18, body => [#counter(footnote).update(6)#footnote[#text("Styled literal note.")]]), ),
  )
  let code-links(line) = {
    let start = 0
    let body = []
    for (from, to, make-link) in links.at(line.number - 1, default: ()) {
      body += code-slice(line.body, start, from).first()
      body += make-link(code-slice(line.body, from, to).first())
      start = to
    }
    body + code-slice(line.body, start, line.text.len()).first()
  }
  show raw.line: line => code-links(line)
  raw(block: true, "Styled literal ￼")
}
#{
  // Slice highlighted text without discarding its syntax styles.
  let code-slice(body, start, end) = {
    if body.has("text") {
      let size = body.text.len()
      (text(body.text.slice(calc.min(start, size), calc.min(end, size))), size)
    } else if body.has("children") {
      let offset = 0
      let parts = []
      for child in body.children {
        let (part, size) = code-slice(child, calc.max(0, start - offset), calc.max(0, end - offset))
        parts += part
        offset += size
      }
      (parts, offset)
    } else if body.has("child") {
      let (child, size) = code-slice(body.child, start, end)
      (body.func()(child, body.styles), size)
    } else {
      ([], 0)
    }
  }
  let links = (
    ((9, 12, body => [#counter(footnote).update(7)#footnote[#text("Indented note.")]]), ),
  )
  let code-links(line) = {
    let start = 0
    let body = []
    for (from, to, make-link) in links.at(line.number - 1, default: ()) {
      body += code-slice(line.body, start, from).first()
      body += make-link(code-slice(line.body, from, to).first())
      start = to
    }
    body + code-slice(line.body, start, line.text.len()).first()
  }
  show raw.line: line => code-links(line)
  raw(block: true, "Indented ￼")
}
#{
  // Slice highlighted text without discarding its syntax styles.
  let code-slice(body, start, end) = {
    if body.has("text") {
      let size = body.text.len()
      (text(body.text.slice(calc.min(start, size), calc.min(end, size))), size)
    } else if body.has("children") {
      let offset = 0
      let parts = []
      for child in body.children {
        let (part, size) = code-slice(child, calc.max(0, start - offset), calc.max(0, end - offset))
        parts += part
        offset += size
      }
      (parts, offset)
    } else if body.has("child") {
      let (child, size) = code-slice(body.child, start, end)
      (body.func()(child, body.styles), size)
    } else {
      ([], 0)
    }
  }
  let links = (
    ((0, 3, body => [#counter(footnote).update(8)#footnote[#text("Only note.")]<id-666f6f746e6f74653a6f6e6c79>]), ),
  )
  let code-links(line) = {
    let start = 0
    let body = []
    for (from, to, make-link) in links.at(line.number - 1, default: ()) {
      body += code-slice(line.body, start, from).first()
      body += make-link(code-slice(line.body, from, to).first())
      start = to
    }
    body + code-slice(line.body, start, line.text.len()).first()
  }
  show raw.line: line => code-links(line)
  raw(block: true, lang: "rust", "￼")
}
#{
  // Slice highlighted text without discarding its syntax styles.
  let code-slice(body, start, end) = {
    if body.has("text") {
      let size = body.text.len()
      (text(body.text.slice(calc.min(start, size), calc.min(end, size))), size)
    } else if body.has("children") {
      let offset = 0
      let parts = []
      for child in body.children {
        let (part, size) = code-slice(child, calc.max(0, start - offset), calc.max(0, end - offset))
        parts += part
        offset += size
      }
      (parts, offset)
    } else if body.has("child") {
      let (child, size) = code-slice(body.child, start, end)
      (body.func()(child, body.styles), size)
    } else {
      ([], 0)
    }
  }
  let links = (
    ((0, 3, body => [#footnote(<id-666f6f746e6f74653a6f6e6c79>)]), ),
    ((0, 3, body => [#counter(footnote).update(9)#footnote[#text("Last line note.")]]), ),
  )
  let code-links(line) = {
    let start = 0
    let body = []
    for (from, to, make-link) in links.at(line.number - 1, default: ()) {
      body += code-slice(line.body, start, from).first()
      body += make-link(code-slice(line.body, from, to).first())
      start = to
    }
    body + code-slice(line.body, start, line.text.len()).first()
  }
  let numbers = (1, 2, )
  let highlighted = (false, false, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, code-links(line))
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "￼\n￼")
}

#{
  let index-anchors = (
    [],
    [],
    [#metadata(none)<__indexterm-3>],
  )
  // Slice highlighted text without discarding its syntax styles.
  let code-slice(body, start, end) = {
    if body.has("text") {
      let size = body.text.len()
      (text(body.text.slice(calc.min(start, size), calc.min(end, size))), size)
    } else if body.has("children") {
      let offset = 0
      let parts = []
      for child in body.children {
        let (part, size) = code-slice(child, calc.max(0, start - offset), calc.max(0, end - offset))
        parts += part
        offset += size
      }
      (parts, offset)
    } else if body.has("child") {
      let (child, size) = code-slice(body.child, start, end)
      (body.func()(child, body.styles), size)
    } else {
      ([], 0)
    }
  }
  let links = (
    ((5, 8, body => [#counter(footnote).update(10)#footnote[#text("Bold note.")]]), ),
    ((0, 6, body => link("https://example.org", body)), (6, 9, body => [#counter(footnote).update(11)#footnote[#text("Link label note.")]]), (9, 13, body => link("https://example.org", body)), ),
    ((8, 11, body => [#counter(footnote).update(12)#footnote[#text("Index label note.")]]), ),
  )
  let code-links(line) = {
    let start = 0
    let body = []
    for (from, to, make-link) in links.at(line.number - 1, default: ()) {
      body += code-slice(line.body, start, from).first()
      body += make-link(code-slice(line.body, from, to).first())
      start = to
    }
    body + code-slice(line.body, start, line.text.len()).first()
  }
  show raw.line: line => index-anchors.at(line.number - 1, default: []) + code-links(line)
  raw(block: true, lang: "rust", "Bold ￼\nLabel ￼ end\nIndexed ￼")
}
#raw(block: true, lang: "rust", "footnote:[Disabled note.]")

#raw(block: true, lang: "rust", "footnote:[Explicitly disabled note.]")

#text("After ")#footnote(<id-666f6f746e6f74653a636f6465>)#text(" and ")#counter(footnote).update(13)#footnote[#text("Final note.")]

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
#text(weight: "bold")[#text("E")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("EscapedBodyTerm")#_acdc_index_pages((<__indexterm-2>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("I")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Indexed footnote:[Index label note.]")#_acdc_index_pages((<__indexterm-3>,), "term")]
#par(hanging-indent: 1em)[#text("InsideNote")#_acdc_index_pages((<__indexterm-1>,), "term")]
]
