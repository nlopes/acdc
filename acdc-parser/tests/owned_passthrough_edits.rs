//! Passthrough transfers keep prepared text, substitution order and optional values.

use std::{alloc::System, hint::black_box};

use acdc_parser::{
    ElementId, InlineMacro, InlineMetadata, InlineNode, Location, Pass, PassthroughKind, Raw,
    Substitution,
    ast::{ElementIdRef, PassRef, RawRef},
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

fn profile() -> Vec<Substitution> {
    vec![
        Substitution::SpecialChars,
        Substitution::Quotes,
        Substitution::SpecialChars,
    ]
}

#[test]
fn raw_copies_keep_identity_roles_text_and_ordered_substitutions() {
    let mut raw = Raw::new("λ *literal*", profile(), Location::default());
    let mut metadata = InlineMetadata::default();
    metadata.id = Some(ElementId::new("raw", Location::default()));
    metadata.roles = vec!["first", "second", "first"].into();
    raw.set_metadata(Some(metadata));
    let copy = Raw::from(RawRef::from(&raw));
    assert_eq!(copy, raw);
    assert_eq!(RawRef::from(&copy).text(), "λ *literal*");
    assert_eq!(
        RawRef::from(&copy).substitutions().collect::<Vec<_>>(),
        profile()
    );
    assert_eq!(copy.clone_metadata(), raw.clone_metadata());
}

#[test]
fn raw_metadata_keeps_absence_distinct_from_present_empty() -> Result<(), Error> {
    let mut raw = Raw::new("", Vec::new(), Location::default());
    assert_eq!(raw.clone_metadata(), None);
    let absent = serde_json::to_value(InlineNode::RawText(raw.clone()))?;
    assert!(absent.get("metadata").is_none());
    raw.set_metadata(Some(InlineMetadata::default()));
    assert_eq!(raw.clone_metadata(), Some(InlineMetadata::default()));
    let present = serde_json::to_value(InlineNode::RawText(raw.clone()))?;
    assert_eq!(present, absent);
    assert_eq!(raw.take_metadata(), Some(InlineMetadata::default()));
    assert_eq!(raw.take_metadata(), None);
    assert_eq!(serde_json::to_value(InlineNode::RawText(raw))?, absent);
    Ok(())
}

#[test]
fn raw_metadata_transfers_keep_original_source_lifetimes() {
    let source = String::from("id");
    let metadata = {
        let mut raw = Raw::new("text", Vec::new(), Location::default());
        let mut metadata = InlineMetadata::default();
        metadata.id = Some(ElementId::new(source.as_str(), Location::default()));
        metadata.roles = vec![source.as_str()].into();
        raw.set_metadata(Some(metadata));
        raw.take_metadata()
    };
    let mut other = Raw::new("other", Vec::new(), Location::default());
    other.set_metadata(metadata);
    assert_eq!(
        RawRef::from(&other).id().map(ElementIdRef::as_str),
        Some(source.as_str())
    );
    assert_eq!(
        RawRef::from(&other).roles().iter().collect::<Vec<_>>(),
        [source.as_str()]
    );
}

#[test]
fn raw_text_transfers_leave_interpretation_and_identity_unchanged() {
    let source = String::from("*original*");
    let replacement = String::from("footnote:[unregistered] {unexpanded}");
    let old = {
        let mut raw = Raw::new(&source, profile(), Location::default());
        raw.set_metadata(Some(InlineMetadata::default()));
        let previous = raw.replace_text(&replacement);
        assert_eq!(RawRef::from(&raw).text(), replacement);
        assert_eq!(
            RawRef::from(&raw).substitutions().collect::<Vec<_>>(),
            profile()
        );
        assert_eq!(raw.clone_metadata(), Some(InlineMetadata::default()));
        previous
    };
    assert!(std::ptr::eq(old, source.as_str()));
}

#[test]
fn raw_substitution_transfers_keep_vector_storage_order_and_duplicates() {
    let initial = profile();
    let original_pointer = initial.as_ptr();
    let mut raw = Raw::new("<x>", initial, Location::default());
    let replacement = vec![Substitution::Attributes, Substitution::Attributes];
    let replacement_pointer = replacement.as_ptr();
    let old = raw.replace_substitutions(replacement);
    assert_eq!(old.as_ptr(), original_pointer);
    assert_eq!(old, profile());
    let current = raw.take_substitutions();
    assert_eq!(current.as_ptr(), replacement_pointer);
    assert_eq!(
        current,
        [Substitution::Attributes, Substitution::Attributes]
    );
    assert_eq!(RawRef::from(&raw).substitutions().count(), 0);
    assert_eq!(RawRef::from(&raw).text(), "<x>");
}

#[test]
fn raw_location_edits_preserve_all_prepared_values() {
    let mut raw = Raw::new("text", profile(), Location::default());
    let span = Location::point(acdc_parser::Position::new(3, 4));
    raw.set_location(span.clone());
    assert_eq!(Location::from(RawRef::from(&raw).location()), span);
    assert_eq!(RawRef::from(&raw).text(), "text");
    assert_eq!(
        RawRef::from(&raw).substitutions().collect::<Vec<_>>(),
        profile()
    );
}

#[test]
fn pass_copies_keep_optional_text_kind_and_substitutions() {
    for text in [None, Some(""), Some("λ *literal*")] {
        for kind in [
            PassthroughKind::Single,
            PassthroughKind::Double,
            PassthroughKind::Triple,
            PassthroughKind::Macro,
            PassthroughKind::AttributeRef,
        ] {
            let pass = Pass::new(text, profile(), kind.clone(), Location::default());
            let copy = Pass::from(PassRef::from(&pass));
            assert_eq!(copy, pass);
            assert_eq!(PassRef::from(&copy).text(), text);
            assert_eq!(PassRef::from(&copy).kind(), kind);
            assert_eq!(
                PassRef::from(&copy).substitutions().collect::<Vec<_>>(),
                profile()
            );
        }
    }
}

#[test]
fn pass_text_transfers_keep_absence_empty_and_source_lifetime_distinct() {
    let source = String::from("*original*");
    let replacement = String::from("image:literal[]");
    let old = {
        let mut pass = Pass::new(
            Some(&source),
            profile(),
            PassthroughKind::Macro,
            Location::default(),
        );
        let old = pass.replace_text(Some(&replacement));
        assert_eq!(PassRef::from(&pass).text(), Some(replacement.as_str()));
        assert_eq!(pass.replace_text(Some("")), Some(replacement.as_str()));
        assert_eq!(PassRef::from(&pass).text(), Some(""));
        assert_eq!(pass.replace_text(None), Some(""));
        assert_eq!(PassRef::from(&pass).text(), None);
        assert_eq!(
            PassRef::from(&pass).substitutions().collect::<Vec<_>>(),
            profile()
        );
        old
    };
    assert_eq!(old, Some(source.as_str()));
}

#[test]
fn pass_substitution_transfers_keep_vector_storage_order_and_duplicates() {
    let initial = profile();
    let original_pointer = initial.as_ptr();
    let mut pass = Pass::new(
        Some("<x>"),
        initial,
        PassthroughKind::Macro,
        Location::default(),
    );
    let replacement = vec![Substitution::Attributes, Substitution::Attributes];
    let replacement_pointer = replacement.as_ptr();
    let old = pass.replace_substitutions(replacement);
    assert_eq!(old.as_ptr(), original_pointer);
    assert_eq!(old, profile());
    let current = pass.take_substitutions();
    assert_eq!(current.as_ptr(), replacement_pointer);
    assert_eq!(
        current,
        [Substitution::Attributes, Substitution::Attributes]
    );
    assert_eq!(PassRef::from(&pass).substitutions().count(), 0);
    assert_eq!(PassRef::from(&pass).text(), Some("<x>"));
}

#[test]
fn pass_form_and_location_edits_do_not_process_or_reconstruct_text() {
    let mut pass = Pass::new(
        Some("*{literal}*"),
        profile(),
        PassthroughKind::Single,
        Location::default(),
    );
    pass.set_kind(PassthroughKind::Triple);
    let span = Location::point(acdc_parser::Position::new(3, 4));
    pass.set_location(span.clone());
    assert_eq!(PassRef::from(&pass).kind(), PassthroughKind::Triple);
    assert_eq!(Location::from(PassRef::from(&pass).location()), span);
    assert_eq!(PassRef::from(&pass).text(), Some("*{literal}*"));
    assert_eq!(
        PassRef::from(&pass).substitutions().collect::<Vec<_>>(),
        profile()
    );
}

#[test]
fn undecorated_passthrough_copies_do_not_allocate() {
    let raw = Raw::new("raw", Vec::new(), Location::default());
    let pass = Pass::new(
        None,
        Vec::new(),
        PassthroughKind::Macro,
        Location::default(),
    );
    let region = Region::new(ALLOCATOR);
    let raw_copy = black_box(Raw::from(RawRef::from(&raw)));
    let pass_copy = black_box(Pass::from(PassRef::from(&pass)));
    let change = region.change();
    assert_eq!(change.allocations, 0);
    assert_eq!(change.reallocations, 0);
    assert_eq!(raw_copy, raw);
    assert_eq!(pass_copy, pass);
}

#[test]
fn selected_passthrough_copies_keep_native_json() -> Result<(), Error> {
    let mut raw = Raw::new("λ <b>", profile(), Location::default());
    raw.set_metadata(Some(InlineMetadata::default()));
    assert_eq!(
        serde_json::to_value(InlineNode::RawText(Raw::from(RawRef::from(&raw))))?,
        serde_json::to_value(InlineNode::RawText(raw))?
    );
    let pass = Pass::new(None, profile(), PassthroughKind::Macro, Location::default());
    assert_eq!(
        serde_json::to_value(InlineNode::Macro(InlineMacro::Pass(Pass::from(
            PassRef::from(&pass)
        ))))?,
        serde_json::to_value(InlineNode::Macro(InlineMacro::Pass(pass)))?
    );
    Ok(())
}
