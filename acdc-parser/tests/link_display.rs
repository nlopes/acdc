use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn hide_uri_scheme_is_resolved_at_each_links_source_position() -> Result<(), Error> {
    let parsed = parse(
        "https://before.example[]\n\n:hide-uri-scheme:\n\nhttps://during.example\n\n:hide-uri-scheme: false\n\nlink:https://false.example[]\n\n:hide-uri-scheme!:\n\nhttps://after.example[]\n\n.Local\n:hide-uri-scheme:\nlink:https://local.example[]\n",
        &Options::default(),
    )?;

    let actual = parsed
        .document()
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|inline| {
                let InlineNode::Macro(inline) = inline else {
                    return None;
                };
                match inline {
                    InlineMacro::Url(url) => Some(url.hides_uri_scheme()),
                    InlineMacro::Link(link) => Some(link.hides_uri_scheme()),
                    InlineMacro::Autolink(autolink) => Some(autolink.hides_uri_scheme()),
                    InlineMacro::Footnote(_)
                    | InlineMacro::Icon(_)
                    | InlineMacro::Image(_)
                    | InlineMacro::Keyboard(_)
                    | InlineMacro::Button(_)
                    | InlineMacro::Menu(_)
                    | InlineMacro::Mailto(_)
                    | InlineMacro::CrossReference(_)
                    | InlineMacro::Pass(_)
                    | InlineMacro::Stem(_)
                    | InlineMacro::IndexTerm(_)
                    | _ => None,
                }
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(actual, [false, true, true, false, true]);
    Ok(())
}
