//! Alternative text retains content, fallback, and ownership rules across input forms.

use std::{borrow::Cow, path::PathBuf};

use acdc_converters_core::{icon, media};
use acdc_parser::{
    ContentAttribute, InlineNode, Location, Options, Raw, Source, SourceUrl, Substitution,
    ast::{ContentRef, SourceRef},
    parse_inline,
};

type TestError = Box<dyn std::error::Error>;

#[test]
fn image_and_icon_fallbacks_keep_distinct_target_policies() -> Result<(), TestError> {
    for (source, image, icon) in [
        (Source::Name("heart"), "heart", "heart"),
        (Source::Name("arrow_left"), "arrow left", "arrow left"),
        (Source::Name("folder/name.svg"), "name", "folder/name.svg"),
        (
            Source::Path(PathBuf::from("images/my_photo.jpeg")),
            "my photo",
            "images/my photo.jpeg",
        ),
        (
            Source::Url(SourceUrl::new(
                "https://example.test/a/../my_photo.svg?download=1#part",
            )?),
            "my photo",
            "https://example.test/a/../my photo.svg?download=1#part",
        ),
        (
            Source::Url(SourceUrl::new("https://example.test/a%20b_c.png?x=1#frag")?),
            "a%20b c",
            "https://example.test/a%20b c.png?x=1#frag",
        ),
        (
            Source::Url(SourceUrl::new("https://example.test/")?),
            "",
            "https://example.test/",
        ),
        (
            Source::Url(SourceUrl::new("mailto:team@example.test")?),
            "",
            "mailto:team@example.test",
        ),
    ] {
        assert_eq!(media::image_alt(&source, None), image);
        assert_eq!(icon::alt(&source, None), icon);
        let target = SourceRef::from(&source);
        assert_eq!(media::image_alt_from_content(target, None), image);
        assert_eq!(icon::alt_from_content(target, None), icon);
        assert_eq!(target.file_name(), source.get_filename());
        if let Some(filename) = target.file_name() {
            assert_eq!(
                Some(filename.as_ptr()),
                source.get_filename().map(str::as_ptr)
            );
        }
    }
    Ok(())
}

#[test]
fn explicit_and_empty_alternatives_outlive_temporary_sources() -> Result<(), TestError> {
    for (markup, expected) in [
        ("", ""),
        ("*Bird* &amp; ->", "Bird & →"),
        ("*Bird* &#38; ->", "Bird & →"),
    ] {
        let (image, icon, typed_image, typed_icon) = {
            let text = String::from(markup);
            let parsed = (!text.is_empty())
                .then(|| parse_inline(&text, &Options::default()))
                .transpose()?;
            let alt = parsed
                .as_ref()
                .map_or_else(ContentAttribute::default, |parsed| {
                    ContentAttribute::new(parsed.inlines().to_vec())
                });
            let target = String::from("fallback-name");
            let source = Source::Name(&target);
            (
                media::image_alt(&source, Some(&alt)),
                icon::alt(&source, Some(&alt)).into_owned(),
                media::image_alt_from_content(
                    SourceRef::from(&source),
                    Some(ContentRef::from(&alt)),
                ),
                icon::alt_from_content(SourceRef::from(&source), Some(ContentRef::from(&alt)))
                    .into_owned(),
            )
        };
        assert!(matches!(image, Cow::Owned(_)));
        assert_eq!(image, expected);
        assert_eq!(icon, expected);
        assert_eq!(typed_image, expected);
        assert_eq!(typed_icon, expected);
    }
    Ok(())
}

#[test]
fn owner_icon_fallback_borrows_the_name_beyond_the_source_wrapper() {
    let name = String::from("heart");
    let alt = {
        let source = Source::Name(&name);
        icon::alt(&source, None)
    };
    assert!(matches!(alt, Cow::Borrowed(_)));
    assert_eq!(alt.as_ptr(), name.as_ptr());
}

#[test]
fn typed_icon_alternatives_do_not_borrow_temporary_content() -> Result<(), TestError> {
    let name = String::from("heart");
    let source = Source::Name(&name);
    let target = SourceRef::from(&source);
    let fallback = icon::alt_from_content(target, None);
    assert!(matches!(fallback, Cow::Borrowed(_)));
    assert_eq!(fallback.as_ptr(), name.as_ptr());
    let alt = {
        let markup = String::from("*Bird* ->");
        let parsed = parse_inline(&markup, &Options::default())?;
        let content = ContentAttribute::new(parsed.inlines().to_vec());
        icon::alt_from_content(target, Some(ContentRef::from(&content)))
    };
    assert!(matches!(alt, Cow::Owned(_)));
    assert_eq!(alt, "Bird →");
    Ok(())
}

#[test]
fn raw_alternatives_decode_only_unescaped_numeric_references() {
    let source = Source::Name("fallback");
    let target = SourceRef::from(&source);
    for (subs, expected) in [
        (vec![], "Bird &"),
        (vec![Substitution::SpecialChars], "Bird &#38;"),
    ] {
        let alt = ContentAttribute::new(vec![InlineNode::RawText(Raw {
            content: "Bird &#38;",
            location: Location::default(),
            subs,
        })]);
        let content = Some(ContentRef::from(&alt));
        assert_eq!(media::image_alt(&source, Some(&alt)), expected);
        assert_eq!(media::image_alt_from_content(target, content), expected);
        assert_eq!(icon::alt(&source, Some(&alt)), expected);
        assert_eq!(icon::alt_from_content(target, content), expected);
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_keep_an_empty_image_alternative() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let source = Source::Path(PathBuf::from(OsString::from_vec(b"bad\xff.png".to_vec())));
    assert_eq!(media::image_alt(&source, None), "");
    assert_eq!(icon::alt(&source, None), "bad�.png");
    let target = SourceRef::from(&source);
    assert_eq!(target.file_name(), None);
    assert_eq!(media::image_alt_from_content(target, None), "");
    assert_eq!(icon::alt_from_content(target, None), "bad�.png");
}

#[test]
fn image_alternatives_preserve_parser_selected_boundaries() -> Result<(), TestError> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse_inline(
            "image:bird.png[alt=\"(C) -> --\",title=\"(C) -> --\"]",
            &options,
        )?;
        let [InlineNode::Macro(acdc_parser::InlineMacro::Image(image))] = parsed.inlines() else {
            return Err("expected inline image".into());
        };
        let alt = image.alt.as_ref().ok_or("missing alt")?;
        let tooltip = image.tooltip.as_ref().ok_or("missing tooltip")?;
        let expected = "© → --";
        assert_eq!(media::image_alt(&image.source, Some(alt)), expected);
        assert_eq!(
            media::image_alt_from_content((&image.source).into(), Some(alt.into())),
            expected
        );
        assert_eq!(
            acdc_converters_core::inline_text::InlineTextTransform::default()
                .rendered()
                .decode_char_refs(true)
                .content_to_string(tooltip.into()),
            expected
        );
    }
    Ok(())
}

#[test]
fn icon_alternatives_preserve_parser_selected_boundaries() -> Result<(), TestError> {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let parsed = parse_inline(
            "icon:heart[alt=\"(C) -> --\",title=\"(C) -> --\"]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let [InlineNode::Macro(acdc_parser::InlineMacro::Icon(icon))] = parsed.inlines() else {
            return Err("expected icon".into());
        };
        let alt = icon.alt.as_ref().ok_or("missing alt")?;
        assert_eq!(
            acdc_converters_core::icon::alt(&icon.target, Some(alt)),
            "© → --"
        );
        assert_eq!(
            acdc_converters_core::icon::alt_from_content((&icon.target).into(), Some(alt.into())),
            "© → --"
        );
    }
    Ok(())
}
