#set document(
  title: "Table footer pagination",
)
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm), fill: rgb("#ffffff"), header: context if counter(page).get().first() > 1 { align(left + horizon)[#text(fill: rgb("#374151"), weight: 500, size: 11pt)[Table footer pagination]] }, footer: text(fill: rgb("#9ca3af"), size: 9pt)[#grid(columns: (1fr, 1fr, 1fr), align(left)[], align(center)[#context counter(page).display()], align(right)[])])
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
#text(size: 22pt, weight: "bold")[#text("Table footer pagination")]
]
#v(1em)

#blocktitle[#text("Long footer")]
#table(columns: (1fr, 1fr), align: (left + top, left + top), table.header(repeat: true, table.cell(x: 0, y: 0)[#tableheader[#text("Item")

]], table.cell(x: 1, y: 0)[#tableheader[#text("Value")

]]), table.cell(x: 0, y: 1)[#text("Row 001")

], table.cell(x: 1, y: 1)[#text("1")

], table.cell(x: 0, y: 2)[#text("Row 002")

], table.cell(x: 1, y: 2)[#text("2")

], table.cell(x: 0, y: 3)[#text("Row 003")

], table.cell(x: 1, y: 3)[#text("3")

], table.cell(x: 0, y: 4)[#text("Row 004")

], table.cell(x: 1, y: 4)[#text("4")

], table.cell(x: 0, y: 5)[#text("Row 005")

], table.cell(x: 1, y: 5)[#text("5")

], table.cell(x: 0, y: 6)[#text("Row 006")

], table.cell(x: 1, y: 6)[#text("6")

], table.cell(x: 0, y: 7)[#text("Row 007")

], table.cell(x: 1, y: 7)[#text("7")

], table.cell(x: 0, y: 8)[#text("Row 008")

], table.cell(x: 1, y: 8)[#text("8")

], table.cell(x: 0, y: 9)[#text("Row 009")

], table.cell(x: 1, y: 9)[#text("9")

], table.cell(x: 0, y: 10)[#text("Row 010")

], table.cell(x: 1, y: 10)[#text("10")

], table.cell(x: 0, y: 11)[#text("Row 011")

], table.cell(x: 1, y: 11)[#text("11")

], table.cell(x: 0, y: 12)[#text("Row 012")

], table.cell(x: 1, y: 12)[#text("12")

], table.cell(x: 0, y: 13)[#text("Row 013")

], table.cell(x: 1, y: 13)[#text("13")

], table.cell(x: 0, y: 14)[#text("Row 014")

], table.cell(x: 1, y: 14)[#text("14")

], table.cell(x: 0, y: 15)[#text("Row 015")

], table.cell(x: 1, y: 15)[#text("15")

], table.cell(x: 0, y: 16)[#text("Row 016")

], table.cell(x: 1, y: 16)[#text("16")

], table.cell(x: 0, y: 17)[#text("Row 017")

], table.cell(x: 1, y: 17)[#text("17")

], table.cell(x: 0, y: 18)[#text("Row 018")

], table.cell(x: 1, y: 18)[#text("18")

], table.cell(x: 0, y: 19)[#text("Row 019")

], table.cell(x: 1, y: 19)[#text("19")

], table.cell(x: 0, y: 20)[#text("Row 020")

], table.cell(x: 1, y: 20)[#text("20")

], table.cell(x: 0, y: 21)[#text("Row 021")

], table.cell(x: 1, y: 21)[#text("21")

], table.cell(x: 0, y: 22)[#text("Row 022")

], table.cell(x: 1, y: 22)[#text("22")

], table.cell(x: 0, y: 23)[#text("Row 023")

], table.cell(x: 1, y: 23)[#text("23")

], table.cell(x: 0, y: 24)[#text("Row 024")

], table.cell(x: 1, y: 24)[#text("24")

], table.cell(x: 0, y: 25)[#text("Row 025")

], table.cell(x: 1, y: 25)[#text("25")

], table.cell(x: 0, y: 26)[#text("Row 026")

], table.cell(x: 1, y: 26)[#text("26")

], table.cell(x: 0, y: 27)[#text("Row 027")

], table.cell(x: 1, y: 27)[#text("27")

], table.cell(x: 0, y: 28)[#text("Row 028")

], table.cell(x: 1, y: 28)[#text("28")

], table.cell(x: 0, y: 29)[#text("Row 029")

], table.cell(x: 1, y: 29)[#text("29")

], table.cell(x: 0, y: 30)[#text("Row 030")

], table.cell(x: 1, y: 30)[#text("30")

], table.cell(x: 0, y: 31)[#text("Row 031")

], table.cell(x: 1, y: 31)[#text("31")

], table.cell(x: 0, y: 32)[#text("Row 032")

], table.cell(x: 1, y: 32)[#text("32")

], table.cell(x: 0, y: 33)[#text("Row 033")

], table.cell(x: 1, y: 33)[#text("33")

], table.cell(x: 0, y: 34)[#text("Row 034")

], table.cell(x: 1, y: 34)[#text("34")

], table.cell(x: 0, y: 35)[#text("Row 035")

], table.cell(x: 1, y: 35)[#text("35")

], table.cell(x: 0, y: 36)[#text("Row 036")

], table.cell(x: 1, y: 36)[#text("36")

], table.cell(x: 0, y: 37)[#text("Row 037")

], table.cell(x: 1, y: 37)[#text("37")

], table.cell(x: 0, y: 38)[#text("Row 038")

], table.cell(x: 1, y: 38)[#text("38")

], table.cell(x: 0, y: 39)[#text("Row 039")

], table.cell(x: 1, y: 39)[#text("39")

], table.cell(x: 0, y: 40)[#text("Row 040")

], table.cell(x: 1, y: 40)[#text("40")

], table.cell(x: 0, y: 41)[#text("Row 041")

], table.cell(x: 1, y: 41)[#text("41")

], table.cell(x: 0, y: 42)[#text("Row 042")

], table.cell(x: 1, y: 42)[#text("42")

], table.cell(x: 0, y: 43)[#text("Row 043")

], table.cell(x: 1, y: 43)[#text("43")

], table.cell(x: 0, y: 44)[#text("Row 044")

], table.cell(x: 1, y: 44)[#text("44")

], table.cell(x: 0, y: 45)[#text("Row 045")

], table.cell(x: 1, y: 45)[#text("45")

], table.cell(x: 0, y: 46)[#text("Row 046")

], table.cell(x: 1, y: 46)[#text("46")

], table.cell(x: 0, y: 47)[#text("Row 047")

], table.cell(x: 1, y: 47)[#text("47")

], table.cell(x: 0, y: 48)[#text("Row 048")

], table.cell(x: 1, y: 48)[#text("48")

], table.cell(x: 0, y: 49)[#text("Row 049")

], table.cell(x: 1, y: 49)[#text("49")

], table.cell(x: 0, y: 50)[#text("Row 050")

], table.cell(x: 1, y: 50)[#text("50")

], table.cell(x: 0, y: 51)[#text("Row 051")

], table.cell(x: 1, y: 51)[#text("51")

], table.cell(x: 0, y: 52)[#text("Row 052")

], table.cell(x: 1, y: 52)[#text("52")

], table.cell(x: 0, y: 53)[#text("Row 053")

], table.cell(x: 1, y: 53)[#text("53")

], table.cell(x: 0, y: 54)[#text("Row 054")

], table.cell(x: 1, y: 54)[#text("54")

], table.cell(x: 0, y: 55)[#text("Row 055")

], table.cell(x: 1, y: 55)[#text("55")

], table.cell(x: 0, y: 56)[#text("Row 056")

], table.cell(x: 1, y: 56)[#text("56")

], table.cell(x: 0, y: 57)[#text("Row 057")

], table.cell(x: 1, y: 57)[#text("57")

], table.cell(x: 0, y: 58)[#text("Row 058")

], table.cell(x: 1, y: 58)[#text("58")

], table.cell(x: 0, y: 59)[#text("Row 059")

], table.cell(x: 1, y: 59)[#text("59")

], table.cell(x: 0, y: 60)[#text("Row 060")

], table.cell(x: 1, y: 60)[#text("60")

], table.cell(x: 0, y: 61)[#text("Row 061")

], table.cell(x: 1, y: 61)[#text("61")

], table.cell(x: 0, y: 62)[#text("Row 062")

], table.cell(x: 1, y: 62)[#text("62")

], table.cell(x: 0, y: 63)[#text("Row 063")

], table.cell(x: 1, y: 63)[#text("63")

], table.cell(x: 0, y: 64)[#text("Row 064")

], table.cell(x: 1, y: 64)[#text("64")

], table.cell(x: 0, y: 65)[#text("Row 065")

], table.cell(x: 1, y: 65)[#text("65")

], table.cell(x: 0, y: 66)[#text("Row 066")

], table.cell(x: 1, y: 66)[#text("66")

], table.cell(x: 0, y: 67)[#text("Row 067")

], table.cell(x: 1, y: 67)[#text("67")

], table.cell(x: 0, y: 68)[#text("Row 068")

], table.cell(x: 1, y: 68)[#text("68")

], table.cell(x: 0, y: 69)[#text("Row 069")

], table.cell(x: 1, y: 69)[#text("69")

], table.cell(x: 0, y: 70)[#text("Row 070")

], table.cell(x: 1, y: 70)[#text("70")

], table.cell(x: 0, y: 71)[#text("Row 071")

], table.cell(x: 1, y: 71)[#text("71")

], table.cell(x: 0, y: 72)[#text("Row 072")

], table.cell(x: 1, y: 72)[#text("72")

], table.cell(x: 0, y: 73)[#text("Row 073")

], table.cell(x: 1, y: 73)[#text("73")

], table.cell(x: 0, y: 74)[#text("Row 074")

], table.cell(x: 1, y: 74)[#text("74")

], table.cell(x: 0, y: 75)[#text("Row 075")

], table.cell(x: 1, y: 75)[#text("75")

], table.cell(x: 0, y: 76)[#text("Row 076")

], table.cell(x: 1, y: 76)[#text("76")

], table.cell(x: 0, y: 77)[#text("Row 077")

], table.cell(x: 1, y: 77)[#text("77")

], table.cell(x: 0, y: 78)[#text("Row 078")

], table.cell(x: 1, y: 78)[#text("78")

], table.cell(x: 0, y: 79)[#text("Row 079")

], table.cell(x: 1, y: 79)[#text("79")

], table.cell(x: 0, y: 80)[#text("Row 080")

], table.cell(x: 1, y: 80)[#text("80")

], table.cell(x: 0, y: 81)[#text("Row 081")

], table.cell(x: 1, y: 81)[#text("81")

], table.cell(x: 0, y: 82)[#text("Row 082")

], table.cell(x: 1, y: 82)[#text("82")

], table.cell(x: 0, y: 83)[#text("Row 083")

], table.cell(x: 1, y: 83)[#text("83")

], table.cell(x: 0, y: 84)[#text("Row 084")

], table.cell(x: 1, y: 84)[#text("84")

], table.cell(x: 0, y: 85)[#text("Row 085")

], table.cell(x: 1, y: 85)[#text("85")

], table.cell(x: 0, y: 86)[#text("Row 086")

], table.cell(x: 1, y: 86)[#text("86")

], table.cell(x: 0, y: 87)[#text("Row 087")

], table.cell(x: 1, y: 87)[#text("87")

], table.cell(x: 0, y: 88)[#text("Row 088")

], table.cell(x: 1, y: 88)[#text("88")

], table.cell(x: 0, y: 89)[#text("Row 089")

], table.cell(x: 1, y: 89)[#text("89")

], table.cell(x: 0, y: 90)[#text("Row 090")

], table.cell(x: 1, y: 90)[#text("90")

], table.footer(repeat: false, table.cell(x: 0, y: 91)[#text("Grand total")

], table.cell(x: 1, y: 91)[#text("90")

]))
