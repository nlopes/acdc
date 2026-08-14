#set document(
  title: "Source line options",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Source line options]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#let docimage(path, width: none, ratio: none, destination: none) = block(width: 100%, radius: 4pt, clip: true, layout(size => {
  let resolved-width = if ratio != none { ratio * size.width } else if width != none { calc.min(width, size.width) } else { size.width }
  let content = image(path, width: resolved-width)
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

#align(center)[
#text(size: 22pt, weight: "bold")[#text("Source line options")]
]
#v(1em)

#heading(level: 1)[#text("Number forms")] <id-5f6e756d6265725f666f726d73>

#{
  let numbers = (1, 2, )
  let highlighted = (false, false, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn positional() {}\nfn second() {}")
}

#{
  let numbers = (10, 11, )
  let highlighted = (false, false, )
  let gutter = 1.2em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn ten() {}\nfn eleven() {}")
}

#{
  let numbers = (1, )
  let highlighted = (false, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn invalid_start() {}")
}

#heading(level: 1)[#text("Highlight selectors")] <id-5f686967686c696768745f73656c6563746f7273>

#{
  let numbers = none
  let highlighted = (false, true, false, true, )
  let gutter = 0.0em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}")
}

#{
  let numbers = (20, 21, 22, )
  let highlighted = (true, false, true, )
  let gutter = 1.2em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn twenty() {}\nfn twenty_one() {}\nfn twenty_two() {}")
}

#{
  let numbers = none
  let highlighted = (true, false, true, true, )
  let gutter = 0.0em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn range_one() {}\nfn range_two() {}\nfn range_three() {}\nfn range_four() {}")
}

#{
  let numbers = none
  let highlighted = (false, false, true, true, )
  let gutter = 0.0em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn open_one() {}\nfn open_two() {}\nfn open_three() {}\nfn open_four() {}")
}

#heading(level: 1)[#text("Source forms")] <id-5f736f757263655f666f726d73>

#{
  let numbers = (1, 2, )
  let highlighted = (false, true, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, "plain one\nplain two")
}

#{
  let numbers = (1, 2, )
  let highlighted = (false, true, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn paragraph_one() {}\nfn paragraph_two() {}")
}

#{
  let numbers = (1, 2, )
  let highlighted = (false, true, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn callout_one() {} (1)\nfn callout_two() {} (2)")
}

#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },
[#text("(1)")], [#text("First callout.")],
[#text("(2)")], [#text("Second callout.")],
)

#heading(level: 1)[#text("Document options")] <id-5f646f63756d656e745f6f7074696f6e73>

#{
  let numbers = (1, 2, )
  let highlighted = (false, false, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn global_one() {}\nfn global_two() {}")
}

#raw(block: true, lang: "rust", "fn after_unset() {}")

#heading(level: 1)[#text("Wrapping")] <id-5f7772617070696e67>

#{
  let numbers = (1, none, )
  let highlighted = (true, true, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn a_very_long_function_name_that_wraps_but_keeps_one_number_and_a_highlighted_con\ntinuation() {}")
}

#{
  let numbers = (1, none, )
  let highlighted = (false, false, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn local_nowrap_is_ignored_by_the_pdf_backend_and_this_line_still_wraps_across_mul\ntiple_visual_lines() {}")
}

#{
  let numbers = (1, none, )
  let highlighted = (false, false, )
  let gutter = 0.6em
  show raw.line: line => {
    let index = line.number - 1
    let marked = highlighted.at(index, default: false)
    let code-width = if numbers == none { 100% } else { 100% - gutter - 0.8em }
    let code = box(width: code-width, fill: if marked { rgb("#374151") } else { none }, line.body)
    if numbers == none {
      code
    } else {
      let number = numbers.at(index, default: none)
      box(width: gutter, align(right + top, if number == none { [] } else { text(fill: rgb("#9ca3af"), str(number)) })) + h(0.8em) + code
    }
  }
  raw(block: true, lang: "rust", "fn global_nowrap_is_ignored_by_the_pdf_backend_and_this_line_still_wraps_across_mu\nltiple_visual_lines() {}")
}
