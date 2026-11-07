//! Document labels agree across rendering and plain-text extraction.

use acdc_converters_core::{
    InlineTextTransform,
    xref::{XrefDisplay, XrefGuard, resolve_reference},
};
use acdc_parser::{CompatibilityMode, Options, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn automatic_document_labels_and_plain_text_share_source_context() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let source = "[[guide,Anchor]]\n= Main: Subtitle\n:reftext: Header\n\n<<guide>> xref:#[]\n\n:reftext: *Body* label\n\n<<guide>> xref:#[]\n\n:reftext:\n\n<<guide>> xref:#[]\n\n:reftext!:\n\n<<guide>> xref:#[]\n";
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let document = parsed.document();
        let transform = InlineTextTransform::default().references(document.references());
        let references = document
            .content_occurrences()
            .flat_map(|field| field.content().inlines())
            .filter_map(|node| {
                if let InlineKind::CrossReference(xref) = node.kind() {
                    Some((node, xref))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let expected = [
            "Header",
            "Header",
            "Body label",
            "Body label",
            "Main: Subtitle",
            "Main: Subtitle",
            "Main: Subtitle",
            "Main: Subtitle",
        ];
        assert_eq!(references.len(), expected.len());
        for ((node, xref), expected) in references.into_iter().zip(expected) {
            let guard = XrefGuard::default();
            let display = resolve_reference(document.references().get(xref.target()), xref, &guard);
            let (XrefDisplay::Label(content, _) | XrefDisplay::Title(content, _)) = &display else {
                return Err(format!("unexpected {display:?}").into());
            };
            assert_eq!(transform.content_to_string(*content), expected);
            assert_eq!(
                transform.content_to_string(acdc_parser::ast::ContentRef::from(
                    &acdc_parser::ContentAttribute::new(vec![acdc_parser::InlineNode::from(node)])
                )),
                expected
            );
            assert!(guard.is_resolving());
            drop(display);
            assert!(!guard.is_resolving());
        }
    }
    Ok(())
}
