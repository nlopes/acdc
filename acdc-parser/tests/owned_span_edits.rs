//! Concrete formatting owners transfer prepared content and retain identity presence.

use std::{alloc::System, hint::black_box};

use acdc_parser::{
    Bold, DoubleQuotedSpan, ElementId, FormattingForm, Highlight, InlineNode, Italic, Location,
    Monospace, Plain, Position, SingleQuotedSpan, Subscript, Superscript,
    ast::{
        BoldRef, ContentRef, DoubleQuotedSpanRef, HighlightRef, InlineKind, ItalicRef,
        MonospaceRef, SingleQuotedSpanRef, SubscriptRef, SuperscriptRef,
    },
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

macro_rules! all_spans {
    ($case:ident) => {
        $case!(Bold, BoldRef, BoldText);
        $case!(Italic, ItalicRef, ItalicText);
        $case!(Monospace, MonospaceRef, MonospaceText);
        $case!(Highlight, HighlightRef, HighlightText);
        $case!(Subscript, SubscriptRef, SubscriptText);
        $case!(Superscript, SuperscriptRef, SuperscriptText);
        $case!(DoubleQuotedSpan, DoubleQuotedSpanRef, DoubleQuotedSpan);
        $case!(SingleQuotedSpan, SingleQuotedSpanRef, SingleQuotedSpan);
    };
}

fn plain(text: &str) -> InlineNode<'_> {
    InlineNode::PlainText(Plain::new(text, Location::default()).with_escaped(true))
}

fn text(content: ContentRef<'_>) -> Result<&str, Error> {
    let mut inlines = content.inlines();
    let (Some(inline), None) = (inlines.next(), inlines.next()) else {
        return Err("expected one prepared inline".into());
    };
    let InlineKind::Plain(plain) = inline.kind() else {
        return Err("expected protected ordinary text".into());
    };
    assert!(plain.is_escaped());
    Ok(plain.text())
}

#[test]
fn all_concrete_span_copies_keep_both_forms_and_every_prepared_value() -> Result<(), Error> {
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            for form in [FormattingForm::Constrained, FormattingForm::Unconstrained] {
                let mut original = $owner::new(
                    vec![plain("λ *prepared*")],
                    form.clone(),
                    Location::default(),
                );
                original.replace_role(Some("one  two one"));
                original.set_id(Some(ElementId::new(
                    "span",
                    Location::point(Position::new(3, 4)),
                )));
                let copy = $owner::from($read::from(&original));
                assert_eq!(copy, original);
                assert_eq!($read::from(&copy).form(), form);
                assert_eq!(text($read::from(&copy).content())?, "λ *prepared*");
                assert_eq!($read::from(&copy).role(), Some("one  two one"));
                assert_eq!(
                    $read::from(&copy)
                        .id()
                        .map(acdc_parser::ast::ElementIdRef::as_str),
                    Some("span")
                );
            }
        };
    }
    all_spans!(check);
    Ok(())
}

#[test]
fn span_content_transfers_keep_buffers_and_original_source_lifetimes() {
    let source = String::from("*original*");
    let replacement = String::from("footnote:[unregistered]");
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let initial = vec![plain(&source)];
            let initial_pointer = initial.as_ptr();
            let new = vec![plain(&replacement)];
            let replacement_pointer = new.as_ptr();
            let (old, transferred) = {
                let mut span =
                    $owner::new(initial, FormattingForm::Constrained, Location::default());
                let old = span.replace_content(new);
                let transferred = span.take_content();
                assert!($read::from(&span).content().is_empty());
                (old, transferred)
            };
            assert_eq!(old.as_ptr(), initial_pointer);
            assert_eq!(transferred.as_ptr(), replacement_pointer);
            assert_eq!(old, [plain(&source)]);
            assert_eq!(transferred, [plain(&replacement)]);
        };
    }
    all_spans!(check);
}

#[test]
fn span_role_transfers_keep_whitespace_duplicates_and_empty_presence() {
    let source = String::from("\tone\u{2003}two one\n");
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let old = {
                let mut span =
                    $owner::new(Vec::new(), FormattingForm::Constrained, Location::default());
                assert_eq!(span.replace_role(Some(&source)), None);
                assert_eq!($read::from(&span).role(), Some(source.as_str()));
                assert_eq!(
                    $read::from(&span).roles().iter().collect::<Vec<_>>(),
                    ["one", "two", "one"]
                );
                let old = span.replace_role(Some(""));
                assert_eq!($read::from(&span).role(), Some(""));
                assert!($read::from(&span).roles().is_empty());
                assert_eq!(span.replace_role(None), Some(""));
                assert_eq!($read::from(&span).role(), None);
                old
            };
            assert_eq!(old, Some(source.as_str()));
        };
    }
    all_spans!(check);
}

#[test]
fn span_id_transfers_keep_their_own_locations_and_source_lifetimes() {
    let source = String::from("λ-span");
    let span_location = Location::point(Position::new(3, 4));
    let id_location = Location::point(Position::new(2, 2));
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let id = {
                let mut span =
                    $owner::new(Vec::new(), FormattingForm::Constrained, Location::default());
                span.set_id(Some(ElementId::new(source.as_str(), id_location.clone())));
                span.set_location(span_location.clone());
                let read = $read::from(&span);
                assert_eq!(Location::from(read.location()), span_location);
                assert_eq!(
                    read.id().and_then(|id| id.location()).map(Location::from),
                    Some(id_location.clone())
                );
                let id = span.take_id();
                assert!($read::from(&span).id().is_none());
                id
            };
            assert_eq!(
                id,
                Some(ElementId::new(source.as_str(), id_location.clone()))
            );
        };
    }
    all_spans!(check);
}

#[test]
fn span_copy_edits_do_not_change_original_children_identity_or_form() -> Result<(), Error> {
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let mut original = $owner::new(
                vec![plain("original")],
                FormattingForm::Constrained,
                Location::default(),
            );
            original.replace_role(Some("source"));
            original.set_id(Some(ElementId::new("original", Location::default())));
            let mut copy = $owner::from($read::from(&original));
            copy.set_form(FormattingForm::Unconstrained);
            copy.replace_role(Some("copy"));
            copy.take_id();
            copy.take_content();
            copy.push_inline(plain("copy"));
            assert_eq!(text($read::from(&original).content())?, "original");
            assert_eq!($read::from(&original).role(), Some("source"));
            assert_eq!($read::from(&original).form(), FormattingForm::Constrained);
            assert_eq!(
                $read::from(&original)
                    .id()
                    .map(acdc_parser::ast::ElementIdRef::as_str),
                Some("original")
            );
            assert_eq!(text($read::from(&copy).content())?, "copy");
            assert_eq!($read::from(&copy).form(), FormattingForm::Unconstrained);
        };
    }
    all_spans!(check);
    Ok(())
}

#[test]
fn highlight_marking_depends_on_identity_presence_even_when_empty() {
    let mut span = Highlight::new(Vec::new(), FormattingForm::Constrained, Location::default());
    assert!(HighlightRef::from(&span).is_marked());
    span.replace_role(Some(""));
    assert!(!HighlightRef::from(&span).is_marked());
    span.replace_role(None);
    assert!(HighlightRef::from(&span).is_marked());
    span.set_id(Some(ElementId::new("", Location::default())));
    assert!(!HighlightRef::from(&span).is_marked());
    span.take_id();
    assert!(HighlightRef::from(&span).is_marked());
}

#[test]
fn empty_span_copies_do_not_allocate() {
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let original = $owner::new(
                Vec::new(),
                FormattingForm::Unconstrained,
                Location::default(),
            );
            let region = Region::new(ALLOCATOR);
            let copy = black_box($owner::from($read::from(&original)));
            let stats = region.change();
            assert_eq!(stats.allocations, 0);
            assert_eq!(stats.reallocations, 0);
            assert_eq!(copy, original);
        };
    }
    all_spans!(check);
}

#[test]
fn selected_span_copies_keep_native_json() -> Result<(), Error> {
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let mut original = $owner::new(
                vec![plain("λ *prepared*")],
                FormattingForm::Unconstrained,
                Location::default(),
            );
            original.replace_role(Some(""));
            original.set_id(Some(ElementId::new("span", Location::default())));
            let copy = $owner::from($read::from(&original));
            assert_eq!(
                serde_json::to_value(InlineNode::$variant(copy))?,
                serde_json::to_value(InlineNode::$variant(original))?
            );
        };
    }
    all_spans!(check);
    Ok(())
}
