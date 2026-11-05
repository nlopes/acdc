use acdc_parser::ast::{
    AttributeValueRef, BlockKind, BlockRef, ContentRef, InlineKind, SectionRef, StructuralRef,
    TableRef,
};
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

pub(super) fn children(block: BlockRef<'_>) -> Vec<BlockRef<'_>> {
    fn append<'doc>(node: StructuralRef<'doc>, blocks: &mut Vec<BlockRef<'doc>>) {
        for child in node.children() {
            if let StructuralRef::Block(block) = child {
                blocks.push(block);
            } else {
                append(child, blocks);
            }
        }
    }
    let mut result = Vec::new();
    append(block.into(), &mut result);
    result
}

pub(super) fn kind(block: BlockRef<'_>) -> &'static str {
    match block.kind() {
        BlockKind::Section(_) => "section",
        BlockKind::Paragraph(_) => "paragraph",
        BlockKind::DiscreteHeader(_) => "discrete_heading",
        BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::CalloutList(_) => "list",
        BlockKind::Admonition(_) => "admonition",
        BlockKind::Image(_) => "image",
        BlockKind::Audio(_) => "audio",
        BlockKind::Video(_) => "video",
        BlockKind::Table(_) => "table",
        BlockKind::Listing(_) => "listing",
        BlockKind::Literal(_) => "literal",
        BlockKind::Stem(_) => "stem",
        BlockKind::Comment(_) | BlockKind::DelimitedComment(_) => "comment",
        BlockKind::Example(_)
        | BlockKind::Open(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Quote(_)
        | BlockKind::Verse(_)
        | BlockKind::Passthrough(_) => "container",
        BlockKind::DocumentAttribute(_) => "attribute",
        BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::TableOfContents(_)
        | _ => "block",
    }
}

pub(super) fn inline_text(nodes: ContentRef<'_>, format: Format) -> String {
    let mut output = String::new();
    for node in nodes.inlines() {
        let text = match node.kind() {
            InlineKind::Plain(t) => {
                if format == Format::Asciidoc && t.is_escaped() {
                    format!("\\{}", t.text())
                } else {
                    t.text().to_owned()
                }
            }
            InlineKind::Raw(t) => {
                if format == Format::Asciidoc {
                    format!("pass:[{}]", t.text())
                } else {
                    t.text().to_owned()
                }
            }
            InlineKind::Verbatim(t) => t.text().to_owned(),
            InlineKind::Bold(t) => wrap(t.content(), "**", format),
            InlineKind::Italic(t) => wrap(t.content(), "__", format),
            InlineKind::Monospace(t) => wrap(t.content(), "``", format),
            InlineKind::Highlight(t) => wrap(t.content(), "##", format),
            InlineKind::Subscript(t) => wrap(t.content(), "~", format),
            InlineKind::Superscript(t) => wrap(t.content(), "^", format),
            InlineKind::CurvedQuotation(t) => format!("“{}”", inline_text(t.content(), format)),
            InlineKind::CurvedApostrophe(t) => format!("‘{}’", inline_text(t.content(), format)),
            InlineKind::Symbol(symbol) => symbol.kind().text().into(),
            InlineKind::CharacterReference(reference) => {
                let spelling = reference.kind().to_string();
                if format == Format::Asciidoc {
                    spelling
                } else {
                    html_escape::decode_html_entities(&spelling).into_owned()
                }
            }
            InlineKind::StandaloneCurvedApostrophe(_) => "’".into(),
            InlineKind::LineBreak(_) => {
                if format == Format::Asciidoc {
                    " +\n".into()
                } else {
                    "\n".into()
                }
            }
            InlineKind::Anchor(a) => {
                if format == Format::Asciidoc {
                    format!("[[{}]]", a.element_id().as_str())
                } else {
                    String::new()
                }
            }
            InlineKind::Callout(c) => format!("<{}>", c.number()),
            other => macro_text(other, format),
        };
        output.push_str(&text);
    }
    output
}

fn wrap(nodes: ContentRef<'_>, marker: &str, format: Format) -> String {
    let text = inline_text(nodes, format);
    if format == Format::Asciidoc {
        format!("{marker}{text}{marker}")
    } else {
        text
    }
}

fn labelled_link(target: &str, label: ContentRef<'_>, format: Format, prefix: &str) -> String {
    let label = inline_text(label, format);
    if format == Format::Asciidoc {
        format!("{prefix}{target}[{}]", label.replace(']', "\\]"))
    } else if label.is_empty() || label == target {
        target.to_owned()
    } else {
        format!("{label} ({target})")
    }
}

fn macro_text(m: InlineKind<'_>, format: Format) -> String {
    match m {
        InlineKind::Link(l) => labelled_link(&l.target().to_string(), l.text(), format, "link:"),
        InlineKind::Url(l) => labelled_link(&l.target().to_string(), l.text(), format, "link:"),
        InlineKind::Mailto(l) => {
            labelled_link(&l.target().to_string(), l.text(), format, "mailto:")
        }
        InlineKind::Autolink(l) => l.target().to_string(),
        InlineKind::CrossReference(l) => labelled_link(l.target(), l.text(), format, "xref:"),
        InlineKind::Footnote(f) => {
            let text = f
                .content()
                .map_or_else(String::new, |body| inline_text(body, format));
            if format == Format::Asciidoc {
                format!("footnote:[{text}]")
            } else {
                format!("[{text}]")
            }
        }
        InlineKind::Pass(p) => {
            if format == Format::Asciidoc {
                format!("pass:[{}]", p.text().unwrap_or_default())
            } else {
                p.text().unwrap_or_default().to_owned()
            }
        }
        InlineKind::Stem(s) => {
            if format == Format::Asciidoc {
                format!("stem:[{}]", s.text())
            } else {
                s.text().to_owned()
            }
        }
        InlineKind::Button(b) => inline_text(b.label(), format),
        InlineKind::Keyboard(k) => k
            .keys()
            .map(|key| inline_text(key, format))
            .collect::<Vec<_>>()
            .join("+"),
        InlineKind::Menu(m) => std::iter::once(m.target())
            .chain(m.items())
            .map(|item| inline_text(item, format))
            .collect::<Vec<_>>()
            .join(" > "),
        InlineKind::Image(i) => {
            let alt = i
                .alt()
                .map_or_else(String::new, |alt| inline_text(alt, Format::Plain));
            if format == Format::Asciidoc {
                format!("image:{}[{alt}]", i.source())
            } else {
                let label = if alt.is_empty() {
                    "Image"
                } else {
                    alt.as_ref()
                };
                format!("{label} ({})", i.source())
            }
        }
        InlineKind::Icon(i) => i.target().to_string(),
        InlineKind::IndexTerm(i) if i.is_visible() => inline_text(i.term(), format),
        InlineKind::IndexTerm(_) => String::new(),
        _ => "[unsupported macro]".into(),
    }
}

fn render_many<'doc>(blocks: impl Iterator<Item = BlockRef<'doc>>, format: Format) -> String {
    blocks
        .map(|b| render_block(b, format, true))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(super) fn render_block(
    block: BlockRef<'_>,
    format: Format,
    include_subsections: bool,
) -> String {
    let title = block
        .title()
        .map(|t| inline_text(t, format))
        .unwrap_or_default();
    let mut body = match block.kind() {
        BlockKind::Section(s) => return render_section(s, &title, format, include_subsections),
        BlockKind::Paragraph(p) => inline_text(p.content(), format),
        BlockKind::DiscreteHeader(h) => {
            if format == Format::Asciidoc {
                format!(
                    "[discrete]\n{} {title}",
                    "=".repeat(usize::from(h.level()) + 1)
                )
            } else {
                title.clone()
            }
        }
        BlockKind::Admonition(a) => {
            let text = render_many(a.blocks(), format);
            if format == Format::Asciidoc {
                format!(
                    "[{}]\n====\n{text}\n====",
                    a.variant().to_string().to_uppercase()
                )
            } else {
                format!("{}: {text}", a.variant().to_string().to_uppercase())
            }
        }
        BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::CalloutList(_) => render_list(block, format),
        BlockKind::Example(_)
        | BlockKind::Open(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Quote(_)
        | BlockKind::Listing(_)
        | BlockKind::Literal(_)
        | BlockKind::Verse(_)
        | BlockKind::Passthrough(_)
        | BlockKind::DelimitedComment(_)
        | BlockKind::Stem(_)
        | BlockKind::Table(_) => render_delimited(block, format),
        BlockKind::Image(i) => {
            let alt = i
                .alt()
                .map_or_else(|| "Image".into(), |alt| inline_text(alt, Format::Plain));
            if format == Format::Asciidoc {
                format!("image::{}[{alt}]", i.source())
            } else {
                format!("{alt} ({})", i.source())
            }
        }
        BlockKind::Audio(a) => {
            if format == Format::Asciidoc {
                format!("audio::{}[]", a.source())
            } else {
                a.source().to_string()
            }
        }
        BlockKind::Video(v) => render_video(v.source().to_string(), format),
        BlockKind::ThematicBreak(_) => "---".into(),
        BlockKind::PageBreak(_) => {
            if format == Format::Asciidoc {
                "<<<".into()
            } else {
                String::new()
            }
        }
        BlockKind::TableOfContents(_) | BlockKind::DocumentAttribute(_) | BlockKind::Comment(_) => {
            String::new()
        }
        _ => "[unsupported block]".into(),
    };
    if format == Format::Asciidoc {
        if !title.is_empty() && !matches!(block.kind(), BlockKind::DiscreteHeader(_)) {
            body = format!(".{title}\n{body}");
        }
        if let Some(id) = block.id() {
            body = format!("[#{}]\n{body}", id.as_str());
        }
    } else if !title.is_empty() && !matches!(block.kind(), BlockKind::DiscreteHeader(_)) {
        body = format!("{title}\n{body}");
    }
    body
}

fn render_section(
    section: SectionRef<'_>,
    title: &str,
    format: Format,
    include_subsections: bool,
) -> String {
    let heading = if format == Format::Asciidoc {
        format!("{} {title}", "=".repeat(usize::from(section.level()) + 1))
    } else {
        title.to_owned()
    };
    let content = section
        .blocks()
        .filter(|b| include_subsections || !matches!(b.kind(), BlockKind::Section(_)))
        .map(|b| render_block(b, format, include_subsections))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.is_empty() {
        heading
    } else {
        format!("{heading}\n\n{content}")
    }
}

fn render_delimited(block: BlockRef<'_>, format: Format) -> String {
    let (body, default_delimiter, authored_delimiter) = match block.kind() {
        BlockKind::Listing(n) => (
            inline_text(n.content(), Format::Plain),
            "----",
            n.delimiter(),
        ),
        BlockKind::Literal(n) => (
            inline_text(n.content(), Format::Plain),
            "----",
            n.delimiter(),
        ),
        BlockKind::Verse(n) => (inline_text(n.content(), format), "____", n.delimiter()),
        BlockKind::Passthrough(n) => (inline_text(n.content(), format), "____", n.delimiter()),
        BlockKind::DelimitedComment(_) => return String::new(),
        BlockKind::Example(n) => (render_many(n.blocks(), format), "====", n.delimiter()),
        BlockKind::Open(n) => (render_many(n.blocks(), format), "--", n.delimiter()),
        BlockKind::Sidebar(n) => (render_many(n.blocks(), format), "****", n.delimiter()),
        BlockKind::Quote(n) => (render_many(n.blocks(), format), "____", n.delimiter()),
        BlockKind::Stem(n) => (n.text().to_owned(), "++++", n.delimiter()),
        BlockKind::Table(n) => return render_table(n, format),
        _ => return "[unsupported delimited block]".into(),
    };
    if format == Format::Plain {
        return body;
    }
    let delimiter = if authored_delimiter.is_empty() {
        default_delimiter
    } else {
        authored_delimiter
    };
    let attributes = block.attributes();
    let value = |name| {
        attributes
            .and_then(|attributes| attributes.get(name))
            .and_then(|value| {
                if let AttributeValueRef::Text(text) = value {
                    Some(text)
                } else {
                    None
                }
            })
            .unwrap_or_default()
    };
    let style = value("style");
    let language = value("language");
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

fn render_video(targets: String, format: Format) -> String {
    if format == Format::Asciidoc {
        format!("video::{targets}[]")
    } else {
        targets
    }
}

fn render_table(table: TableRef<'_>, format: Format) -> String {
    let mut rows = Vec::new();
    for row in table
        .header()
        .into_iter()
        .chain(table.rows())
        .chain(table.footer())
    {
        let mut cells = Vec::new();
        for cell in row.cells() {
            let text = render_many(cell.blocks(), format);
            if format == Format::Plain {
                cells.push(text.replace('\n', " "));
            } else {
                let span = if cell.colspan() > 1 || cell.rowspan() > 1 {
                    format!("{}.{}+", cell.colspan(), cell.rowspan())
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
        .columns()
        .count()
        .max(table.rows().next().map_or(0, |r| r.cells().count()))
        .max(1);
    let options = if table.header().is_some() {
        ",options=header"
    } else {
        ""
    };
    format!("[cols=\"{columns}*\"{options}]\n|===\n{rows}\n|===")
}

fn render_list(block: BlockRef<'_>, format: Format) -> String {
    match block.kind() {
        BlockKind::UnorderedList(l) => l
            .items()
            .map(|i| {
                let text = inline_text(i.principal(), format);
                let nested = render_many(i.blocks(), format);
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
        BlockKind::OrderedList(l) => l
            .items()
            .enumerate()
            .map(|(n, i)| {
                let text = inline_text(i.principal(), format);
                let nested = render_many(i.blocks(), format);
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
        BlockKind::DescriptionList(l) => l
            .items()
            .map(|i| {
                let term = inline_text(i.term(), format);
                let principal = inline_text(i.principal(), format);
                let nested = render_many(i.blocks(), format);
                let delimiter = if format == Format::Asciidoc {
                    "::"
                } else {
                    ":"
                };
                format!("{term}{delimiter} {principal}\n{nested}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        BlockKind::CalloutList(l) => l
            .items()
            .map(|i| {
                format!(
                    "<{}> {}\n{}",
                    i.callout().number(),
                    inline_text(i.principal(), format),
                    render_many(i.blocks(), format)
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
