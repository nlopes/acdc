//! Prepared image and icon edits keep content separate from checked macro controls.

use std::{alloc::System, hint::black_box};

use acdc_parser::{
    AttributeValue, CompatibilityMode, ContentAttribute, ElementAttributes, ElementId, Icon,
    InlineImage, InlineMacro, InlineMetadata, InlineNode, Location, Options, Plain, Position,
    Source,
    ast::{
        AttributeValueRef, BlockKind, ContentRef, IconRef, InlineImageRef, InlineKind, SourceKind,
    },
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

macro_rules! all_media {
    ($check:ident) => {
        $check!(InlineImage, InlineImageRef, Image, replace_source, source);
        $check!(Icon, IconRef, Icon, replace_target, target);
    };
}

fn plain(text: &str) -> InlineNode<'_> {
    InlineNode::PlainText(Plain::new(text, Location::default()).with_escaped(true))
}

#[test]
fn media_labels_transfer_without_formatting_or_losing_empty_presence() -> Result<(), Error> {
    let source = String::from("{unexpanded} *prepared*");
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident, $replace:ident, $source:ident) => {
            let nodes = vec![plain(&source)];
            let pointer = nodes.as_ptr();
            let (alt, tooltip) = {
                let mut node = $owner::new(Source::Name("target"), Location::default());
                assert!($read::from(&node).alt().is_none());
                assert!($read::from(&node).tooltip().is_none());
                node.set_alt(Some(ContentAttribute::new(nodes)));
                node.set_tooltip(Some(ContentAttribute::default()));
                assert!(
                    $read::from(&node)
                        .tooltip()
                        .is_some_and(ContentRef::is_empty)
                );
                (node.take_alt(), node.take_tooltip())
            };
            let alt = alt.ok_or("alt")?.into_inlines();
            assert_eq!(alt.as_ptr(), pointer);
            assert_eq!(alt, [plain(&source)]);
            assert_eq!(tooltip, Some(ContentAttribute::default()));
        };
    }
    all_media!(check);
    Ok(())
}

#[test]
fn media_bulk_controls_are_checked_atomically_with_all_reserved_aliases() -> Result<(), Error> {
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident, $replace:ident, $source:ident) => {
            let mut node = $owner::new(Source::Name("target"), Location::default());
            node.set_attribute("width".into(), AttributeValue::from("12"))?;
            node.set_alt(Some(ContentAttribute::default()));
            node.set_tooltip(Some(ContentAttribute::new(vec![plain("Tooltip")])));
            node.replace_options(vec!["first", "first", ""]);
            let mut metadata = InlineMetadata::default();
            metadata.id = Some(ElementId::new("id", Location::default()));
            metadata.roles = vec!["one", "two"];
            node.set_metadata(metadata);
            for key in ["alt", "title", "id", "role", "roles", "opts", "options"] {
                let before = node.clone();
                let mut attributes = ElementAttributes::default();
                attributes.set("height".into(), AttributeValue::from("20"));
                attributes.set(key.into(), AttributeValue::from("invalid"));
                assert_eq!(
                    node.set_attributes(attributes)
                        .err()
                        .map(|error| error.name),
                    Some(key.to_owned())
                );
                assert_eq!(node, before);
                assert!(
                    node.set_attribute(key.into(), AttributeValue::None)
                        .is_err()
                );
                assert_eq!(node, before);
                assert!(node.remove_attribute(key).is_err());
                assert_eq!(node, before);
            }
            let mut attributes = ElementAttributes::default();
            attributes.set("extension".into(), AttributeValue::None);
            attributes.set("empty".into(), AttributeValue::from(""));
            attributes.set("disabled".into(), AttributeValue::Bool(false));
            node.set_attributes(attributes.clone())?;
            let read = $read::from(&node);
            assert_eq!(
                read.attributes().get("extension"),
                Some(AttributeValueRef::None)
            );
            assert_eq!(
                read.attributes().get("empty"),
                Some(AttributeValueRef::Text(""))
            );
            assert_eq!(
                read.attributes().get("disabled"),
                Some(AttributeValueRef::Bool(false))
            );
            assert!(read.attributes().get("width").is_none());
            assert_eq!(node.clone_attributes(), attributes);
            assert_eq!(node.take_attributes(), attributes);
            assert!(node.clone_attributes().is_empty());
            assert!($read::from(&node).alt().is_some());
            assert_eq!(
                $read::from(&node).options().iter().collect::<Vec<_>>(),
                ["first", "first", ""]
            );
        };
    }
    all_media!(check);
    Ok(())
}

#[test]
fn media_option_and_metadata_transfers_keep_buffers_and_source_lifetimes() {
    let source = String::from("original");
    let replacement = String::from("replacement");
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident, $replace:ident, $source:ident) => {
            let options = vec![source.as_str(), "", source.as_str()];
            let pointer = options.as_ptr();
            let roles = vec![replacement.as_str()];
            let roles_pointer = roles.as_ptr();
            let (old, roles) = {
                let mut node = $owner::new(Source::Name("target"), Location::default());
                node.replace_options(options);
                let old = node.replace_options(vec![replacement.as_str()]);
                node.push_option("");
                assert_eq!(node.take_options(), [replacement.as_str(), ""]);
                assert!($read::from(&node).options().is_empty());
                let mut metadata = InlineMetadata::default();
                metadata.roles = roles;
                node.set_metadata(metadata);
                let copy = node.clone_metadata();
                assert_eq!(copy.roles, [replacement.as_str()]);
                let taken = node.take_metadata();
                assert!($read::from(&node).roles().is_empty());
                (old, taken.roles)
            };
            assert_eq!(old.as_ptr(), pointer);
            assert_eq!(old, [source.as_str(), "", source.as_str()]);
            assert_eq!(roles.as_ptr(), roles_pointer);
            assert_eq!(roles, [replacement.as_str()]);
        };
    }
    all_media!(check);
}

#[test]
fn media_target_replacement_keeps_original_url_spelling_and_does_not_fetch() -> Result<(), Error> {
    let source = String::from("HTTPS://example.invalid/a?key=Value#part");
    // SourceUrl accepts the original spelling; prepared replacement does not classify it again.
    let url = acdc_parser::SourceUrl::new(&source)?;
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident, $replace:ident, $source:ident) => {
            let old = {
                let mut node = $owner::new(Source::Name("old"), Location::default());
                assert_eq!(node.$replace(Source::Url(url.clone())), Source::Name("old"));
                assert_eq!(
                    $read::from(&node).$source().kind(),
                    SourceKind::Url(source.as_str())
                );
                node.$replace(Source::Name("new"))
            };
            assert_eq!(old, Source::Url(url.clone()));
            assert_eq!(old.to_string(), source);
        };
    }
    all_media!(check);
    Ok(())
}

#[test]
fn media_location_edits_keep_id_and_content_locations() -> Result<(), Error> {
    let original = Location::point(Position::new(2, 3));
    let id_location = Location::point(Position::new(1, 4));
    let replacement = Location::point(Position::new(8, 9));
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident, $replace:ident, $source:ident) => {
            let mut node = $owner::new(Source::Name("target"), original.clone());
            node.set_alt(Some(ContentAttribute::new(vec![InlineNode::PlainText(
                Plain::new("Alt", original.clone()),
            )])));
            let mut metadata = InlineMetadata::default();
            metadata.id = Some(ElementId::new("id", id_location.clone()));
            node.set_metadata(metadata);
            node.set_location(replacement.clone());
            let read = $read::from(&node);
            assert_eq!(Location::from(read.location()), replacement);
            assert_eq!(
                read.id().and_then(|id| id.location()).map(Location::from),
                Some(id_location.clone())
            );
            assert_eq!(
                Location::from(
                    read.alt()
                        .ok_or("alt")?
                        .inlines()
                        .next()
                        .ok_or("inline")?
                        .location()
                ),
                original
            );
        };
    }
    all_media!(check);
    Ok(())
}

#[rstest::rstest]
fn selected_media_copies_keep_native_json_and_do_not_register_edits(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "image:bird.svg[Bird,title=Tip,width=12,id=bird,role=one,opts=first,extension=value] icon:heart[2x,alt=Heart,title=Tip,id=heart,role=two,opts=second,extension=icon]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let BlockKind::Paragraph(paragraph) =
        parsed.document().blocks().next().ok_or("paragraph")?.kind()
    else {
        return Err("paragraph".into());
    };
    let mut kinds = Vec::new();
    for (index, inline) in paragraph.content().inlines().enumerate() {
        macro_rules! check {
            ($owner:ident, $read:ident, $variant:ident, $value:expr) => {{
                let original = $owner::from($value);
                let mut copy = $owner::from($value);
                assert_eq!(
                    &serde_json::to_value(InlineNode::Macro(InlineMacro::$variant(
                        copy.clone().into()
                    )))?,
                    before
                        .pointer(&format!("/blocks/0/inlines/{index}"))
                        .ok_or("native inline")?
                );
                copy.set_alt(Some(ContentAttribute::default()));
                copy.set_tooltip(None);
                drop(copy.take_metadata());
                drop(copy.take_options());
                drop(copy.take_attributes());
                assert_eq!($owner::from($value), original);
                kinds.push(stringify!($owner));
            }};
        }
        if let InlineKind::Image(image) = inline.kind() {
            check!(InlineImage, InlineImageRef, Image, image);
        } else if let InlineKind::Icon(icon) = inline.kind() {
            check!(Icon, IconRef, Icon, icon);
        }
    }
    assert_eq!(kinds, ["InlineImage", "Icon"]);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn selected_empty_named_media_copies_allocate_nothing() {
    let image = InlineImage::new(Source::Name("image"), Location::default());
    let icon = Icon::new(Source::Name("icon"), Location::default());
    let region = Region::new(ALLOCATOR);
    let copies = black_box((
        InlineImage::from(InlineImageRef::from(&image)),
        Icon::from(IconRef::from(&icon)),
    ));
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(copies.0, image);
    assert_eq!(copies.1, icon);
}

#[test]
fn scalar_control_transfers_keep_the_original_name_and_value_lifetimes() -> Result<(), Error> {
    let name = String::from("extension");
    let source = String::from("{unexpanded} *literal*");
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident, $replace:ident, $source:ident) => {
            let mut attributes = ElementAttributes::default();
            attributes.set(name.as_str().into(), AttributeValue::from(source.as_str()));
            let pointer = std::ptr::from_ref(attributes.get(&name).ok_or("prepared control")?);
            let transferred = {
                let mut node = $owner::new(Source::Name("target"), Location::default());
                node.set_attributes(attributes)?;
                node.take_attributes()
            };
            let value = transferred.get(&name).ok_or("transferred control")?;
            assert_eq!(std::ptr::from_ref(value), pointer);
            assert_eq!(value, &AttributeValue::from(source.as_str()));
            if let AttributeValue::String(text) = value {
                assert_eq!(text.as_ptr(), source.as_ptr());
            } else {
                return Err("expected borrowed scalar text".into());
            }
        };
    }
    all_media!(check);
    Ok(())
}
