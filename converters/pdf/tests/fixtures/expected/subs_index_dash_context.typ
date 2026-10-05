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

#text("S01 prefix")#metadata(none)<__indexterm-1>#text("—​")#text("tail")

#text("S02 prefix")#metadata(none)<__indexterm-2>#text("—​")#text("tail")

#text("S03 prefix")#metadata(none)<__indexterm-3>#text("--")#text("tail")

#text("S04 prefixindexterm2:[--]tail")

#text("S05 prefix")#metadata(none)<__indexterm-4>#text("—​")#text("tail")

#text("S06 prefix")#metadata(none)<__indexterm-5>#text("--")#text("tail")

#{
  let index-anchors = (
    [#metadata(none)<__indexterm-6>],
    [#metadata(none)<__indexterm-7>],
    [#metadata(none)<__indexterm-8>],
    [#metadata(none)<__indexterm-9>#metadata(none)<__indexterm-10>],
    [#metadata(none)<__indexterm-11>],
    [#metadata(none)<__indexterm-12>#metadata(none)<__indexterm-13>],
    [#metadata(none)<__indexterm-14>],
    [#metadata(none)<__indexterm-15>],
    [#metadata(none)<__indexterm-16>],
    [#metadata(none)<__indexterm-17>],
    [#metadata(none)<__indexterm-18>],
    [#metadata(none)<__indexterm-19>],
    [#metadata(none)<__indexterm-20>],
    [#metadata(none)<__indexterm-21>],
    [#metadata(none)<__indexterm-22>],
    [#metadata(none)<__indexterm-23>],
    [#metadata(none)<__indexterm-24>],
    [#metadata(none)<__indexterm-25>],
    [#metadata(none)<__indexterm-26>],
    [#metadata(none)<__indexterm-27>],
    [#metadata(none)<__indexterm-28>],
    [#metadata(none)<__indexterm-29>],
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
    ((21, 27, body => link("https://next.example/07", body)), ),
    ((21, 27, body => link("https://next.example/08", body)), ),
    ((21, 27, body => link("https://next.example/09", body)), ),
    ((21, 27, body => link("https://next.example/10", body)), ),
    ((10, 10, body => [#metadata(none)<id-636f64652d6265666f7265>] + []), (16, 16, body => [#metadata(none)<id-636f64652d6166746572>] + []), (21, 27, body => link("https://next.example/11", body)), ),
    ((29, 35, body => link("https://next.example/12", body)), ),
    ((20, 26, body => link("https://next.example/13", body)), ),
    ((21, 27, body => link("https://next.example/14", body)), ),
    ((17, 23, body => link("https://next.example/15", body)), ),
    ((18, 24, body => link("https://next.example/16", body)), ),
    ((18, 24, body => link("https://next.example/17", body)), ),
    ((18, 24, body => link("https://next.example/18", body)), ),
    ((13, 19, body => link("https://next.example/19", body)), ),
    ((11, 17, body => link("https://next.example/20", body)), ),
    ((18, 24, body => link("https://next.example/21", body)), ),
    ((18, 24, body => link("https://next.example/22", body)), ),
    ((24, 30, body => link("https://next.example/23", body)), ),
    ((17, 23, body => link("https://next.example/24", body)), ),
    ((21, 27, body => link("https://next.example/25", body)), ),
    ((17, 23, body => link("https://next.example/26", body)), ),
    ((12, 18, body => link("https://next.example/31", body)), ),
    ((4, 12, body => link("https://next.example/32", body)), ),
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
  raw(block: true, "S07 prefix—​tail Next07\nS08 prefix—​tail Next08\nS09 prefix—​tail Next09\nS10 prefix—​tail Next10\nS11 prefix—​tail Next11\nS12 prefixbegin—​endtail Next12\nS13 pré—​été Next13\nS14 word_—​_tail Next14\nS15 prefix--tail Next15\nS16 prefix---tail Next16\nS17 prefix---tail Next17\nS18 prefix---tail Next18\nS19 prefix-- Next19\nS20 --tail Next20\nS21 prefix.--tail Next21\nS22 prefix--.tail Next22\nS23 prefix — tail Next23\nS24 prefix--tail Next24\nS25 prefix—​tail Next25\nS26 prefix--tail Next26\nS31 prefix--Tail31\nS32 Prefix32--tail")
}
#{
  let index-anchors = (
    [#metadata(none)<__indexterm-30>],
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
    ((21, 27, body => link("https://next.example/27", body)), ),
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
  raw(block: true, "S27 prefix—​tail Next27")
}
#{
  let index-anchors = (
    [#metadata(none)<__indexterm-31>],
    [#metadata(none)<__indexterm-32>],
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
    ((17, 23, body => link("https://next.example/28", body)), ),
    ((21, 27, body => link("https://next.example/29", body)), ),
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
  raw(block: true, "S28 prefix--tail Next28\nS29 prefix—​tail Next29")
}
#raw(block: true, "S30 prefixindexterm2:[--]tail")

#{
  let index-anchors = (
    [#metadata(none)<__indexterm-33>],
  )
  show raw.line: line => index-anchors.at(line.number - 1, default: []) + line
  raw(block: true, " — ")
}
#{
  let index-anchors = (
    [#metadata(none)<__indexterm-34>],
  )
  show raw.line: line => index-anchors.at(line.number - 1, default: []) + line
  raw(block: true, "prefix--")
}
#{
  let index-anchors = (
    [#metadata(none)<__indexterm-35>],
    [#metadata(none)<__indexterm-36>],
    [#metadata(none)<__indexterm-37>#metadata(none)<__indexterm-38>],
    [#metadata(none)<__indexterm-39>#metadata(none)<__indexterm-40>],
    [#metadata(none)<__indexterm-41>],
    [#metadata(none)<__indexterm-42>],
    [#metadata(none)<__indexterm-43>],
    [#metadata(none)<__indexterm-44>],
    [#metadata(none)<__indexterm-45>],
    [#metadata(none)<__indexterm-46>],
    [#metadata(none)<__indexterm-47>],
    [#metadata(none)<__indexterm-48>],
    [#metadata(none)<__indexterm-49>],
    [#metadata(none)<__indexterm-50>#metadata(none)<__indexterm-51>],
    [#metadata(none)<__indexterm-52>],
    [#metadata(none)<__indexterm-53>],
    [#metadata(none)<__indexterm-54>],
    [#metadata(none)<__indexterm-55>],
    [#metadata(none)<__indexterm-56>],
    [#metadata(none)<__indexterm-57>#metadata(none)<__indexterm-58>],
    [#metadata(none)<__indexterm-59>#metadata(none)<__indexterm-60>],
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
    ((21, 27, body => link("https://next.example/33", body)), ),
    ((21, 27, body => link("https://next.example/34", body)), ),
    ((21, 27, body => link("https://next.example/35", body)), ),
    ((24, 30, body => link("https://next.example/36", body)), ),
    ((10, 10, body => [#metadata(none)<id-73706163652d6265666f7265>] + []), (19, 19, body => [#metadata(none)<id-73706163652d6166746572>] + []), (24, 30, body => link("https://next.example/37", body)), ),
    ((26, 32, body => link("https://next.example/38", body)), ),
    ((12, 18, body => link("https://next.example/39", body)), ),
    ((12, 18, body => link("https://next.example/40", body)), ),
    ((12, 18, body => link("https://next.example/41", body)), ),
    ((17, 23, body => link("https://next.example/42", body)), ),
    ((8, 14, body => link("https://next.example/43", body)), ),
    ((8, 14, body => link("https://next.example/44", body)), ),
    ((10, 16, body => link("https://next.example/45", body)), ),
    ((12, 18, body => link("https://next.example/46", body)), ),
    ((6, 6, body => [#metadata(none)<id-61706f7374726f706865>] + []), (12, 18, body => link("https://next.example/47", body)), ),
    ((17, 23, body => link("https://next.example/48", body)), ),
    ((19, 25, body => link("https://next.example/49", body)), ),
    ((19, 25, body => link("https://next.example/50", body)), ),
    ((19, 25, body => link("https://next.example/51", body)), ),
    ((35, 41, body => link("https://next.example/52", body)), ),
    ((33, 39, body => link("https://next.example/53", body)), ),
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
  raw(block: true, "S33 prefix—​tail Next33\nS34 prefix—​tail Next34\nS35 prefix—​tail Next35\nS36 prefix — tail Next36\nS37 prefix — tail Next37\nS38 prefix  —  tail Next38\nS39 Sam’s Next39\nS40 Sam’s Next40\nS41 Sam’s Next41\nS42 pré’été Next42\nS43 3'4 Next43\nS44 _'s Next44\nS45 Sam's Next45\nS46 Sam’s Next46\nS47 Sam’s Next47\nS48 prefix--tail Next48\nS49 prefix -- tail Next49\nS50 prefix -- tail Next50\nS51 prefix -- tail Next51\nS52 prefix — -- — tail Next52\nS53 prefix —  — tail Next53")
}
#text("S54 prefix")#metadata(none)<__indexterm-61>#text(" — ")#text("tail")

#text("S55 prefix")#metadata(none)<__indexterm-62>#text(" — ")#text("tail")

#text("S56 prefix ")#metadata(none)<__indexterm-63>#text("--")#text(" tail")

#text("S57 Sa")#metadata(none)<__indexterm-64>#text("m")#text("’s")

#text("S58 Sa")#metadata(none)<__indexterm-65>#text("m")#text("’s")

#text("S59 Sa")#metadata(none)<__indexterm-66>#text("m")#text("'s")

#text("S60 prefix—​")#metadata(none)<__indexterm-67>#text("")#text("tail")

#text("S61 prefix—​")#metadata(none)<__indexterm-68>#text("")#text("tail")

#text("S62 prefix-")#metadata(none)<__indexterm-69>#text("-")#text("tail")

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
    ((21, 27, body => link("https://next.example/63", body)), ),
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
  raw(block: true, "S63 prefix—​tail Next63")
}
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
#par(hanging-indent: 1em)[#text("'")#_acdc_index_pages((<__indexterm-45>,<__indexterm-46>,<__indexterm-47>,<__indexterm-48>,), "term")]
#par(hanging-indent: 1em)[#text("-")#_acdc_index_pages((<__indexterm-35>,<__indexterm-36>,<__indexterm-37>,<__indexterm-38>,<__indexterm-53>,<__indexterm-67>,<__indexterm-68>,<__indexterm-69>,), "term")]
#par(hanging-indent: 1em)[#text("--")#_acdc_index_pages((<__indexterm-1>,<__indexterm-3>,<__indexterm-5>,<__indexterm-6>,<__indexterm-10>,<__indexterm-11>,<__indexterm-14>,<__indexterm-15>,<__indexterm-18>,<__indexterm-19>,<__indexterm-20>,<__indexterm-21>,<__indexterm-22>,<__indexterm-23>,<__indexterm-24>,<__indexterm-25>,<__indexterm-28>,<__indexterm-29>,<__indexterm-31>,<__indexterm-33>,<__indexterm-34>,<__indexterm-40>,<__indexterm-41>,<__indexterm-42>,<__indexterm-54>,<__indexterm-55>,<__indexterm-57>,<__indexterm-58>,<__indexterm-59>,<__indexterm-60>,<__indexterm-61>,<__indexterm-63>,), "term")]
#par(hanging-indent: 1em)[#text(" — ")#_acdc_index_pages((<__indexterm-2>,<__indexterm-4>,<__indexterm-26>,<__indexterm-30>,<__indexterm-32>,<__indexterm-56>,<__indexterm-62>,), "term")]
#par(hanging-indent: 1em)[#text("---")#_acdc_index_pages((<__indexterm-17>,), "term")]
#par(hanging-indent: 1em)[#text("--tail")#_acdc_index_pages((<__indexterm-7>,), "term")]
#par(hanging-indent: 1em)[#text("\\'")#_acdc_index_pages((<__indexterm-49>,), "term")]
#par(hanging-indent: 1em)[#text("--")#_acdc_index_pages((<__indexterm-27>,), "term")]
#par(hanging-indent: 1em)[#text("\\--")#_acdc_index_pages((<__indexterm-16>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("B")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("begin--")#_acdc_index_pages((<__indexterm-12>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("E")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("end")#_acdc_index_pages((<__indexterm-13>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("H")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Hidden")#_acdc_index_pages((<__indexterm-9>,<__indexterm-39>,<__indexterm-50>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("M")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("m")#_acdc_index_pages((<__indexterm-43>,<__indexterm-51>,<__indexterm-52>,<__indexterm-64>,<__indexterm-65>,<__indexterm-66>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("P")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("prefix--")#_acdc_index_pages((<__indexterm-8>,), "term")]
#v(0.75em)
#text(weight: "bold")[#text("S")]
#v(0.25em)
#par(hanging-indent: 1em)[#text("Sam'")#_acdc_index_pages((<__indexterm-44>,), "term")]
]
