#set document(
  title: "All Book Special Sections Numbered",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[All Book Special Sections Numbered]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
#set text(font: ("IBM Plex Serif", "Noto Color Emoji"), size: 11pt, weight: 400, fill: rgb("#111111"), tracking: 0em, lang: "en")
#set par(leading: 0.65em, justify: false)
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
#show raw.where(block: true): it => block(width: 100%, fill: rgb("#1e1e1e"), radius: 4pt, inset: 10pt, text(fill: rgb("#d4d4d4"), it))
#let blockquote(body) = block(width: 100%, inset: (left: 12pt), stroke: (left: 3pt + rgb("#d1d5db")), text(style: "italic", fill: rgb("#4b5563"), body))
#let examplebox(body) = block(width: 100%, fill: rgb("#f3f4f6"), radius: 4pt, inset: (x: 12pt, y: 10pt), body)
#let sidebarbox(body) = block(width: 100%, fill: rgb("#f3f4f6"), stroke: 0.75pt + rgb("#e5e7eb"), radius: 4pt, inset: (x: 12pt, y: 10pt), body)
#let sidebartitle(body) = align(center, text(weight: "bold", body))
#let verse(body) = block(inset: (left: 12pt), text(fill: rgb("#4b5563"), body))
#let attribution(body) = block(inset: (left: 12pt), above: 0.6em, text(size: 0.9em, fill: rgb("#4b5563"))[— #body])
#let _cbadge(body) = box(circle(radius: 0.6em, fill: rgb("#111111"), inset: 0pt, align(center + horizon, body)))
#let _cico(glyph) = _cbadge(text(fill: white, weight: 700, size: 0.82em)[#glyph])
#let _ccheck = _cbadge(box(width: 0.62em, height: 0.62em, place(curve(stroke: (paint: white, thickness: 1.5pt, cap: "round", join: "round"), curve.move((0em, 0.34em)), curve.line((0.21em, 0.55em)), curve.line((0.58em, 0.08em))))))
#let _cicon(kind) = ("note": _cico("i"), "tip": _cico("i"), "important": _cico("!"), "warning": _cico("!"), "caution": _cico("!"), "success": _ccheck).at(kind, default: _cico("i"))
#let callout(kind, body) = pad(left: 0pt, block(width: 100%, fill: rgb("#f3f4f6"), radius: 4pt, inset: (x: 12pt, y: 10pt), grid(columns: (auto, 1fr), column-gutter: 9.600000000000001pt, align: top, _cicon(kind), body)))
#let checkbox(checked) = box(height: 0.85em, width: 0.85em, baseline: 0.15em, radius: 2pt, stroke: 0.75pt + rgb("#9ca3af"), fill: if checked { rgb("#374151") } else { white })
#let hr() = block(above: 1.2em, below: 1.2em, line(length: 100%, stroke: 0.75pt + rgb("#e5e7eb")))
#let docimage(path) = block(radius: 4pt, clip: true, image(path, width: 100%))
#set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))))
#set enum(numbering: (..n) => text(fill: rgb("#9ca3af"))[#numbering("1.", ..n.pos())])
#set table(stroke: (_, y) => (bottom: 0.75pt + rgb("#e5e7eb")), inset: (x: 0.6em, y: 0.45em))
#let tableheader(body) = text(weight: 700, body)

#page(header: none, footer: none)[
#v(30%)
#align(center)[
#text(size: 22pt, weight: "bold")[#text("All Book Special Sections Numbered")]
]
#counter(page).update(0)
]

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
        str(counter(page).at(target).first()),
      ),
    ),
  )
}
#_acdc_toc_entry(<id-5f70726566616365>, 0, [#text("1. ")#text("Preface")])
#_acdc_toc_entry(<id-5f707265666163655f746f706963>, 1, [#text("1.1. ")#text("Preface Topic")])
#_acdc_toc_entry(<id-5f6162737472616374>, 0, [#text("2. ")#text("Abstract")])
#_acdc_toc_entry(<id-5f61627374726163745f746f706963>, 1, [#text("2.1. ")#text("Abstract Topic")])
#_acdc_toc_entry(<id-5f64656469636174696f6e>, 0, [#text("3. ")#text("Dedication")])
#_acdc_toc_entry(<id-5f636f6c6f70686f6e>, 0, [#text("4. ")#text("Colophon")])
#_acdc_toc_entry(<id-5f676c6f7373617279>, 0, [#text("5. ")#text("Glossary")])
#_acdc_toc_entry(<id-5f6269626c696f677261706879>, 0, [#text("6. ")#text("Bibliography")])
#_acdc_toc_entry(<id-5f617070656e6469785f6d6174657269616c>, 0, [#text("Appendix A: ")#text("Appendix Material")])
#_acdc_toc_entry(<id-5f617070656e6469785f746f706963>, 1, [#text("A.1. ")#text("Appendix Topic")])
#_acdc_toc_entry(<id-5f615f7265616c5f70617274>, 0, [#text("I: ")#text("A Real Part")])
#_acdc_toc_entry(<id-5f615f63686170746572>, 1, [#text("7. ")#text("A Chapter")])
#_acdc_toc_entry(<id-5f696e646578>, 0, [#text("8. ")#text("Index")])
#pagebreak()

#pagebreak(weak: true)

#heading(level: 1)[#text("Chapter 1. ")#text("Preface")] <id-5f70726566616365>

#heading(level: 2)[#text("1.1. ")#text("Preface Topic")] <id-5f707265666163655f746f706963>

#pagebreak(weak: true)

#heading(level: 1)[#text("Chapter 2. ")#text("Abstract")] <id-5f6162737472616374>

#heading(level: 2)[#text("2.1. ")#text("Abstract Topic")] <id-5f61627374726163745f746f706963>

#pagebreak(weak: true)

#heading(level: 1)[#text("Chapter 3. ")#text("Dedication")] <id-5f64656469636174696f6e>

#pagebreak(weak: true)

#heading(level: 1)[#text("Chapter 4. ")#text("Colophon")] <id-5f636f6c6f70686f6e>

#pagebreak(weak: true)

#heading(level: 1)[#text("Chapter 5. ")#text("Glossary")] <id-5f676c6f7373617279>

#pagebreak(weak: true)

#heading(level: 1)[#text("Chapter 6. ")#text("Bibliography")] <id-5f6269626c696f677261706879>

#pagebreak(weak: true)

#heading(level: 1)[#text("Appendix A: ")#text("Appendix Material")] <id-5f617070656e6469785f6d6174657269616c>

#heading(level: 2)[#text("A.1. ")#text("Appendix Topic")] <id-5f617070656e6469785f746f706963>

#pagebreak(weak: true)

#heading(level: 1)[#text("Part I: ")#text("A Real Part")] <id-5f615f7265616c5f70617274>

#pagebreak(weak: true)

#heading(level: 1)[#text("Chapter 7. ")#text("A Chapter")] <id-5f615f63686170746572>

#text("A concealed index term ")#text(".")

#pagebreak(weak: true)

#heading(level: 1)[#text("Chapter 8. ")#text("Index")] <id-5f696e646578>

