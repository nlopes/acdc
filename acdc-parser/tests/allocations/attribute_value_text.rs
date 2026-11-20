//! `AttributeValueRef::as_str` does not allocate for any value kind.

use std::hint::black_box;

use acdc_parser::{
    ContentAttribute, IndexTerm, IndexTermKind, InlineImage, InlineNode, Location, Paragraph,
    Plain, Source, SubstitutionSpec,
    ast::{AttributeValueRef, ContentRef, IndexTermRef, InlineImageRef, ParagraphRef},
};
use stats_alloc::Region;

use super::ALLOCATOR;

type Error = Box<dyn std::error::Error>;

fn all_value_kinds_keep_their_semantics_without_allocating() -> Result<(), Error> {
    let empty = ContentAttribute::default();
    let content = ContentAttribute::new(vec![InlineNode::PlainText(Plain::new(
        "not a scalar",
        Location::default(),
    ))]);
    let index = IndexTerm::new(
        IndexTermKind::Concealed {
            term: Vec::new(),
            secondary: Some(Vec::new()),
            tertiary: None,
        },
        Location::default(),
    );
    let mut image = InlineImage::new(Source::Name("bird"), Location::default());
    let mut metadata = image.take_metadata();
    metadata.roles = vec!["one", "two"].into();
    image.set_metadata(metadata);
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let mut metadata = paragraph.take_metadata();
    metadata.substitutions = Some(SubstitutionSpec::Source(Vec::new()));
    paragraph.set_metadata(metadata)?;
    let values = [
        AttributeValueRef::Text(""),
        AttributeValueRef::Text("exact"),
        AttributeValueRef::Bool(false),
        AttributeValueRef::Bool(true),
        AttributeValueRef::None,
        AttributeValueRef::Content(ContentRef::from(&empty)),
        AttributeValueRef::Content(ContentRef::from(&content)),
        IndexTermRef::from(&index)
            .attributes()
            .get("terms")
            .ok_or("terms")?,
        AttributeValueRef::Tokens(InlineImageRef::from(&image).roles()),
        ParagraphRef::from(&paragraph)
            .attributes()
            .get("subs")
            .ok_or("substitutions")?,
    ];
    assert!(matches!(
        values.get(7),
        Some(AttributeValueRef::ContentList(_))
    ));
    assert!(matches!(
        values.get(9),
        Some(AttributeValueRef::Substitutions(_))
    ));
    for value in values.iter().skip(2) {
        assert_eq!(value.as_str(), None);
    }
    let region = Region::new(ALLOCATOR);
    let mut count = 0;
    for _ in 0..100 {
        for value in black_box(values) {
            if black_box(value).as_str().is_some() {
                count += 1;
            }
        }
    }
    let stats = region.change();
    assert_eq!(count, 200);
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
    Ok(())
}

pub(super) fn trials() -> Vec<libtest_mimic::Trial> {
    trials! {
        tests: [all_value_kinds_keep_their_semantics_without_allocating],
    }
}
