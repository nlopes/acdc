//! Code-content selection through the public parser read API.

use std::error::Error;

use acdc_converters_core::{InlineTextTransform, code::code_inline_content};
use acdc_parser::{
    CompatibilityMode, IndexTermKind, InlineMacro, InlineNode, Options, ast::InlineRef,
    parse_inline,
};

fn code_inline_children<'source>(node: &InlineNode<'source>) -> Option<Vec<InlineNode<'source>>> {
    Some(match node.clone() {
        InlineNode::BoldText(mut span) => span.take_content(),
        InlineNode::ItalicText(mut span) => span.take_content(),
        InlineNode::MonospaceText(mut span) => span.take_content(),
        InlineNode::HighlightText(mut span) => span.take_content(),
        InlineNode::SubscriptText(mut span) => span.take_content(),
        InlineNode::SuperscriptText(mut span) => span.take_content(),
        InlineNode::DoubleQuotedSpan(mut span) => span.take_content(),
        InlineNode::SingleQuotedSpan(mut span) => span.take_content(),
        InlineNode::Macro(InlineMacro::IndexTerm(mut term)) if term.is_visible() => {
            if let IndexTermKind::Flow(content) = term.replace_kind(IndexTermKind::Flow(Vec::new()))
            {
                content
            } else {
                return None;
            }
        }
        InlineNode::PlainText(_)
        | InlineNode::Symbol(_)
        | InlineNode::CharacterReference(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutMarker(_)
        | _ => return None,
    })
}

#[test]
fn typed_code_content_preserves_selection_and_locations() -> Result<(), Box<dyn Error>> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (input, expected) in [
            ("*bold _nested_*", "bold nested"),
            ("_italic_", "italic"),
            ("`mono`", "mono"),
            ("#marked#", "marked"),
            ("~sub~", "sub"),
            ("^sup^", "sup"),
            ("\"`double`\"", "double"),
            ("'`single`'", "single"),
            ("indexterm2:[visible]", "visible"),
        ] {
            let parsed = parse_inline(input, &options)?;
            let parsed_inlines =
                acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
            let [node] = parsed_inlines.as_slice() else {
                return Err(format!("expected one node: {input}").into());
            };
            let content = code_inline_content(node.into()).ok_or("missing typed content")?;
            let original = code_inline_children(node).ok_or("missing owner content")?;
            let mut text = String::new();
            InlineTextTransform::default().write_inlines(&mut text, content.inlines())?;
            assert_eq!(text, expected, "{mode:?} {input}");
            assert_eq!(content.inlines().count(), original.len());
            for (child, original) in content.inlines().zip(&original) {
                assert_eq!(child.location(), InlineRef::from(original).location());
                assert_eq!(
                    std::mem::discriminant(&child.kind()),
                    std::mem::discriminant(&InlineRef::from(original).kind())
                );
            }
        }
        for input in [
            "plain",
            "indexterm:[hidden]",
            "image:picture.png[alt]",
            "icon:heart[]",
            "link:page.html[label]",
            "https://example.org[label]",
            "<<target,label>>",
            "footnote:[note]",
            "kbd:[Ctrl+C]",
            "btn:[Save]",
            "menu:File[Open]",
            "stem:[x]",
            "[[anchor]]",
            "pass:[raw]",
        ] {
            let parsed = parse_inline(input, &options)?;
            let parsed_inlines =
                acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
            assert!(!parsed_inlines.as_slice().is_empty(), "{mode:?} {input}");
            for node in parsed_inlines.as_slice() {
                assert!(
                    code_inline_content(node.into()).is_none(),
                    "{mode:?} {input}"
                );
                assert!(code_inline_children(node).is_none(), "{mode:?} {input}");
            }
        }
    }
    Ok(())
}

#[test]
fn typed_code_content_keeps_present_empty_formatting() -> Result<(), Box<dyn Error>> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_inline(
            "[.keep]*bold*",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
        let [InlineNode::BoldText(span)] = parsed_inlines.as_slice() else {
            return Err("expected bold text".into());
        };
        let mut span = span.clone();
        span.take_content();
        let node = InlineNode::BoldText(span);
        let retained = node.clone();
        let content = code_inline_content((&node).into()).ok_or("empty content is present")?;
        assert_eq!(content.inlines().count(), 0);
        assert_eq!(
            code_inline_children(&node).map(|nodes| nodes.len()),
            Some(0)
        );
        assert_eq!(node, retained);
    }
    Ok(())
}
