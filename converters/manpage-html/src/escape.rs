pub fn escape_html(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(ch),
        }
    }
    result
}

pub fn extract_plain_text(nodes: &[acdc_parser::InlineNode]) -> String {
    let mut result = String::new();
    for node in nodes {
        match node {
            acdc_parser::InlineNode::PlainText(text) => {
                result.push_str(&text.content);
            }
            acdc_parser::InlineNode::RawText(text) => {
                result.push_str(&text.content);
            }
            acdc_parser::InlineNode::VerbatimText(text) => result.push_str(&text.content),
            acdc_parser::InlineNode::BoldText(bold) => {
                result.push_str(&extract_plain_text(&bold.content));
            }
            acdc_parser::InlineNode::ItalicText(italic) => {
                result.push_str(&extract_plain_text(&italic.content));
            }
            acdc_parser::InlineNode::MonospaceText(mono) => {
                result.push_str(&extract_plain_text(&mono.content));
            }
            acdc_parser::InlineNode::HighlightText(highlight) => {
                result.push_str(&extract_plain_text(&highlight.content));
            }
            acdc_parser::InlineNode::SubscriptText(sub) => {
                result.push_str(&extract_plain_text(&sub.content));
            }
            acdc_parser::InlineNode::SuperscriptText(sup) => {
                result.push_str(&extract_plain_text(&sup.content));
            }
            acdc_parser::InlineNode::CurvedQuotationText(quoted) => {
                result.push_str(&extract_plain_text(&quoted.content));
            }
            acdc_parser::InlineNode::CurvedApostropheText(quoted) => {
                result.push_str(&extract_plain_text(&quoted.content));
            }
            #[allow(clippy::match_same_arms, clippy::wildcard_enum_match_arm)]
            acdc_parser::InlineNode::StandaloneCurvedApostrophe(_)
            | acdc_parser::InlineNode::LineBreak(_)
            | acdc_parser::InlineNode::InlineAnchor(_)
            | acdc_parser::InlineNode::Macro(_)
            | _ => {}
        }
    }
    result
}
