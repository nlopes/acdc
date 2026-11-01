//! Code-content selection through the public parser read API.

use std::error::Error;

use acdc_converters_core::{
    InlineTextTransform,
    code::{code_inline_children, code_inline_content},
};
use acdc_parser::{CompatibilityMode, InlineNode, Options, ast::InlineRef, parse_inline};

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
            let [node] = parsed.inlines() else {
                return Err(format!("expected one node: {input}").into());
            };
            let content = code_inline_content(node.into()).ok_or("missing typed content")?;
            let original = code_inline_children(node).ok_or("missing owner content")?;
            let mut text = String::new();
            InlineTextTransform::default().write_inlines(&mut text, content.inlines())?;
            assert_eq!(text, expected, "{mode:?} {input}");
            assert_eq!(content.inlines().count(), original.len());
            for (child, original) in content.inlines().zip(original) {
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
            assert!(!parsed.inlines().is_empty(), "{mode:?} {input}");
            for node in parsed.inlines() {
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
        let [InlineNode::BoldText(span)] = parsed.inlines() else {
            return Err("expected bold text".into());
        };
        let mut span = span.clone();
        span.content.clear();
        let node = InlineNode::BoldText(span);
        let retained = node.clone();
        let content = code_inline_content((&node).into()).ok_or("empty content is present")?;
        assert_eq!(content.inlines().count(), 0);
        assert_eq!(
            code_inline_children(&node).map(<[InlineNode<'_>]>::len),
            Some(0)
        );
        assert_eq!(node, retained);
    }
    Ok(())
}
