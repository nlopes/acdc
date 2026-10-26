use acdc_parser::{Block, DelimitedBlockType, InlineMacro, InlineNode, Title};
use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Format {
    #[default]
    Asciidoc,
    Plain,
}

pub(super) fn title<'a>(block: &'a Block<'a>) -> Option<&'a Title<'a>> {
    match block {
        Block::Section(b) => Some(&b.title),
        Block::Paragraph(b) => Some(&b.title),
        Block::DelimitedBlock(b) => Some(&b.title),
        Block::DiscreteHeader(b) => Some(&b.title),
        Block::Admonition(b) => Some(&b.title),
        Block::UnorderedList(b) => Some(&b.title),
        Block::OrderedList(b) => Some(&b.title),
        Block::DescriptionList(b) => Some(&b.title),
        Block::CalloutList(b) => Some(&b.title),
        Block::Image(b) => Some(&b.title),
        Block::Audio(b) => Some(&b.title),
        Block::Video(b) => Some(&b.title),
        Block::PageBreak(b) => Some(&b.title),
        Block::ThematicBreak(b) => Some(&b.title),
        _ => None,
    }
}

pub(super) fn children<'a>(block: &'a Block<'a>) -> Vec<&'a Block<'a>> {
    match block {
        Block::Section(s) => s.content.iter().collect(),
        Block::Admonition(a) => a.blocks.iter().collect(),
        Block::UnorderedList(l) => l.items.iter().flat_map(|i| &i.blocks).collect(),
        Block::OrderedList(l) => l.items.iter().flat_map(|i| &i.blocks).collect(),
        Block::CalloutList(l) => l.items.iter().flat_map(|i| &i.blocks).collect(),
        Block::DescriptionList(l) => l.items.iter().flat_map(|i| &i.description).collect(),
        Block::DelimitedBlock(d) => match &d.inner {
            DelimitedBlockType::DelimitedExample(b)
            | DelimitedBlockType::DelimitedOpen(b)
            | DelimitedBlockType::DelimitedSidebar(b)
            | DelimitedBlockType::DelimitedQuote(b) => b.iter().collect(),
            DelimitedBlockType::DelimitedTable(t) => t
                .header
                .iter()
                .chain(&t.rows)
                .chain(&t.footer)
                .flat_map(|row| &row.columns)
                .flat_map(|cell| &cell.content)
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub(super) fn kind(block: &Block<'_>) -> &'static str {
    match block {
        Block::Section(_) => "section",
        Block::Paragraph(_) => "paragraph",
        Block::DiscreteHeader(_) => "discrete_heading",
        Block::UnorderedList(_)
        | Block::OrderedList(_)
        | Block::DescriptionList(_)
        | Block::CalloutList(_) => "list",
        Block::Admonition(_) => "admonition",
        Block::Image(_) => "image",
        Block::Audio(_) => "audio",
        Block::Video(_) => "video",
        Block::DelimitedBlock(d) => match d.inner {
            DelimitedBlockType::DelimitedTable(_) => "table",
            DelimitedBlockType::DelimitedListing(_) => "listing",
            DelimitedBlockType::DelimitedLiteral(_) => "literal",
            DelimitedBlockType::DelimitedStem(_) => "stem",
            DelimitedBlockType::DelimitedComment(_) => "comment",
            _ => "container",
        },
        Block::DocumentAttribute(_) => "attribute",
        Block::Comment(_) => "comment",
        _ => "block",
    }
}

pub(super) fn inline_text(nodes: &[InlineNode<'_>], format: Format) -> String {
    let mut output = String::new();
    for node in nodes {
        let text = match node {
            InlineNode::PlainText(t) => {
                if format == Format::Asciidoc && t.escaped {
                    format!("\\{}", t.content)
                } else {
                    t.content.to_owned()
                }
            }
            InlineNode::RawText(t) => {
                if format == Format::Asciidoc {
                    format!("pass:[{}]", t.content)
                } else {
                    t.content.to_owned()
                }
            }
            InlineNode::VerbatimText(t) => t.content.to_owned(),
            InlineNode::BoldText(t) => wrap(&t.content, "**", format),
            InlineNode::ItalicText(t) => wrap(&t.content, "__", format),
            InlineNode::MonospaceText(t) => wrap(&t.content, "``", format),
            InlineNode::HighlightText(t) => wrap(&t.content, "##", format),
            InlineNode::SubscriptText(t) => wrap(&t.content, "~", format),
            InlineNode::SuperscriptText(t) => wrap(&t.content, "^", format),
            InlineNode::CurvedQuotationText(t) => format!("“{}”", inline_text(&t.content, format)),
            InlineNode::CurvedApostropheText(t) => format!("‘{}’", inline_text(&t.content, format)),
            InlineNode::StandaloneCurvedApostrophe(_) => "’".into(),
            InlineNode::LineBreak(_) => {
                if format == Format::Asciidoc {
                    " +\n".into()
                } else {
                    "\n".into()
                }
            }
            InlineNode::InlineAnchor(a) => {
                if format == Format::Asciidoc {
                    format!("[[{}]]", a.id)
                } else {
                    String::new()
                }
            }
            InlineNode::CalloutRef(c) => format!("<{}>", c.number),
            InlineNode::Macro(m) => macro_text(m, format),
            _ => "[unsupported inline]".into(),
        };
        output.push_str(&text);
    }
    output
}

fn wrap(nodes: &[InlineNode<'_>], marker: &str, format: Format) -> String {
    let text = inline_text(nodes, format);
    if format == Format::Asciidoc {
        format!("{marker}{text}{marker}")
    } else {
        text
    }
}

fn labelled_link(target: &str, label: &[InlineNode<'_>], format: Format, prefix: &str) -> String {
    let label = inline_text(label, format);
    if format == Format::Asciidoc {
        format!("{prefix}{target}[{}]", label.replace(']', "\\]"))
    } else if label.is_empty() || label == target {
        target.to_owned()
    } else {
        format!("{label} ({target})")
    }
}

fn macro_text(m: &InlineMacro<'_>, format: Format) -> String {
    match m {
        InlineMacro::Link(l) => labelled_link(&l.target.to_string(), &l.text, format, "link:"),
        InlineMacro::Url(l) => labelled_link(&l.target.to_string(), &l.text, format, "link:"),
        InlineMacro::Mailto(l) => labelled_link(&l.target.to_string(), &l.text, format, "mailto:"),
        InlineMacro::Autolink(l) => l.url.to_string(),
        InlineMacro::CrossReference(l) => labelled_link(l.target, &l.text, format, "xref:"),
        InlineMacro::Footnote(f) => {
            let text = inline_text(&f.content, format);
            if format == Format::Asciidoc {
                format!("footnote:[{text}]")
            } else {
                format!("[{text}]")
            }
        }
        InlineMacro::Pass(p) => {
            if format == Format::Asciidoc {
                format!("pass:[{}]", p.text.unwrap_or_default())
            } else {
                p.text.unwrap_or_default().to_owned()
            }
        }
        InlineMacro::Stem(s) => {
            if format == Format::Asciidoc {
                format!("stem:[{}]", s.content)
            } else {
                s.content.to_owned()
            }
        }
        InlineMacro::Button(b) => inline_text(&b.label, format),
        InlineMacro::Keyboard(k) => k
            .keys
            .iter()
            .map(|key| inline_text(key, format))
            .collect::<Vec<_>>()
            .join("+"),
        InlineMacro::Menu(m) => std::iter::once(&m.target)
            .chain(&m.items)
            .map(|item| inline_text(item, format))
            .collect::<Vec<_>>()
            .join(" > "),
        InlineMacro::Image(i) => {
            let alt = i
                .alt
                .as_ref()
                .map_or_else(String::new, |alt| inline_text(alt, Format::Plain));
            if format == Format::Asciidoc {
                format!("image:{}[{alt}]", i.source)
            } else {
                let label = if alt.is_empty() {
                    "Image"
                } else {
                    alt.as_ref()
                };
                format!("{label} ({})", i.source)
            }
        }
        InlineMacro::Icon(i) => i.target.to_string(),
        InlineMacro::IndexTerm(i) => match &i.kind {
            acdc_parser::IndexTermKind::Flow(term) => inline_text(term, format),
            _ => String::new(),
        },
        _ => "[unsupported macro]".into(),
    }
}

fn render_many(blocks: &[Block<'_>], format: Format) -> String {
    blocks
        .iter()
        .map(|b| render_block(b, format, true))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(super) fn render_block(block: &Block<'_>, format: Format, include_subsections: bool) -> String {
    let title = title(block)
        .map(|t| inline_text(t, format))
        .unwrap_or_default();
    let mut body = match block {
        Block::Section(s) => {
            let heading = if format == Format::Asciidoc {
                format!("{} {title}", "=".repeat(usize::from(s.level) + 1))
            } else {
                title.clone()
            };
            let content = s
                .content
                .iter()
                .filter(|b| include_subsections || !matches!(b, Block::Section(_)))
                .map(|b| render_block(b, format, include_subsections))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            return if content.is_empty() {
                heading
            } else {
                format!("{heading}\n\n{content}")
            };
        }
        Block::Paragraph(p) => inline_text(&p.content, format),
        Block::DiscreteHeader(h) => {
            if format == Format::Asciidoc {
                format!(
                    "[discrete]\n{} {title}",
                    "=".repeat(usize::from(h.level) + 1)
                )
            } else {
                title.clone()
            }
        }
        Block::Admonition(a) => {
            let text = render_many(&a.blocks, format);
            if format == Format::Asciidoc {
                format!(
                    "[{}]\n====\n{text}\n====",
                    a.variant.to_string().to_uppercase()
                )
            } else {
                format!("{}: {text}", a.variant.to_string().to_uppercase())
            }
        }
        Block::UnorderedList(_)
        | Block::OrderedList(_)
        | Block::DescriptionList(_)
        | Block::CalloutList(_) => render_list(block, format),
        Block::DelimitedBlock(d) => render_delimited(d, format),
        Block::Image(i) => {
            let alt = i
                .alt
                .as_ref()
                .map_or_else(|| "Image".into(), |alt| inline_text(alt, Format::Plain));
            if format == Format::Asciidoc {
                format!("image::{}[{alt}]", i.source)
            } else {
                format!("{alt} ({})", i.source)
            }
        }
        Block::Audio(a) => {
            if format == Format::Asciidoc {
                format!("audio::{}[]", a.source)
            } else {
                a.source.to_string()
            }
        }
        Block::Video(v) => render_video(v, format),
        Block::ThematicBreak(_) => "---".into(),
        Block::PageBreak(_) => {
            if format == Format::Asciidoc {
                "<<<".into()
            } else {
                String::new()
            }
        }
        Block::TableOfContents(_) | Block::DocumentAttribute(_) | Block::Comment(_) => {
            String::new()
        }
        _ => "[unsupported block]".into(),
    };
    if format == Format::Asciidoc {
        if !title.is_empty() && !matches!(block, Block::DiscreteHeader(_)) {
            body = format!(".{title}\n{body}");
        }
        if let Some(metadata) = block.metadata()
            && let Some(anchor) = &metadata.id
        {
            body = format!("[#{}]\n{body}", anchor.id);
        }
    } else if !title.is_empty() && !matches!(block, Block::DiscreteHeader(_)) {
        body = format!("{title}\n{body}");
    }
    body
}

fn render_delimited(d: &acdc_parser::DelimitedBlock<'_>, format: Format) -> String {
    let (body, default_delimiter) = match &d.inner {
        DelimitedBlockType::DelimitedListing(n) | DelimitedBlockType::DelimitedLiteral(n) => {
            (inline_text(n, Format::Plain), "----")
        }
        DelimitedBlockType::DelimitedVerse(n) | DelimitedBlockType::DelimitedPass(n) => {
            (inline_text(n, format), "____")
        }
        DelimitedBlockType::DelimitedComment(_) => return String::new(),
        DelimitedBlockType::DelimitedExample(b) => (render_many(b, format), "===="),
        DelimitedBlockType::DelimitedOpen(b) => (render_many(b, format), "--"),
        DelimitedBlockType::DelimitedSidebar(b) => (render_many(b, format), "****"),
        DelimitedBlockType::DelimitedQuote(b) => (render_many(b, format), "____"),
        DelimitedBlockType::DelimitedStem(s) => (s.content.to_owned(), "++++"),
        DelimitedBlockType::DelimitedTable(t) => return render_table(t, format),
        _ => return "[unsupported delimited block]".into(),
    };
    if format == Format::Plain {
        return body;
    }
    let delimiter = if d.delimiter.is_empty() {
        default_delimiter
    } else {
        d.delimiter
    };
    let style = d.metadata.style.unwrap_or_default();
    let language = d
        .metadata
        .attributes
        .get_string("language")
        .unwrap_or_default();
    let attributes = if style.is_empty() {
        String::new()
    } else if language.is_empty() {
        format!("[{style}]\n")
    } else {
        format!("[{style},{language}]\n")
    };
    format!(
        "{attributes}{delimiter}\n{}{delimiter}",
        if body.ends_with('\n') {
            body
        } else {
            format!("{body}\n")
        }
    )
}

fn render_video(video: &acdc_parser::Video<'_>, format: Format) -> String {
    let targets = video.source.to_string();
    if format == Format::Asciidoc {
        format!("video::{targets}[]")
    } else {
        targets
    }
}

fn render_table(table: &acdc_parser::Table<'_>, format: Format) -> String {
    let mut rows = Vec::new();
    for row in table.header.iter().chain(&table.rows).chain(&table.footer) {
        let mut cells = Vec::new();
        for cell in &row.columns {
            let text = render_many(&cell.content, format);
            if format == Format::Plain {
                cells.push(text.replace('\n', " "));
            } else {
                let span = if cell.colspan > 1 || cell.rowspan > 1 {
                    format!("{}.{}+", cell.colspan, cell.rowspan)
                } else {
                    String::new()
                };
                cells.push(format!("{span}a|{}", text.replace('|', "\\|")));
            }
        }
        rows.push(cells.join(if format == Format::Plain {
            " | "
        } else {
            "\n\n"
        }));
    }
    let rows = rows.join("\n\n");
    if format == Format::Plain {
        return rows;
    }
    let columns = table
        .columns
        .len()
        .max(table.rows.first().map_or(0, |r| r.columns.len()))
        .max(1);
    let options = if table.header.is_some() {
        ",options=header"
    } else {
        ""
    };
    format!("[cols=\"{columns}*\"{options}]\n|===\n{rows}\n|===")
}

fn render_list(block: &Block<'_>, format: Format) -> String {
    match block {
        Block::UnorderedList(l) => l
            .items
            .iter()
            .map(|i| {
                let text = inline_text(&i.principal, format);
                let nested = render_many(&i.blocks, format);
                if nested.is_empty() {
                    format!("* {text}")
                } else if format == Format::Asciidoc {
                    format!("* {text}\n+\n{nested}")
                } else {
                    format!("* {text}\n{nested}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::OrderedList(l) => l
            .items
            .iter()
            .enumerate()
            .map(|(n, i)| {
                let text = inline_text(&i.principal, format);
                let nested = render_many(&i.blocks, format);
                let marker = if format == Format::Asciidoc {
                    ".".into()
                } else {
                    format!("{}.", n + 1)
                };
                if nested.is_empty() {
                    format!("{marker} {text}")
                } else if format == Format::Asciidoc {
                    format!("{marker} {text}\n+\n{nested}")
                } else {
                    format!("{marker} {text}\n{nested}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::DescriptionList(l) => l
            .items
            .iter()
            .map(|i| {
                let term = inline_text(&i.term, format);
                let principal = inline_text(&i.principal_text, format);
                let nested = render_many(&i.description, format);
                let delimiter = if format == Format::Asciidoc {
                    "::"
                } else {
                    ":"
                };
                format!("{term}{delimiter} {principal}\n{nested}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::CalloutList(l) => l
            .items
            .iter()
            .map(|i| {
                format!(
                    "<{}> {}\n{}",
                    i.callout.number,
                    inline_text(&i.principal, format),
                    render_many(&i.blocks, format)
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
