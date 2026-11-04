//! Boundary owners keep prepared content, identity order and assignment provenance.

type Error = Box<dyn std::error::Error>;

macro_rules! metadata_contract {
    ($module:ident, $owner:ident, $source:literal, $new:expr) => {
        mod $module {
            use super::Error;
            use acdc_parser::{Block, CompatibilityMode, Options, ast::BlockRef, parse};

            #[rstest::rstest]
            fn metadata_round_trips_and_rejected_replacements_are_atomic(
                #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
            ) -> Result<(), Error> {
                let parsed = parse(
                    $source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let mut block = Block::from(parsed.document().blocks().next().ok_or("block")?);
                let original = block.clone();
                let before = serde_json::to_value(&block)?;
                let Block::$owner(owner) = &mut block else {
                    return Err("kind".into());
                };
                let metadata = owner.take_metadata();
                owner.set_metadata(metadata)?;
                for name in [
                    "title",
                    "id",
                    "role",
                    "roles",
                    "opts",
                    "options",
                    "style",
                    "caption",
                    "reftext",
                    "subs",
                    "attribution",
                    "citetitle",
                ] {
                    let expected = owner.clone();
                    let mut invalid = owner.clone_metadata();
                    invalid.attributes.set(name.into(), "shadow".into());
                    assert!(owner.set_metadata(invalid).is_err(), "{name}");
                    assert_eq!(*owner, expected, "{name}");
                }
                assert_eq!(block, original);
                assert_eq!(serde_json::to_value(&block)?, before);
                assert!(BlockRef::from(&block).id().is_some());
                Ok(())
            }

            #[test]
            fn metadata_transfers_keep_owned_values_after_the_owner_is_dropped() -> Result<(), Error> {
                let mut original = $new;
                let mut metadata = original.take_metadata();
                let value = String::from("unchanged allocation");
                let pointer = value.as_ptr();
                metadata.attributes.set("custom".into(), value.into());
                original.set_metadata(metadata)?;
                let metadata = original.take_metadata();
                drop(original);
                let mut rebuilt = $new;
                rebuilt.set_metadata(metadata)?;
                let read = Block::$owner(rebuilt);
                let attributes = BlockRef::from(&read).attributes().ok_or("attributes")?;
                let Some(acdc_parser::ast::AttributeValueRef::Text(value)) = attributes.get("custom")
                else {
                    return Err("value".into());
                };
                assert_eq!(value.as_ptr(), pointer);
                Ok(())
            }
        }
    };
}

metadata_contract!(
    heading,
    DiscreteHeader,
    "[discrete#local.note]\n== A *heading*",
    acdc_parser::DiscreteHeader::new(
        acdc_parser::Title::default(),
        2,
        acdc_parser::Location::default()
    )
);
metadata_contract!(
    page,
    PageBreak,
    "[#local.note]\n.Title\n<<<",
    acdc_parser::PageBreak::new(acdc_parser::Location::default())
);
metadata_contract!(
    toc,
    TableOfContents,
    "[#local.note]\ntoc::[]",
    acdc_parser::TableOfContents::new(acdc_parser::Location::default())
);

#[rstest::rstest]
fn boundary_titles_keep_absent_empty_and_formatted_content(
    #[values(
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft
    )]
    mode: acdc_parser::CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{
        Block, Location, Options, PageBreak, ThematicBreak, Title,
        ast::{BlockRef, ContentRef},
        parse,
    };
    let parsed = parse(
        "[discrete]\n== A *heading*",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Block::DiscreteHeader(mut heading) =
        Block::from(parsed.document().blocks().next().ok_or("heading")?)
    else {
        return Err("heading".into());
    };
    let title = heading.take_title();
    heading.set_level(3);
    heading.set_title(title.clone());
    heading.set_location(Location::default());
    let mut page = PageBreak::new(Location::default()).with_title(title.clone());
    let mut rule = ThematicBreak::new(Location::default()).with_title(title);
    let page_title = page.take_title();
    page.set_title(page_title);
    let rule_title = rule.take_title();
    rule.set_title(rule_title);
    for mut block in [
        Block::DiscreteHeader(heading),
        Block::PageBreak(page),
        Block::ThematicBreak(rule),
    ] {
        assert!(
            BlockRef::from(&block)
                .title()
                .is_some_and(|title| !title.is_empty())
        );
        if let Block::DiscreteHeader(owner) = &mut block {
            owner.set_title(Title::new(Vec::new()));
        } else if let Block::PageBreak(owner) = &mut block {
            owner.set_title(Title::new(Vec::new()));
        } else if let Block::ThematicBreak(owner) = &mut block {
            owner.set_title(Title::new(Vec::new()));
        } else {
            return Err("title kind".into());
        }
        assert!(
            BlockRef::from(&block)
                .title()
                .is_some_and(ContentRef::is_empty)
        );
        let removed = if let Block::DiscreteHeader(owner) = &mut block {
            owner.take_title()
        } else if let Block::PageBreak(owner) = &mut block {
            owner.take_title()
        } else if let Block::ThematicBreak(owner) = &mut block {
            owner.take_title()
        } else {
            return Err("title kind".into());
        };
        assert!(removed.is_present());
        assert!(BlockRef::from(&block).title().is_none());
    }
    Ok(())
}

#[test]
fn thematic_anchor_transfers_preserve_order_content_and_source_lifetimes() -> Result<(), Error> {
    use acdc_parser::{Anchor, Location, ThematicBreak, ast::ThematicBreakRef};
    let id = String::from("original");
    let anchors = {
        let mut local = ThematicBreak::new(Location::default());
        local.set_anchors([
            Anchor::new(&id, Location::default()),
            Anchor::new("second", Location::default()),
        ]);
        local.take_anchors()
    };
    let mut rebuilt = ThematicBreak::new(Location::default());
    rebuilt.set_anchors(anchors);
    rebuilt.push_anchor(Anchor::new("second", Location::default()));
    rebuilt.set_location(Location::default());
    let read = ThematicBreakRef::from(&rebuilt);
    assert_eq!(read.id().ok_or("id")?.as_str(), "original");
    assert_eq!(
        read.anchors()
            .map(acdc_parser::ast::AnchorRef::id)
            .collect::<Vec<_>>(),
        ["original", "second", "second"]
    );
    assert_eq!(read.id().ok_or("id")?.as_str().as_ptr(), id.as_ptr());
    Ok(())
}

#[test]
fn comment_edits_preserve_literal_text_and_retained_category() {
    use acdc_parser::{Comment, CommentKind, Location, ast::CommentRef};
    let text = String::from("  *literal* {name} λ  ");
    let previous = {
        let mut comment = Comment::new(CommentKind::Line, &text, Location::default());
        comment.set_kind(CommentKind::Paragraph);
        comment.replace_text("")
    };
    let mut comment = Comment::new(CommentKind::Paragraph, previous, Location::default());
    comment.set_location(Location::default());
    assert_eq!(CommentRef::from(&comment).kind(), CommentKind::Paragraph);
    assert_eq!(CommentRef::from(&comment).text(), text);
    assert_eq!(CommentRef::from(&comment).text().as_ptr(), text.as_ptr());
}

#[rstest::rstest]
fn moving_an_assignment_span_does_not_replay_accepted_document_policy(
    #[values(
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft
    )]
    mode: acdc_parser::CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{Block, Location, Options, ast::DocumentAttributeRef, parse};
    let parsed = parse(
        "= Attributes\n:label: Header\n\nBody.\n\n:label: Body\n\n:label!:\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut count = 0;
    for block in parsed.document().blocks() {
        if let Block::DocumentAttribute(mut event) = Block::from(block) {
            let original = event.clone();
            let original = DocumentAttributeRef::from(&original);
            event.set_location(Location::default());
            assert_eq!(DocumentAttributeRef::from(&event).name(), original.name());
            assert_eq!(
                DocumentAttributeRef::from(&event).assignment(),
                original.assignment()
            );
            count += 1;
        }
    }
    assert_eq!(count, 2);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}
