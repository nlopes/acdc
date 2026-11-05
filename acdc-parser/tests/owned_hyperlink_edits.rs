//! Prepared hyperlink edits preserve labels, raw query values and declaration snapshots.

use std::{alloc::System, hint::black_box};

use acdc_parser::{
    AttributeValue, Autolink, ContentAttribute, ElementAttributes, ElementId, InlineMacro,
    InlineMetadata, InlineNode, Link, Location, Mailto, Options, Plain, Position, Source, Url,
    ast::{
        AttributeValueRef, AutolinkRef, BlockKind, ContentRef, InlineKind, LinkRef, MailtoRef,
        SourceKind, UrlRef,
    },
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

macro_rules! all_links {
    ($check:ident) => {
        $check!(Link, LinkRef, Link);
        $check!(Url, UrlRef, Url);
        $check!(Mailto, MailtoRef, Mailto);
    };
}

fn plain(text: &str) -> InlineNode<'_> {
    InlineNode::PlainText(Plain::new(text, Location::default()).with_escaped(true))
}

#[test]
fn hyperlink_labels_transfer_prepared_buffers_without_parsing_or_registration() {
    let source = String::from("original");
    let replacement = String::from("footnote:new[{unexpanded} *literal*]");
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let original = vec![plain(&source)];
            let original_pointer = original.as_ptr();
            let mut next = Vec::with_capacity(2);
            next.push(plain(&replacement));
            let next_pointer = next.as_ptr();
            let (old, taken) = {
                let mut link = $owner::new(Source::Name("target"), Location::default());
                assert!($read::from(&link).text().is_empty());
                assert!(link.replace_text(original).is_empty());
                let old = link.replace_text(next);
                link.push_inline(plain("tail"));
                let taken = link.take_text();
                assert!($read::from(&link).text().is_empty());
                (old, taken)
            };
            assert_eq!(old.as_ptr(), original_pointer);
            assert_eq!(taken.as_ptr(), next_pointer);
            assert_eq!(old, [plain(&source)]);
            assert_eq!(taken, [plain(&replacement), plain("tail")]);
        };
    }
    all_links!(check);
}

#[test]
fn hyperlink_tooltips_keep_absence_empty_presence_and_original_source_lifetimes()
-> Result<(), Error> {
    let source = String::from("Tooltip");
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let nodes = vec![plain(&source)];
            let pointer = nodes.as_ptr();
            let taken = {
                let mut link = $owner::new(Source::Name("target"), Location::default());
                assert!($read::from(&link).tooltip().is_none());
                link.set_tooltip(Some(ContentAttribute::default()));
                assert!(
                    $read::from(&link)
                        .tooltip()
                        .is_some_and(ContentRef::is_empty)
                );
                assert_eq!(link.take_tooltip(), Some(ContentAttribute::default()));
                link.set_tooltip(Some(ContentAttribute::new(nodes)));
                link.take_tooltip().ok_or("tooltip")?.into_inlines()
            };
            assert_eq!(taken.as_ptr(), pointer);
            assert_eq!(taken, [plain(&source)]);
        };
    }
    all_links!(check);
    Ok(())
}

#[test]
fn hyperlink_scalar_replacement_rejects_reserved_aliases_atomically() -> Result<(), Error> {
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let mut link = $owner::new(Source::Name("target"), Location::default());
            link.replace_text(vec![plain("Label")]);
            link.set_tooltip(Some(ContentAttribute::default()));
            link.replace_options(vec!["first", "first", ""]);
            let mut metadata = InlineMetadata::default();
            metadata.id = Some(ElementId::new("id", Location::default()));
            metadata.roles = vec!["role"];
            link.set_metadata(metadata);
            link.set_attribute("window".into(), "_blank".into())?;
            for key in ["title", "id", "role", "roles", "opts", "options"] {
                let before = link.clone();
                let mut attributes = ElementAttributes::default();
                attributes.set("extension".into(), AttributeValue::from("new"));
                attributes.set(key.into(), AttributeValue::None);
                assert_eq!(
                    link.set_attributes(attributes)
                        .err()
                        .map(|error| error.name),
                    Some(key.to_owned())
                );
                assert_eq!(link, before);
                assert!(link.set_attribute(key.into(), "invalid".into()).is_err());
                assert_eq!(link, before);
                assert!(link.remove_attribute(key).is_err());
                assert_eq!(link, before);
            }
            let mut attributes = ElementAttributes::default();
            attributes.set("alt".into(), AttributeValue::from("extension"));
            attributes.set("subject".into(), AttributeValue::from("named extension"));
            attributes.set("body".into(), AttributeValue::None);
            attributes.set("disabled".into(), AttributeValue::Bool(false));
            link.set_attributes(attributes.clone())?;
            let read = $read::from(&link);
            assert_eq!(
                read.attributes().get("alt"),
                Some(AttributeValueRef::Text("extension"))
            );
            assert_eq!(
                read.attributes().get("subject"),
                Some(AttributeValueRef::Text("named extension"))
            );
            assert_eq!(read.attributes().get("body"), Some(AttributeValueRef::None));
            assert_eq!(
                read.attributes().get("disabled"),
                Some(AttributeValueRef::Bool(false))
            );
            assert_eq!(link.clone_attributes(), attributes);
            assert_eq!(link.take_attributes(), attributes);
            assert!(link.clone_attributes().is_empty());
            assert!($read::from(&link).tooltip().is_some());
        };
    }
    all_links!(check);
    Ok(())
}

#[test]
fn hyperlink_metadata_options_and_scalar_transfers_outlive_the_previous_owner() -> Result<(), Error>
{
    let source = String::from("original");
    let replacement = String::from("replacement");
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let options = vec![source.as_str(), "", source.as_str()];
            let pointer = options.as_ptr();
            let roles = vec![replacement.as_str()];
            let roles_pointer = roles.as_ptr();
            let (old, roles, scalar) = {
                let mut link = $owner::new(Source::Name("target"), Location::default());
                link.replace_options(options);
                let old = link.replace_options(vec![replacement.as_str()]);
                link.push_option("");
                assert_eq!(link.take_options(), [replacement.as_str(), ""]);
                assert!($read::from(&link).options().is_empty());
                let mut metadata = InlineMetadata::default();
                metadata.roles = roles;
                link.set_metadata(metadata);
                assert_eq!(link.clone_metadata().roles, [replacement.as_str()]);
                let roles = link.take_metadata().roles;
                assert!($read::from(&link).roles().is_empty());
                link.set_attribute(
                    source.as_str().into(),
                    AttributeValue::from(replacement.as_str()),
                )?;
                (old, roles, link.take_attributes())
            };
            assert_eq!(old.as_ptr(), pointer);
            assert_eq!(old, [source.as_str(), "", source.as_str()]);
            assert_eq!(roles.as_ptr(), roles_pointer);
            assert_eq!(roles, [replacement.as_str()]);
            assert_eq!(
                scalar.get(&source),
                Some(&AttributeValue::from(replacement.as_str()))
            );
        };
    }
    all_links!(check);
    Ok(())
}

#[test]
fn hyperlink_target_and_location_edits_preserve_other_prepared_values() -> Result<(), Error> {
    let source = String::from("HTTPS://example.invalid/path?key=Value#part");
    let target = Source::Url(acdc_parser::SourceUrl::new(&source)?);
    let original = Location::point(Position::new(2, 3));
    let replacement = Location::point(Position::new(8, 9));
    let id_location = Location::point(Position::new(1, 4));
    macro_rules! check {
        ($owner:ident, $read:ident, $variant:ident) => {
            let taken = {
                let mut link = $owner::new(Source::Name("original"), original.clone());
                link.replace_text(vec![InlineNode::PlainText(Plain::new(
                    "Label",
                    original.clone(),
                ))]);
                let mut metadata = InlineMetadata::default();
                metadata.id = Some(ElementId::new("id", id_location.clone()));
                link.set_metadata(metadata);
                assert_eq!(
                    link.replace_target(target.clone()),
                    Source::Name("original")
                );
                link.set_location(replacement.clone());
                let read = $read::from(&link);
                assert_eq!(read.target().kind(), SourceKind::Url(source.as_str()));
                assert_eq!(Location::from(read.location()), replacement);
                assert_eq!(
                    read.id().and_then(|id| id.location()).map(Location::from),
                    Some(id_location.clone())
                );
                assert_eq!(
                    Location::from(read.text().inlines().next().ok_or("label")?.location()),
                    original
                );
                link.replace_target(Source::Name("new"))
            };
            assert_eq!(taken, target);
            assert_eq!(taken.to_string(), source);
        };
    }
    all_links!(check);
    Ok(())
}

#[test]
fn mail_query_edits_keep_absence_empty_and_unencoded_values() -> Result<(), Error> {
    let source = String::from("a&b=%20+λ\nnext");
    for initial in [None, Some(""), Some(source.as_str())] {
        let (subject, body) = {
            let mut mail = Mailto::new(Source::Name("reader@example.invalid"), Location::default());
            assert_eq!(mail.replace_subject(initial), None);
            assert_eq!(mail.replace_body(initial), None);
            assert_eq!(MailtoRef::from(&mail).subject(), initial);
            assert_eq!(MailtoRef::from(&mail).body(), initial);
            mail.set_attribute("subject".into(), "extension".into())?;
            mail.set_attribute("body".into(), "extension".into())?;
            assert_eq!(MailtoRef::from(&mail).subject(), initial);
            assert_eq!(MailtoRef::from(&mail).body(), initial);
            (mail.replace_subject(None), mail.replace_body(None))
        };
        assert_eq!(subject, initial);
        assert_eq!(body, initial);
    }
    Ok(())
}

#[test]
fn prepared_autolink_edits_keep_source_flags_and_location() {
    let source = String::from("target");
    let location = Location::point(Position::new(4, 7));
    let taken = {
        let mut link = Autolink::new(Source::Name(&source), Location::default());
        assert!(!AutolinkRef::from(&link).is_bracketed());
        assert!(!AutolinkRef::from(&link).hides_uri_scheme());
        link.set_bracketed(true);
        link.set_hide_uri_scheme(true);
        link.set_location(location.clone());
        let copy = Autolink::from(AutolinkRef::from(&link));
        assert_eq!(copy, link);
        let old = link.replace_target(Source::Name("new"));
        assert!(AutolinkRef::from(&link).is_bracketed());
        assert!(AutolinkRef::from(&link).hides_uri_scheme());
        assert_eq!(
            Location::from(AutolinkRef::from(&link).location()),
            location
        );
        link.set_bracketed(false);
        link.set_hide_uri_scheme(false);
        assert!(!AutolinkRef::from(&link).is_bracketed());
        assert!(!AutolinkRef::from(&link).hides_uri_scheme());
        old
    };
    assert_eq!(taken, Source::Name(source.as_str()));
}

#[rstest::rstest]
fn selected_hyperlink_copies_preserve_native_json_and_declaration_snapshots(
    #[values(
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft
    )]
    mode: acdc_parser::CompatibilityMode,
) -> Result<(), Error> {
    let source = ":hide-uri-scheme:\n\nlink:https://example.org[Link,title=Tip,id=link,role=one,opts=first,extension=value] https://example.org/path[URL,title=Tip,id=url] mailto:reader@example.org[Email,Subject,Body,title=Tip] https://example.org/plain\n\n:!hide-uri-scheme:\n\nlink:https://example.org[Link] https://example.org/path[URL] https://example.org/plain\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut flags = Vec::new();
    let mut kinds = Vec::new();
    for (block_index, block) in parsed.document().blocks().enumerate() {
        if let BlockKind::Paragraph(paragraph) = block.kind() {
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
                                .pointer(&format!("/blocks/{block_index}/inlines/{index}"))
                                .ok_or("native link")?
                        );
                        copy.take_text();
                        drop(copy.take_tooltip());
                        drop(copy.take_metadata());
                        drop(copy.take_attributes());
                        copy.take_options();
                        assert_eq!($owner::from($value), original);
                        kinds.push(stringify!($owner));
                    }};
                }
                if let InlineKind::Link(link) = inline.kind() {
                    let mut copy = Link::from(link);
                    flags.push(link.hides_uri_scheme());
                    copy.replace_target(Source::Name("new"));
                    assert_eq!(
                        LinkRef::from(&copy).hides_uri_scheme(),
                        link.hides_uri_scheme()
                    );
                    copy.set_hide_uri_scheme(!link.hides_uri_scheme());
                    assert_eq!(
                        LinkRef::from(&copy).hides_uri_scheme(),
                        !link.hides_uri_scheme()
                    );
                    check!(Link, LinkRef, Link, link);
                } else if let InlineKind::Url(url) = inline.kind() {
                    let mut copy = Url::from(url);
                    flags.push(url.hides_uri_scheme());
                    copy.replace_target(Source::Name("new"));
                    assert_eq!(
                        UrlRef::from(&copy).hides_uri_scheme(),
                        url.hides_uri_scheme()
                    );
                    copy.set_hide_uri_scheme(!url.hides_uri_scheme());
                    assert_eq!(
                        UrlRef::from(&copy).hides_uri_scheme(),
                        !url.hides_uri_scheme()
                    );
                    check!(Url, UrlRef, Url, url);
                } else if let InlineKind::Mailto(mail) = inline.kind() {
                    check!(Mailto, MailtoRef, Mailto, mail);
                } else if let InlineKind::Autolink(link) = inline.kind() {
                    let mut copy = Autolink::from(link);
                    let original = copy.clone();
                    flags.push(link.hides_uri_scheme());
                    copy.replace_target(Source::Name("new"));
                    assert_eq!(
                        AutolinkRef::from(&copy).hides_uri_scheme(),
                        link.hides_uri_scheme()
                    );
                    assert_eq!(Autolink::from(link), original);
                    kinds.push("Autolink");
                }
            }
        }
    }
    assert_eq!(flags, [true, true, true, false, false, false]);
    assert_eq!(
        kinds,
        [
            "Link", "Url", "Mailto", "Autolink", "Link", "Url", "Autolink"
        ]
    );
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn selected_empty_named_hyperlink_copies_allocate_nothing() {
    let link = Link::new(Source::Name("link"), Location::default());
    let url = Url::new(Source::Name("url"), Location::default());
    let mail = Mailto::new(Source::Name("mail"), Location::default());
    let auto = Autolink::new(Source::Name("auto"), Location::default());
    let region = Region::new(ALLOCATOR);
    let copies = black_box((
        Link::from(LinkRef::from(&link)),
        Url::from(UrlRef::from(&url)),
        Mailto::from(MailtoRef::from(&mail)),
        Autolink::from(AutolinkRef::from(&auto)),
    ));
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(copies.0, link);
    assert_eq!(copies.1, url);
    assert_eq!(copies.2, mail);
    assert_eq!(copies.3, auto);
}
