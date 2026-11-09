use crate::{Block, Document, Location, Title, asg};

#[test]
fn section_policy_errors_identify_the_native_property() {
    for (kind, numbered, expected) in [
        (crate::SectionKind::Appendix, false, "kind"),
        (crate::SectionKind::Normal, true, "numbering"),
    ] {
        let mut section =
            crate::Section::new(Title::new(Vec::new()), 1, Vec::new(), Location::default())
                .with_numbering(numbered);
        section.kind = kind;
        let mut document = Document::default();
        document.push_block(Block::Section(section));
        assert!(
            matches!(asg::export_document((&document).into()), Err(asg::Error::Unsupported {
            path, construct: Some("section"), ..
        }) if path == format!("/blocks/0/{expected}"))
        );
    }
}
