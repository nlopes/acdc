//! Prepared anchor, note and callout edits retain labels and registration snapshots.

use std::{alloc::System, hint::black_box};

use acdc_parser::{
    Anchor, CalloutMarker, CalloutMarkerKind, CompatibilityMode, ContentAttribute, Footnote,
    InlineNode, Location, Options, Plain, Position,
    ast::{AnchorRef, BlockKind, ContentRef, FootnoteRef, InlineKind},
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

fn plain(text: &str) -> InlineNode<'_> {
    InlineNode::PlainText(Plain::new(text, Location::default()).with_escaped(true))
}

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    content
        .inlines()
        .map(|inline| {
            if let InlineKind::Plain(value) = inline.kind() {
                Ok(value.text().to_owned())
            } else if let InlineKind::Raw(value) = inline.kind() {
                Ok(value.text().to_owned())
            } else if let InlineKind::Bold(value) = inline.kind() {
                text(value.content())
            } else {
                Err("expected text".into())
            }
        })
        .collect()
}

#[test]
fn anchor_prepared_labels_keep_citation_separate_from_bibliography_display() -> Result<(), Error> {
    let mut anchor = Anchor::new("book", Location::default());
    anchor.set_reftext(Some(ContentAttribute::new(vec![plain("Citation")])));
    anchor.set_bibliography(Some(ContentAttribute::new(vec![plain("Display")])));
    let read = AnchorRef::from(&anchor);
    assert!(read.is_bibliography());
    assert_eq!(text(read.reftext().ok_or("citation")?)?, "Citation");
    assert_eq!(
        text(read.bibliography_label().ok_or("display")?)?,
        "Display"
    );
    let display = anchor.take_bibliography_label().ok_or("display transfer")?;
    assert!(AnchorRef::from(&anchor).is_bibliography());
    assert!(AnchorRef::from(&anchor).bibliography_label().is_none());
    assert_eq!(text(ContentRef::from(&display))?, "Display");
    anchor.set_bibliography(Some(display));
    anchor.clear_bibliography();
    assert!(!AnchorRef::from(&anchor).is_bibliography());
    assert!(AnchorRef::from(&anchor).bibliography_label().is_none());
    assert_eq!(
        text(AnchorRef::from(&anchor).reftext().ok_or("citation")?)?,
        "Citation"
    );
    Ok(())
}

#[test]
fn anchor_labels_preserve_absence_empty_presence_and_owned_buffers() -> Result<(), Error> {
    let source = String::from("{prepared} *literal*");
    let body = vec![plain(&source)];
    let pointer = body.as_ptr();
    let (reference, display) = {
        let mut anchor = Anchor::new("entry", Location::default());
        assert!(anchor.take_reftext().is_none());
        anchor.set_reftext(Some(ContentAttribute::new(body)));
        anchor.set_bibliography(None);
        assert!(AnchorRef::from(&anchor).is_bibliography());
        assert!(AnchorRef::from(&anchor).bibliography_label().is_none());
        anchor.set_bibliography(Some(ContentAttribute::default()));
        assert!(
            AnchorRef::from(&anchor)
                .bibliography_label()
                .is_some_and(ContentRef::is_empty)
        );
        (
            anchor.take_reftext().map(ContentAttribute::into_inlines),
            anchor.take_bibliography_label(),
        )
    };
    let reference = reference.ok_or("prepared reference text")?;
    assert_eq!(reference.as_ptr(), pointer);
    assert_eq!(reference, [plain(&source)]);
    assert_eq!(display, Some(ContentAttribute::default()));
    Ok(())
}

#[test]
fn bibliography_label_transfer_reuses_its_prepared_inline_buffer() -> Result<(), Error> {
    let source = String::from("Display");
    let body = vec![plain(&source)];
    let pointer = body.as_ptr();
    let transferred = {
        let mut anchor = Anchor::new("entry", Location::default());
        anchor.set_bibliography(Some(ContentAttribute::new(body)));
        anchor
            .take_bibliography_label()
            .ok_or("display")?
            .into_inlines()
    };
    assert_eq!(transferred.as_ptr(), pointer);
    assert_eq!(transferred, [plain(&source)]);
    Ok(())
}

#[test]
fn anchor_identity_and_reftext_transfers_outlive_the_previous_record() {
    let source = String::from("original-id");
    let replacement = String::from("new-id");
    let old = {
        let mut anchor = Anchor::new(&source, Location::default());
        let old = anchor.replace_id(&replacement);
        assert_eq!(AnchorRef::from(&anchor).id(), replacement);
        anchor.set_reftext(Some(ContentAttribute::default()));
        assert_eq!(anchor.take_reftext(), Some(ContentAttribute::default()));
        assert!(AnchorRef::from(&anchor).reftext().is_none());
        old
    };
    assert_eq!(old, source);
}

#[rstest::rstest]
fn selected_anchor_copies_keep_bibliography_labels_and_native_json(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[bibliography]\n* [[[book,*Bold*]]] Entry.\n* [[[fallback]]] Entry.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let BlockKind::UnorderedList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("list".into());
    };
    assert_eq!(list.items().count(), 2);
    for (index, item) in list.items().enumerate() {
        let anchor = item
            .principal()
            .inlines()
            .find_map(|inline| {
                if let InlineKind::Anchor(anchor) = inline.kind() {
                    Some(anchor)
                } else {
                    None
                }
            })
            .ok_or("anchor")?;
        let mut copy = Anchor::from(anchor);
        assert_eq!(
            &serde_json::to_value(InlineNode::InlineAnchor(copy.clone()))?,
            before
                .pointer(&format!("/blocks/0/items/{index}/principal/0"))
                .ok_or("native anchor")?
        );
        let original = copy.clone();
        copy.replace_id("changed");
        copy.set_reftext(Some(ContentAttribute::default()));
        copy.clear_bibliography();
        assert_eq!(Anchor::from(anchor), original);
    }
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn footnote_content_transfers_keep_buffers_source_lifetimes_and_assigned_numbers()
-> Result<(), Error> {
    let source = String::from("first");
    let replacement = String::from("footnote:new[prepared]");
    let original = vec![plain(&source)];
    let next = vec![plain(&replacement)];
    let original_pointer = original.as_ptr();
    let (old, taken, old_id) = {
        let mut note = Footnote::new(original, 17, Location::default());
        assert_eq!(FootnoteRef::from(&note).id(), None);
        note.replace_id(Some(&source));
        let old_id = note.replace_id(Some(&replacement));
        let old = note.replace_content(Some(next));
        assert_eq!(FootnoteRef::from(&note).number(), 17);
        note.push_inline(plain("tail"));
        assert_eq!(
            FootnoteRef::from(&note)
                .content()
                .ok_or("definition body")?
                .inlines()
                .count(),
            2
        );
        // Push can grow a Vec; use a second transfer to check the original replacement buffer.
        let mut second = Footnote::new(Vec::new(), 0, Location::default());
        second.replace_content(old);
        let old = second.take_content().ok_or("transferred definition")?;
        let taken = note.take_content().ok_or("transferred definition")?;
        assert!(FootnoteRef::from(&note).content().is_none());
        assert_eq!(FootnoteRef::from(&note).number(), 17);
        (old, taken, old_id)
    };
    assert_eq!(old.as_ptr(), original_pointer);
    assert_eq!(old, [plain(&source)]);
    assert_eq!(taken, [plain(&replacement), plain("tail")]);
    assert_eq!(old_id, Some(source.as_str()));
    let mut note = Footnote::new(Vec::new(), 0, Location::default());
    let next = vec![plain(&replacement)];
    let pointer = next.as_ptr();
    note.replace_content(Some(next));
    let taken = note.take_content().ok_or("transferred definition")?;
    assert_eq!(taken.as_ptr(), pointer);
    Ok(())
}

#[rstest::rstest]
fn editing_copied_notes_does_not_reregister_or_renumber_document_snapshots(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "footnote:n[*First*] footnote:n[] footnote:n[Other] footnote:[Anonymous]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let BlockKind::Paragraph(paragraph) =
        parsed.document().blocks().next().ok_or("paragraph")?.kind()
    else {
        return Err("paragraph".into());
    };
    let mut observations = Vec::new();
    for inline in paragraph.content().inlines() {
        if let InlineKind::Footnote(note) = inline.kind() {
            let original = Footnote::from(note);
            let mut copy = Footnote::from(note);
            assert_eq!(copy, original);
            observations.push((note.id(), note.number(), note.content().is_none()));
            copy.replace_id(Some("unregistered"));
            copy.replace_content(Some(vec![plain("{unexpanded} *unparsed*")]));
            copy.set_number(99);
            assert_eq!(
                text(
                    FootnoteRef::from(&copy)
                        .content()
                        .ok_or("prepared definition")?
                )?,
                "{unexpanded} *unparsed*"
            );
            assert_eq!(Footnote::from(note), original);
        }
    }
    assert_eq!(
        observations,
        [
            (Some("n"), 1, false),
            (Some("n"), 1, true),
            (Some("n"), 1, true),
            (None, 2, false)
        ]
    );
    assert_eq!(parsed.document().footnotes().count(), 2);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn note_and_anchor_location_edits_keep_prepared_child_spans() -> Result<(), Error> {
    let original = Location::point(Position::new(2, 3));
    let replacement = Location::point(Position::new(7, 11));
    let body = vec![InlineNode::PlainText(Plain::new("text", original.clone()))];
    let mut note = Footnote::new(body.clone(), 0, original.clone());
    note.set_location(replacement.clone());
    assert_eq!(
        Location::from(FootnoteRef::from(&note).location()),
        replacement
    );
    assert_eq!(
        Location::from(
            FootnoteRef::from(&note)
                .content()
                .ok_or("definition body")?
                .inlines()
                .next()
                .ok_or("child")?
                .location()
        ),
        original
    );
    let mut anchor = Anchor::new("id", original.clone());
    anchor.set_reftext(Some(ContentAttribute::new(body)));
    anchor.set_location(replacement.clone());
    let read = AnchorRef::from(&anchor);
    assert_eq!(Location::from(read.location()), replacement);
    assert_eq!(
        Location::from(
            read.reftext()
                .ok_or("label")?
                .inlines()
                .next()
                .ok_or("child")?
                .location()
        ),
        original
    );
    Ok(())
}

#[test]
fn callout_edits_keep_explicit_auto_and_zero_without_sequence_renumbering() -> Result<(), Error> {
    for mut callout in [
        CalloutMarker::explicit(0, Location::default()),
        CalloutMarker::auto(3, Location::default()),
    ] {
        let read = acdc_parser::ast::CalloutMarkerRef::from(&callout);
        assert_eq!(CalloutMarker::from(read), callout);
        let original = serde_json::to_value(&callout)?;
        assert_eq!(serde_json::to_value(CalloutMarker::from(read))?, original);
        callout.set_kind(CalloutMarkerKind::Auto);
        callout.set_number(9);
        let location = Location::point(Position::new(8, 2));
        callout.set_location(location.clone());
        let read = acdc_parser::ast::CalloutMarkerRef::from(&callout);
        assert_eq!(read.kind(), CalloutMarkerKind::Auto);
        assert_eq!(read.number(), 9);
        assert_eq!(Location::from(read.location()), location);
        callout.set_kind(CalloutMarkerKind::Explicit);
        callout.set_number(0);
        assert_eq!(
            acdc_parser::ast::CalloutMarkerRef::from(&callout).number(),
            0
        );
    }
    Ok(())
}

#[test]
fn empty_selected_note_anchor_and_callout_copies_allocate_nothing() {
    let anchor = Anchor::new("entry", Location::default());
    let note = Footnote::new(Vec::new(), 0, Location::default());
    let callout = CalloutMarker::auto(0, Location::default());
    let region = Region::new(ALLOCATOR);
    let copies = black_box((
        Anchor::from(AnchorRef::from(&anchor)),
        Footnote::from(FootnoteRef::from(&note)),
        CalloutMarker::from(acdc_parser::ast::CalloutMarkerRef::from(&callout)),
    ));
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(copies.0, anchor);
    assert_eq!(copies.1, note);
    assert_eq!(copies.2, callout);
}
