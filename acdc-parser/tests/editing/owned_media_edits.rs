//! Media edits preserve prepared values, source lifetimes and content ownership.

type Error = Box<dyn std::error::Error>;

macro_rules! media_contract {
    ($module:ident, $owner:ident, $read:ident, $source:literal, $alt_reserved:literal) => {
        mod $module {
            use super::Error;
            use acdc_parser::{
                Block, CompatibilityMode, Location, Options, ParseResult, Source, Title,
                ast::{AttributeValueRef, ContentRef, $read},
                parse, $owner,
            };

            fn owner(parsed: &ParseResult) -> Result<$owner<'_>, Error> {
                let block = parsed.document().blocks().next().ok_or("media")?;
                let Block::$owner(owner) = Block::from(block) else {
                    return Err("media kind".into());
                };
                Ok(owner)
            }

            #[rstest::rstest]
            fn round_trips_keep_all_prepared_values_and_content_presence(
                #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)]
                mode: CompatibilityMode,
            ) -> Result<(), Error> {
                let parsed = parse(
                    $source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let mut edited = owner(&parsed)?;
                let original = edited.clone();
                let before = serde_json::to_value(&edited)?;
                let metadata = edited.take_metadata();
                let title = edited.take_title();
                let target = edited.replace_source(Source::Name("replacement"));
                assert!($read::from(&edited).title().is_none());
                edited.set_metadata(metadata)?;
                edited.set_title(title);
                edited.replace_source(target);
                assert_eq!(edited, original);
                assert_eq!(serde_json::to_value(&edited)?, before);
                Ok(())
            }

            #[rstest::rstest]
            fn owned_parts_outlive_the_old_owner_and_keep_prepared_values(
                #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)]
                mode: CompatibilityMode,
            ) -> Result<(), Error> {
                let parsed = parse(
                    $source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let mut original = owner(&parsed)?;
                let expected = $read::from(&original).source().to_string();
                let (target, metadata, title) = {
                    let mut local = owner(&parsed)?;
                    let target = local.replace_source(Source::Name("replacement"));
                    let metadata = local.take_metadata();
                    let title = local.take_title();
                    drop(local);
                    (target, metadata, title)
                };
                let mut rebuilt = $owner::new(target, Location::default());
                rebuilt.set_metadata(metadata)?;
                rebuilt.set_title(title);
                assert_eq!($read::from(&rebuilt).source().to_string(), expected);
                assert_eq!(rebuilt.clone_metadata(), original.clone_metadata());
                assert_eq!(rebuilt.take_title(), original.take_title());
                Ok(())
            }

            #[rstest::rstest]
            fn rejected_metadata_is_atomic_and_alt_reservation_is_contextual(
                #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)]
                mode: CompatibilityMode,
            ) -> Result<(), Error> {
                let parsed = parse(
                    $source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let mut edited = owner(&parsed)?;
                let original = edited.clone();
                for name in [
                    "title",
                    "id",
                    "roles",
                    "opts",
                    "style",
                    "caption",
                    "reftext",
                    "subs",
                    "attribution",
                    "citetitle",
                ] {
                    let mut metadata = edited.clone_metadata();
                    metadata.attributes.set(name.into(), "shadow".into());
                    assert!(edited.set_metadata(metadata).is_err(), "{name}");
                    assert_eq!(edited, original, "{name}");
                }
                let mut metadata = edited.clone_metadata();
                metadata.attributes.set("alt".into(), "extension".into());
                if $alt_reserved {
                    assert!(edited.set_metadata(metadata).is_err());
                    assert_eq!(edited, original);
                } else {
                    edited.set_metadata(metadata)?;
                    assert_eq!(
                        $read::from(&edited).attributes().get("alt"),
                        Some(AttributeValueRef::Text("extension"))
                    );
                }
                Ok(())
            }

            #[test]
            fn constructed_empty_titles_and_detached_metadata_remain_explicit() -> Result<(), Error>
            {
                let mut edited = $owner::new(Source::Name("target"), Location::default());
                assert!($read::from(&edited).title().is_none());
                edited.set_title(Title::new(Vec::new()));
                assert!(
                    $read::from(&edited)
                        .title()
                        .is_some_and(ContentRef::is_empty)
                );
                let mut metadata = edited.take_metadata();
                let value = String::from("owned control");
                let pointer = value.as_ptr();
                metadata.attributes.set("custom".into(), value.into());
                edited.set_metadata(metadata)?;
                let attributes = $read::from(&edited).attributes();
                let Some(AttributeValueRef::Text(value)) = attributes.get("custom") else {
                    return Err("custom control".into());
                };
                assert_eq!(value.as_ptr(), pointer);
                assert!(edited.take_title().is_present());
                assert!($read::from(&edited).title().is_none());
                edited.set_location(Location::default());
                Ok(())
            }
        }
    };
}

media_contract!(
    image,
    Image,
    ImageRef,
    "[#media.note]\n.A *title*\nimage::bird.svg[alt='*Bird*',width=120]\n",
    true
);
media_contract!(
    audio,
    Audio,
    AudioRef,
    "[#media.note]\n.A *title*\naudio::https://example.org/Voice.ogg[opts=loop]\n",
    false
);
media_contract!(
    video,
    Video,
    VideoRef,
    "[#media.note]\n.A *title*\nvideo::RvRhUHTV_8k,_SvwdK_HibQ,SGqg_ZzThDU[youtube,width=120]\n",
    false
);

#[rstest::rstest]
fn image_alt_and_caption_title_keep_distinct_absent_empty_and_prepared_content(
    #[values(
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft
    )]
    mode: acdc_parser::CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{
        Block, ContentAttribute, Location, Options, Plain,
        ast::{ContentRef, ImageRef},
        parse,
    };
    let parsed = parse(
        ".Caption\nimage::bird.svg[alt='*Bird*']",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Block::Image(mut image) = Block::from(parsed.document().blocks().next().ok_or("image")?)
    else {
        return Err("image".into());
    };
    let alt = image.take_alt().ok_or("formatted alt")?;
    assert!(ImageRef::from(&image).alt().is_none());
    assert!(ImageRef::from(&image).title().is_some());
    image.set_alt(Some(alt));
    image.set_alt(Some(ContentAttribute::default()));
    assert!(
        ImageRef::from(&image)
            .alt()
            .is_some_and(ContentRef::is_empty)
    );
    image.set_alt(Some(ContentAttribute::new(vec![
        acdc_parser::InlineNode::PlainText(Plain::new("*literal* {name}", Location::default())),
    ])));
    assert_eq!(
        serde_json::to_value(image.take_alt())?.pointer("/0/value"),
        Some(&serde_json::json!("*literal* {name}"))
    );
    assert!(ImageRef::from(&image).alt().is_none());
    Ok(())
}
