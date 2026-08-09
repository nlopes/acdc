#set document(
  title: "Source block pagination",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Source block pagination]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#show raw.where(block: true): it => block(width: 100%, fill: rgb("#1e1e1e"), radius: 4pt, inset: 10pt, text(fill: rgb("#d4d4d4"), it))
#let captiontext(body) = {
  show strong: set text(fill: rgb("#333333"), weight: 700, style: "normal")
  text(size: 0.91em, weight: 400, style: "italic", fill: rgb("#333333"), body)
}
#let blocktitle(body) = {
  block(width: 100%, above: 19.15pt, below: 0pt, align(left, captiontext(body)))
  block(height: 8pt, above: 0pt, below: 0pt)
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
#let docimage(path) = block(radius: 4pt, clip: true, image(path, width: 100%))
#set list(marker: (box(baseline: -0.2em, circle(radius: 0.14em, fill: rgb("#6b7280"))), box(baseline: -0.2em, circle(radius: 0.13em, stroke: 0.6pt + rgb("#6b7280"))), box(baseline: -0.2em, rect(width: 0.24em, height: 0.24em, fill: rgb("#6b7280")))))
#set enum(numbering: (..n) => text(fill: rgb("#9ca3af"))[#numbering("1.", ..n.pos())])
#set table(stroke: (_, y) => (bottom: 0.75pt + rgb("#e5e7eb")), inset: (x: 0.6em, y: 0.45em))
#let tableheader(body) = text(weight: 700, body)

#align(center)[
#text(size: 22pt, weight: "bold")[#text("Source block pagination")]
]
#v(1em)

#{
  let numbers = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, )
  let highlighted = (true, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, true, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, true, )
  let gutter = 1.8em
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
  raw(block: true, lang: "rust", "let line_001 = 1;\nlet line_002 = 2;\nlet line_003 = 3;\nlet line_004 = 4;\nlet line_005 = 5;\nlet line_006 = 6;\nlet line_007 = 7;\nlet line_008 = 8;\nlet line_009 = 9;\nlet line_010 = 10;\nlet line_011 = 11;\nlet line_012 = 12;\nlet line_013 = 13;\nlet line_014 = 14;\nlet line_015 = 15;\nlet line_016 = 16;\nlet line_017 = 17;\nlet line_018 = 18;\nlet line_019 = 19;\nlet line_020 = 20;\nlet line_021 = 21;\nlet line_022 = 22;\nlet line_023 = 23;\nlet line_024 = 24;\nlet line_025 = 25;\nlet line_026 = 26;\nlet line_027 = 27;\nlet line_028 = 28;\nlet line_029 = 29;\nlet line_030 = 30;\nlet line_031 = 31;\nlet line_032 = 32;\nlet line_033 = 33;\nlet line_034 = 34;\nlet line_035 = 35;\nlet line_036 = 36;\nlet line_037 = 37;\nlet line_038 = 38;\nlet line_039 = 39;\nlet line_040 = 40;\nlet line_041 = 41;\nlet line_042 = 42;\nlet line_043 = 43;\nlet line_044 = 44;\nlet line_045 = 45;\nlet line_046 = 46;\nlet line_047 = 47;\nlet line_048 = 48;\nlet line_049 = 49;\nlet line_050 = 50;\nlet line_051 = 51;\nlet line_052 = 52;\nlet line_053 = 53;\nlet line_054 = 54;\nlet line_055 = 55;\nlet line_056 = 56;\nlet line_057 = 57;\nlet line_058 = 58;\nlet line_059 = 59;\nlet line_060 = 60;\nlet line_061 = 61;\nlet line_062 = 62;\nlet line_063 = 63;\nlet line_064 = 64;\nlet line_065 = 65;\nlet line_066 = 66;\nlet line_067 = 67;\nlet line_068 = 68;\nlet line_069 = 69;\nlet line_070 = 70;\nlet line_071 = 71;\nlet line_072 = 72;\nlet line_073 = 73;\nlet line_074 = 74;\nlet line_075 = 75;\nlet line_076 = 76;\nlet line_077 = 77;\nlet line_078 = 78;\nlet line_079 = 79;\nlet line_080 = 80;\nlet line_081 = 81;\nlet line_082 = 82;\nlet line_083 = 83;\nlet line_084 = 84;\nlet line_085 = 85;\nlet line_086 = 86;\nlet line_087 = 87;\nlet line_088 = 88;\nlet line_089 = 89;\nlet line_090 = 90;\nlet line_091 = 91;\nlet line_092 = 92;\nlet line_093 = 93;\nlet line_094 = 94;\nlet line_095 = 95;\nlet line_096 = 96;\nlet line_097 = 97;\nlet line_098 = 98;\nlet line_099 = 99;\nlet line_100 = 100;")
}
