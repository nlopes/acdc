use std::{alloc::System, hint::black_box};

use acdc_parser::{
    Bold, ContentAttribute, Form, Icon, InlineImage, InlineNode, Location, Plain, Source,
    ast::{AttributeValueRef, BoldRef, IconRef, InlineImageRef},
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[test]
fn block_content_and_substitution_reads_do_not_allocate() {
    let mut paragraph = acdc_parser::Paragraph::new(Vec::new(), Location::default());
    paragraph.metadata.attribution = Some(ContentAttribute::default());
    paragraph.metadata.citetitle = Some(ContentAttribute::default());
    paragraph.metadata.reftext = Some(ContentAttribute::default());
    paragraph.metadata.caption = Some(acdc_parser::Caption::CustomContent(
        ContentAttribute::default(),
    ));
    paragraph.metadata.substitutions = Some(acdc_parser::SubstitutionSpec::Source(vec![
        String::from("q"),
        String::from("+macros"),
        String::from("unknown"),
    ]));
    let region = Region::new(ALLOCATOR);
    let attributes = acdc_parser::ast::ParagraphRef::from(&paragraph).attributes();
    for (name, value) in attributes.iter() {
        black_box(attributes.get(name));
        black_box(attributes.contains(name));
        if let AttributeValueRef::Content(content) = value {
            for node in content.inlines() {
                black_box(node);
            }
        } else if let AttributeValueRef::Substitutions(spec) = value {
            black_box(spec.kind());
            black_box(spec.is_empty());
            for entry in spec.entries() {
                black_box(entry);
            }
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}

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
        id: Some(acdc_parser::ElementId::new("span", Location::default())),
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

#[test]
fn icon_reads_borrow_content_and_controls_without_allocating()
-> Result<(), Box<dyn std::error::Error>> {
    let mut icon = Icon::new(Source::Name("star"), Location::default());
    icon.set_attribute(String::from("custom").into(), String::from("owned").into())?;
    icon.set_attribute("size".into(), "2x".into())?;
    icon.metadata.roles.push("red");
    icon.options_mut().push("nofollow");
    icon.alt = Some(ContentAttribute::default());
    icon.tooltip = Some(ContentAttribute::default());
    let region = Region::new(ALLOCATOR);
    let icon = IconRef::from(&icon);
    black_box(icon.target().kind());
    black_box(icon.id());
    black_box(icon.size());
    black_box(icon.alt());
    black_box(icon.tooltip());
    for (name, value) in icon.attributes().iter() {
        black_box(icon.attributes().get(name));
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
    Ok(())
}

#[test]
fn hyperlink_reads_do_not_allocate() -> Result<(), Box<dyn std::error::Error>> {
    let mut link = acdc_parser::Link::new(Source::Name("guide.adoc"), Location::default());
    link.set_attribute(
        String::from("window").into(),
        String::from("reading").into(),
    )?;
    link.metadata.roles.push("one");
    link.options_mut().push("first");
    link.tooltip = Some(ContentAttribute::default());
    let mut mail =
        acdc_parser::Mailto::new(Source::Name("mailto:a@example.org"), Location::default());
    mail.subject = Some("Hello & café");
    mail.body = Some("");
    let region = Region::new(ALLOCATOR);
    let link = acdc_parser::ast::LinkRef::from(&link);
    black_box(link.target().kind());
    black_box(link.text().inlines().next());
    black_box(link.tooltip());
    black_box(link.id());
    black_box(link.window());
    black_box(link.hides_uri_scheme());
    for (name, value) in link.attributes().iter() {
        black_box(link.attributes().get(name));
        if let AttributeValueRef::Tokens(tokens) = value {
            for token in tokens.iter() {
                black_box(token);
            }
        }
    }
    let mail = acdc_parser::ast::MailtoRef::from(&mail);
    black_box(mail.subject());
    black_box(mail.body());
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
    Ok(())
}

#[test]
fn cross_reference_reads_do_not_allocate() -> Result<(), Box<dyn std::error::Error>> {
    let mut owner = acdc_parser::CrossReference::new("target", Location::default());
    owner.xrefstyle = acdc_parser::XrefStyle::Short;
    owner.metadata.roles.push("resource");
    owner.options_mut().push("nofollow");
    owner.set_attribute(String::from("custom").into(), String::from("owned").into())?;
    let region = Region::new(ALLOCATOR);
    let node = acdc_parser::ast::CrossReferenceRef::from(&owner);
    black_box(node.target());
    black_box(node.target_is_local());
    black_box(node.text().inlines().next());
    black_box(node.id());
    black_box(node.xrefstyle());
    black_box(node.caption_label());
    black_box(node.signifier());
    for (name, value) in node.attributes().iter() {
        black_box(node.attributes().get(name));
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
    Ok(())
}
