//! Content copies and consuming transfers preserve inline values and title presence.

use std::{alloc::System, fmt::Write, hint::black_box};

use acdc_parser::{
    CompatibilityMode, ContentAttribute, InlineNode, Location, Options, Plain, Title,
    ast::{BlockKind, ContentRef, InlineKind},
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "= Doc\n\n.Title *bold*\n[#target,reftext='Ref _label_']\nimage:bird.svg[Alt *bold*,title='Tip _text_',id=pic] link:https://example.org[label] <<target>> footnote:[Note] ((term)).\n";

#[rstest::rstest]
fn selected_copies_preserve_inline_content_identity_and_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original_json = serde_json::to_value(parsed.document())?;
    let block = parsed.document().blocks().next().ok_or("block")?;
    let BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("paragraph".into());
    };
    let content = paragraph.content();
    assert!(
        content
            .inlines()
            .any(|node| matches!(node.kind(), InlineKind::Image(_)))
    );
    assert!(
        content
            .inlines()
            .any(|node| matches!(node.kind(), InlineKind::Footnote(_)))
    );
    let copy = ContentAttribute::from(content);
    assert_eq!(ContentRef::from(&copy), content);
    let original = copy.clone();
    let nodes = copy.into_inlines();
    assert_eq!(ContentAttribute::new(nodes), original);
    let title = block.title().ok_or("title")?;
    let title_copy = Title::from(title);
    assert!(title_copy.is_present());
    assert_eq!(title_copy.content(), Some(title));
    let original = title_copy.clone();
    assert_eq!(Title::new(title_copy.into_inlines()), original);
    assert_eq!(serde_json::to_value(parsed.document())?, original_json);
    Ok(())
}

#[test]
fn empty_content_copies_remain_present_titles() {
    let empty = ContentAttribute::default();
    let content = ContentRef::from(&empty);
    let copy = ContentAttribute::from(content);
    assert!(copy.is_empty());
    assert_eq!(copy.len(), 0);
    let title = Title::from(content);
    assert!(title.is_present());
    assert!(title.is_empty());
    assert_eq!(title.len(), 0);
    assert!(title.content().is_some());
    assert!(Title::default().content().is_none());
}

#[test]
fn optional_title_copies_keep_absence_separate_from_empty() {
    for title in [Title::default(), Title::new(Vec::new())] {
        let copied = title.content().map(Title::from).unwrap_or_default();
        assert_eq!(copied.is_present(), title.is_present());
        assert_eq!(copied, title);
    }
}

fn text(source: &str) -> InlineNode<'_> {
    InlineNode::PlainText(Plain {
        content: source,
        location: Location::default(),
        escaped: true,
    })
}

#[test]
fn consuming_transfers_outlive_the_previous_content_owner() -> Result<(), Error> {
    let source = String::from("λ *literal*");
    let (content, title) = {
        let content = ContentAttribute::new(vec![text(&source)]);
        let title = Title::new(vec![text(&source)]);
        (content.into_inlines(), title.into_inlines())
    };
    let content = ContentAttribute::new(content);
    let title = Title::new(title);
    for value in [ContentRef::from(&content), title.content().ok_or("title")?] {
        let InlineKind::Plain(plain) = value.inlines().next().ok_or("plain")?.kind() else {
            return Err("plain".into());
        };
        assert_eq!(plain.text(), source);
        assert_eq!(plain.text().as_ptr(), source.as_ptr());
        assert!(plain.is_escaped());
    }
    Ok(())
}

#[test]
fn consuming_transfers_move_the_existing_node_allocation() {
    let nodes = vec![text("content")];
    let original = nodes.as_ptr();
    let content = ContentAttribute::new(nodes);
    let region = Region::new(ALLOCATOR);
    let nodes = content.into_inlines();
    let title = Title::new(nodes);
    let nodes = title.into_inlines();
    let stats = region.change();
    assert_eq!(nodes.as_ptr(), original);
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}

#[test]
fn typed_content_reads_do_not_allocate() {
    let content = ContentAttribute::new(vec![text("content")]);
    let title = Title::new(vec![text("title")]);
    let region = Region::new(ALLOCATOR);
    for _ in 0..32 {
        black_box(ContentRef::from(&content).inlines().count());
        black_box(title.content().map(|value| value.inlines().count()));
        black_box(content.is_empty());
        black_box(content.len());
        black_box(title.is_empty());
        black_box(title.len());
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.deallocations, 0);
}

fn cost<T>(copy: impl FnOnce() -> T) -> (usize, usize) {
    let region = Region::new(ALLOCATOR);
    let value = black_box(copy());
    let stats = region.change();
    drop(value);
    (stats.allocations, stats.bytes_allocated)
}

#[test]
fn selected_content_copy_cost_ignores_unrelated_blocks() -> Result<(), Error> {
    let mut contents = Vec::new();
    let mut titles = Vec::new();
    for others in [0, 1000] {
        let mut source = String::from(".Title *bold*\nFirst _content_.\n\n");
        for index in 0..others {
            write!(source, "Paragraph {index}.\n\n")?;
        }
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(parsed.document().blocks().count(), 1 + others);
        let block = parsed.document().blocks().next().ok_or("block")?;
        let BlockKind::Paragraph(paragraph) = block.kind() else {
            return Err("paragraph".into());
        };
        contents.push(cost(|| ContentAttribute::from(paragraph.content())));
        let title = block.title().ok_or("title")?;
        titles.push(cost(|| Title::from(title)));
    }
    assert_eq!(contents.first(), contents.last());
    assert_eq!(titles.first(), titles.last());
    Ok(())
}

#[test]
fn copying_empty_content_and_titles_does_not_allocate() {
    let content = ContentAttribute::default();
    let view = ContentRef::from(&content);
    assert_eq!(cost(|| ContentAttribute::from(view)), (0, 0));
    assert_eq!(cost(|| Title::from(view)), (0, 0));
}
