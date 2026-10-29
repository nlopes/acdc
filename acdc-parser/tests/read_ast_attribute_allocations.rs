use std::{alloc::System, hint::black_box};

use acdc_parser::{
    Bold, ContentAttribute, Form, InlineImage, InlineNode, Location, Plain, Source,
    ast::{AttributeValueRef, BoldRef, InlineImageRef},
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn direct_attribute_reads_do_not_allocate_for_owned_text_or_content()
-> Result<(), Box<dyn std::error::Error>> {
    let mut image = InlineImage::new(Source::Name("bird"), Location::default());
    image.set_attribute(
        String::from("custom").into(),
        String::from("\"owned text\"").into(),
    )?;
    image.metadata.roles = vec!["one", "two"];
    image.options_mut().extend(["first", "second"]);
    image.alt = Some(ContentAttribute::new(vec![InlineNode::PlainText(Plain {
        content: "Bird",
        location: Location::default(),
        escaped: false,
    })]));
    image.tooltip = Some(ContentAttribute::default());
    let region = Region::new(ALLOCATOR);
    let attributes = InlineImageRef::from(&image).attributes();
    black_box(attributes.get(black_box("custom")));
    black_box(attributes.contains(black_box("role")));
    for (name, value) in attributes.iter() {
        black_box(name);
        black_box(value);
        if let AttributeValueRef::Tokens(tokens) = value {
            for token in tokens.iter() {
                black_box(token);
            }
        } else if let AttributeValueRef::Content(content) = value {
            for inline in content.inlines() {
                black_box(inline);
            }
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
    Ok(())
}

#[test]
fn span_attribute_reads_split_role_text_without_allocating() {
    let bold = Bold {
        role: Some("one\ttwo\u{2003}one"),
        id: Some("span"),
        form: Form::Unconstrained,
        content: vec![InlineNode::PlainText(Plain {
            content: "content",
            location: Location::default(),
            escaped: false,
        })],
        location: Location::default(),
    };
    let region = Region::new(ALLOCATOR);
    let span = BoldRef::from(&bold);
    black_box(span.id());
    black_box(span.form());
    black_box(span.roles().contains(black_box("two")));
    black_box(span.roles().is_empty());
    let roles = black_box(span.roles());
    black_box(span.roles() == roles);
    for inline in span.content().inlines() {
        black_box(inline);
    }
    for (name, value) in span.attributes().iter() {
        black_box(span.attributes().get(name));
        if let AttributeValueRef::Tokens(tokens) = value {
            for token in tokens.iter() {
                black_box(token);
            }
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}
