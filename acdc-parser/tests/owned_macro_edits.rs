//! Prepared UI and reference edits preserve content and registration snapshots.

use std::alloc::System;

use acdc_parser::{
    Button, CompatibilityMode, ContentAttribute, InlineMacro, InlineNode, Keyboard, Location, Menu,
    Options, Plain,
    ast::{ButtonRef, ContentRef, InlineKind, KeyboardRef, MenuRef},
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

fn label(text: &str) -> ContentAttribute<'_> {
    ContentAttribute::new(vec![InlineNode::PlainText(Plain::new(
        text,
        Location::default(),
    ))])
}

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    content
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(plain) = node.kind() {
                Ok(plain.text().to_owned())
            } else {
                Err(format!("expected prepared plain content: {node:?}").into())
            }
        })
        .collect()
}

#[test]
fn button_transfers_prepared_label_without_parsing() -> Result<(), Error> {
    let authored = String::from("*Save* {entry} footnote:[literal]");
    let mut inlines = Vec::with_capacity(2);
    inlines.push(InlineNode::PlainText(Plain::new(
        &authored,
        Location::default(),
    )));
    let pointer = inlines.as_ptr();
    let mut button = Button::new(ContentAttribute::new(inlines), Location::default());
    assert_eq!(text(ButtonRef::from(&button).label())?, authored);
    let old = button
        .replace_label(ContentAttribute::default())
        .into_inlines();
    assert_eq!(old.as_ptr(), pointer);
    assert!(ButtonRef::from(&button).label().is_empty());
    let mut old = old;
    old.push(InlineNode::PlainText(Plain::new("!", Location::default())));
    assert_eq!(old.as_ptr(), pointer);
    button.replace_label(ContentAttribute::new(old));
    assert_eq!(
        text(ButtonRef::from(&button).label())?,
        format!("{authored}!")
    );
    Ok(())
}

#[test]
fn menu_target_and_item_transfers_preserve_empty_positions() -> Result<(), Error> {
    let mut items = Vec::with_capacity(4);
    items.extend([label("Open"), ContentAttribute::default(), label("Close")]);
    let pointer = items.as_ptr();
    let mut menu = Menu::new(label("File"), items, Location::default());
    assert_eq!(text(MenuRef::from(&menu).target())?, "File");
    assert_eq!(
        MenuRef::from(&menu)
            .items()
            .map(text)
            .collect::<Result<Vec<_>, _>>()?,
        ["Open", "", "Close"]
    );
    let target = menu.replace_target(ContentAttribute::default());
    assert_eq!(text(ContentRef::from(&target))?, "File");
    assert!(MenuRef::from(&menu).target().is_empty());
    menu.push_item(label("Save"));
    let items = menu.take_items();
    assert_eq!(items.as_ptr(), pointer);
    assert_eq!(items.len(), 4);
    assert_eq!(MenuRef::from(&menu).items().count(), 0);
    let empty = menu.replace_items(items);
    assert_eq!(empty, []);
    assert_eq!(
        MenuRef::from(&menu)
            .items()
            .map(text)
            .collect::<Result<Vec<_>, _>>()?,
        ["Open", "", "Close", "Save"]
    );
    Ok(())
}

#[test]
fn keyboard_transfers_prepared_keys_without_splitting() -> Result<(), Error> {
    let mut keys = Vec::with_capacity(4);
    keys.extend([label("Ctrl+Alt"), ContentAttribute::default(), label("É")]);
    let pointer = keys.as_ptr();
    let mut keyboard = Keyboard::new(keys, Location::default());
    keyboard.push_key(label("Enter"));
    let keys = keyboard.take_keys();
    assert_eq!(keys.as_ptr(), pointer);
    assert_eq!(KeyboardRef::from(&keyboard).keys().count(), 0);
    assert_eq!(keyboard.replace_keys(keys), []);
    assert_eq!(
        KeyboardRef::from(&keyboard)
            .keys()
            .map(text)
            .collect::<Result<Vec<_>, _>>()?,
        ["Ctrl+Alt", "", "É", "Enter"]
    );
    Ok(())
}

#[test]
fn selected_ui_copies_keep_originals_and_source_lifetimes() -> Result<(), Error> {
    let input = String::from("Shared *literal*");
    {
        let button = Button::new(label(&input), Location::default());
        let menu = Menu::new(label(&input), vec![label(&input)], Location::default());
        let keyboard = Keyboard::new(vec![label(&input)], Location::default());
        let mut copied_button = Button::from(ButtonRef::from(&button));
        let mut copied_menu = Menu::from(MenuRef::from(&menu));
        let mut copied_keyboard = Keyboard::from(KeyboardRef::from(&keyboard));
        assert_eq!(copied_button, button);
        assert_eq!(copied_menu, menu);
        assert_eq!(copied_keyboard, keyboard);
        let button_label = copied_button.replace_label(ContentAttribute::default());
        let menu_items = copied_menu.take_items();
        let keys = copied_keyboard.take_keys();
        assert_eq!(text(ButtonRef::from(&button).label())?, input);
        assert_eq!(MenuRef::from(&menu).items().count(), 1);
        assert_eq!(KeyboardRef::from(&keyboard).keys().count(), 1);
        assert_eq!(text(ContentRef::from(&button_label))?, input);
        assert_eq!(
            menu_items
                .iter()
                .map(ContentRef::from)
                .map(text)
                .collect::<Result<Vec<_>, _>>()?,
            [input.as_str()]
        );
        assert_eq!(
            keys.iter()
                .map(ContentRef::from)
                .map(text)
                .collect::<Result<Vec<_>, _>>()?,
            [input.as_str()]
        );
    }
    let (button, menu, keyboard) = {
        let mut button = Button::new(label(&input), Location::default());
        let mut menu = Menu::new(label(&input), vec![label(&input)], Location::default());
        let mut keyboard = Keyboard::new(vec![label(&input)], Location::default());
        (
            button.replace_label(ContentAttribute::default()),
            menu.take_items(),
            keyboard.take_keys(),
        )
    };
    assert_eq!(text(ContentRef::from(&button))?, input);
    assert_eq!(
        menu.iter()
            .map(ContentRef::from)
            .map(text)
            .collect::<Result<Vec<_>, _>>()?,
        [input.as_str()]
    );
    assert_eq!(
        keyboard
            .iter()
            .map(ContentRef::from)
            .map(text)
            .collect::<Result<Vec<_>, _>>()?,
        [input.as_str()]
    );
    Ok(())
}

#[test]
fn ui_span_edits_leave_child_locations_independent() -> Result<(), Error> {
    let source = "label";
    let child_location = Location::default();
    let mut location = Location::default();
    location.absolute_start = 42;
    location.absolute_end = 49;
    let mut button = Button::new(label(source), child_location.clone());
    let mut menu = Menu::new(label(source), vec![label(source)], child_location.clone());
    let mut keyboard = Keyboard::new(vec![label(source)], child_location.clone());
    button.set_location(location.clone());
    menu.set_location(location.clone());
    keyboard.set_location(location.clone());
    assert_eq!(
        Location::from(ButtonRef::from(&button).location()),
        location
    );
    assert_eq!(Location::from(MenuRef::from(&menu).location()), location);
    assert_eq!(
        Location::from(KeyboardRef::from(&keyboard).location()),
        location
    );
    assert_eq!(
        Location::from(
            ButtonRef::from(&button)
                .label()
                .inlines()
                .next()
                .ok_or("label")?
                .location()
        ),
        child_location
    );
    assert_eq!(
        Location::from(
            MenuRef::from(&menu)
                .target()
                .inlines()
                .next()
                .ok_or("target")?
                .location()
        ),
        child_location
    );
    assert_eq!(
        Location::from(
            KeyboardRef::from(&keyboard)
                .keys()
                .next()
                .ok_or("key")?
                .inlines()
                .next()
                .ok_or("key content")?
                .location()
        ),
        child_location
    );
    Ok(())
}

#[rstest::rstest]
fn parsed_ui_copies_preserve_native_output_and_nested_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("experimental", "")
        .build()?;
    let parsed = acdc_parser::parse_inline(
        "btn:[*Save*] kbd:[*Ctrl*+Enter] menu:*File*[_Open_ > Close]",
        &options,
    )?;
    let before = serde_json::to_value(parsed.inlines())?;
    let mut kinds = Vec::new();
    for (index, node) in parsed.content().inlines().enumerate() {
        let owned = if let InlineKind::Button(button) = node.kind() {
            kinds.push("button");
            InlineNode::Macro(InlineMacro::Button(Button::from(button)))
        } else if let InlineKind::Keyboard(keyboard) = node.kind() {
            kinds.push("keyboard");
            InlineNode::Macro(InlineMacro::Keyboard(Keyboard::from(keyboard)))
        } else if let InlineKind::Menu(menu) = node.kind() {
            kinds.push("menu");
            InlineNode::Macro(InlineMacro::Menu(Menu::from(menu)))
        } else {
            continue;
        };
        assert_eq!(
            before.pointer(&format!("/{index}")),
            Some(&serde_json::to_value(owned)?)
        );
    }
    assert_eq!(kinds, ["button", "keyboard", "menu"]);
    assert_eq!(serde_json::to_value(parsed.inlines())?, before);
    Ok(())
}

#[test]
fn copying_empty_macro_owners_allocates_nothing() {
    let button = Button::new(ContentAttribute::default(), Location::default());
    let menu = Menu::new(ContentAttribute::default(), Vec::new(), Location::default());
    let keyboard = Keyboard::new(Vec::new(), Location::default());
    let xref = acdc_parser::CrossReference::new("target", Location::default());
    let index = acdc_parser::IndexTerm::new(
        acdc_parser::IndexTermKind::Flow(Vec::new()),
        Location::default(),
    );
    let region = Region::new(ALLOCATOR);
    let copies = std::hint::black_box((
        Button::from(ButtonRef::from(&button)),
        Menu::from(MenuRef::from(&menu)),
        Keyboard::from(KeyboardRef::from(&keyboard)),
        acdc_parser::CrossReference::from(acdc_parser::ast::CrossReferenceRef::from(&xref)),
        acdc_parser::IndexTerm::from(acdc_parser::ast::IndexTermRef::from(&index)),
    ));
    let allocations = region.change();
    assert_eq!(allocations.allocations, 0);
    assert_eq!(copies.0, button);
    assert_eq!(copies.1, menu);
    assert_eq!(copies.2, keyboard);
    assert_eq!(copies.3, xref);
    assert_eq!(copies.4, index);
}

#[test]
fn remaining_control_replacement_is_checked_and_atomic() -> Result<(), Error> {
    use acdc_parser::{
        AttributeValue, CrossReference, ElementAttributes, IndexTerm, IndexTermKind, InlineMetadata,
    };
    macro_rules! check {
        ($owner:expr, $reserved:expr, $extensions:expr) => {{
            let mut node = $owner;
            let mut metadata = InlineMetadata::default();
            metadata.roles.push("role");
            node.set_metadata(metadata);
            node.push_option("option");
            node.set_attribute("kept".into(), "value".into())?;
            for name in $reserved {
                let before = node.clone();
                let mut attributes = ElementAttributes::default();
                attributes.set("new".into(), "unapplied".into());
                attributes.set(name.into(), "shadow".into());
                assert_eq!(
                    node.set_attributes(attributes)
                        .err()
                        .ok_or("reserved replacement")?
                        .name,
                    name
                );
                assert_eq!(node, before);
                assert_eq!(
                    node.set_attribute(name.into(), "shadow".into())
                        .err()
                        .ok_or("reserved set")?
                        .name,
                    name
                );
                assert_eq!(
                    node.remove_attribute(name)
                        .err()
                        .ok_or("reserved removal")?
                        .name,
                    name
                );
                assert_eq!(node, before);
            }
            let mut attributes = ElementAttributes::default();
            attributes.set("false".into(), AttributeValue::Bool(false));
            attributes.set("valueless".into(), AttributeValue::None);
            for name in $extensions {
                attributes.set(name.into(), "".into());
            }
            node.set_attributes(attributes)?;
            assert!(!node.clone_attributes().contains_key("kept"));
            assert_eq!(
                node.clone_attributes().get("false"),
                Some(&AttributeValue::Bool(false))
            );
            let controls = node.take_attributes();
            assert_eq!(controls.get("valueless"), Some(&AttributeValue::None));
            assert!(node.clone_attributes().is_empty());
            assert_eq!(node.clone_metadata().roles, ["role"]);
            assert_eq!(node.take_options(), ["option"]);
            assert_eq!(node.take_metadata().roles, ["role"]);
            assert_eq!(node.clone_metadata(), InlineMetadata::default());
        }};
    }
    check!(
        CrossReference::new("target", Location::default()),
        ["xrefstyle", "id", "role", "roles", "opts", "options"],
        ["title", "reftext", "caption_label"]
    );
    check!(
        IndexTerm::new(IndexTermKind::Flow(Vec::new()), Location::default()),
        [
            "terms", "see", "see-also", "id", "role", "roles", "opts", "options"
        ],
        ["kind", "relationship", "catalog"]
    );
    Ok(())
}

#[test]
fn xref_transfers_keep_buffers_source_lifetimes_and_display_snapshots() -> Result<(), Error> {
    use acdc_parser::{CrossReference, XrefCaptionLabel, XrefStyle, ast::CrossReferenceRef};
    let authored = String::from("*label* {entry} footnote:[literal]");
    let destination = String::from("chapter.adoc");
    let (content, options, target) = {
        let mut node = CrossReference::new(&destination, Location::default());
        assert!(!CrossReferenceRef::from(&node).target_is_local());
        node.set_target_is_local(true);
        let mut content = Vec::with_capacity(2);
        content.push(InlineNode::PlainText(Plain::new(
            &authored,
            Location::default(),
        )));
        let pointer = content.as_ptr();
        assert_eq!(node.replace_text(content), []);
        node.push_inline(InlineNode::PlainText(Plain::new("!", Location::default())));
        let content = node.take_text();
        assert_eq!(content.as_ptr(), pointer);
        assert!(CrossReferenceRef::from(&node).text().is_empty());
        let mut options = Vec::with_capacity(2);
        options.push("first");
        let pointer = options.as_ptr();
        assert_eq!(node.replace_options(options), [] as [&str; 0]);
        node.push_option("");
        let options = node.take_options();
        assert_eq!(options.as_ptr(), pointer);
        node.set_xrefstyle(XrefStyle::Short);
        node.set_caption_label(XrefCaptionLabel::AtReference(""));
        let copied = CrossReference::from(CrossReferenceRef::from(&node));
        assert_eq!(copied, node);
        let target = node.replace_target("other.adoc");
        assert_eq!(CrossReferenceRef::from(&node).target(), "other.adoc");
        assert!(CrossReferenceRef::from(&node).target_is_local());
        assert_eq!(CrossReferenceRef::from(&node).xrefstyle(), XrefStyle::Short);
        assert_eq!(
            CrossReferenceRef::from(&node).caption_label(),
            XrefCaptionLabel::AtReference("")
        );
        node.set_target_is_local(false);
        assert!(!CrossReferenceRef::from(&node).target_is_local());
        (content, options, target)
    };
    assert_eq!(target, destination);
    assert_eq!(options, ["first", ""]);
    assert_eq!(
        text(ContentRef::from(&ContentAttribute::new(content)))?,
        format!("{authored}!")
    );
    Ok(())
}

#[test]
fn index_transfers_keep_empty_hierarchy_relationship_and_source_values() -> Result<(), Error> {
    use acdc_parser::{IndexTerm, IndexTermKind, IndexTermRelationship, ast::IndexTermRef};
    let authored = String::from("*Term* {entry}");
    let (kind, relationship) = {
        let term = vec![InlineNode::PlainText(Plain::new(
            &authored,
            Location::default(),
        ))];
        let pointer = term.as_ptr();
        let mut node = IndexTerm::new(IndexTermKind::Flow(term), Location::default());
        let kind = node.replace_kind(IndexTermKind::Concealed {
            term: Vec::new(),
            secondary: Some(Vec::new()),
            tertiary: None,
        });
        if let IndexTermKind::Flow(term) = &kind {
            assert_eq!(term.as_ptr(), pointer);
        } else {
            return Err("flow transfer".into());
        }
        let read = IndexTermRef::from(&node);
        assert!(!read.is_visible());
        assert!(read.term().is_empty());
        assert!(
            read.secondary()
                .ok_or("present empty secondary")?
                .is_empty()
        );
        assert!(read.tertiary().is_none());
        let target = vec![InlineNode::PlainText(Plain::new(
            &authored,
            Location::default(),
        ))];
        let pointer = target.as_ptr();
        assert!(
            node.replace_relationship(Some(IndexTermRelationship::See { target }))
                .is_none()
        );
        let relationship = node.take_relationship().ok_or("relationship")?;
        if let IndexTermRelationship::See { target } = &relationship {
            assert_eq!(target.as_ptr(), pointer);
        } else {
            return Err("see transfer".into());
        }
        assert!(IndexTermRef::from(&node).relationship().is_none());
        (kind, relationship)
    };
    if let IndexTermKind::Flow(term) = kind {
        assert_eq!(
            text(ContentRef::from(&ContentAttribute::new(term)))?,
            authored
        );
    } else {
        return Err("flow".into());
    }
    if let IndexTermRelationship::See { target } = relationship {
        assert_eq!(
            text(ContentRef::from(&ContentAttribute::new(target)))?,
            authored
        );
    } else {
        return Err("see".into());
    }
    Ok(())
}

#[rstest::rstest]
fn selected_index_copies_keep_registered_labels_after_display_edits(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{
        IndexTerm, IndexTermKind,
        ast::{BlockKind, IndexTermRef},
    };
    let parsed = acdc_parser::parse(
        include_str!("../fixtures/tests/subs_index_control_snapshots.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut snapshots = 0;
    for block in parsed.document().blocks() {
        let BlockKind::Paragraph(paragraph) = block.kind() else {
            continue;
        };
        for inline in paragraph.content().inlines() {
            let InlineKind::IndexTerm(term) = inline.kind() else {
                continue;
            };
            let catalog = ContentAttribute::from(term.catalog().term());
            let displayed = ContentAttribute::from(term.term());
            let mut copied = IndexTerm::from(term);
            assert_eq!(
                ContentAttribute::from(IndexTermRef::from(&copied).term()),
                displayed
            );
            assert_eq!(
                ContentAttribute::from(IndexTermRef::from(&copied).catalog().term()),
                catalog
            );
            if displayed != catalog {
                snapshots += 1;
                copied.replace_kind(IndexTermKind::Flow(vec![InlineNode::PlainText(
                    Plain::new("Edited", Location::default()),
                )]));
                assert_eq!(
                    ContentAttribute::from(IndexTermRef::from(&copied).catalog().term()),
                    catalog
                );
            }
        }
    }
    assert!(snapshots > 0);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}
