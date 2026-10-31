//! Playback links preserve provider precedence, scalar values, and owned results.

use acdc_converters_core::{
    media::{with_time_range, with_time_range_from_attributes},
    video::{TryUrl, UrlError},
};
use acdc_parser::{
    AttributeValue, Audio, BlockMetadata, ElementAttributes, Location, Source, Video,
    ast::{AudioRef, VideoRef},
};

fn video<'a>(target: &'a str, values: &[(&'a str, &'a str)], options: Vec<&'a str>) -> Video<'a> {
    let mut attributes = ElementAttributes::default();
    for &(key, value) in values {
        attributes.set(key.into(), value.into());
    }
    Video::new(Source::Name(target), Location::default()).with_metadata(
        BlockMetadata::new()
            .with_attributes(attributes)
            .with_options(options),
    )
}

fn assert_urls(video: &Video<'_>, watch: &str, embed: &str) -> Result<(), UrlError> {
    assert_eq!(video.try_url(false)?, watch);
    assert_eq!(video.try_url(true)?, embed);
    let typed = VideoRef::from(video);
    assert_eq!(typed.try_url(false)?, watch);
    assert_eq!(typed.try_url(true)?, embed);
    Ok(())
}

#[test]
fn youtube_playlists_keep_shorthand_and_named_precedence() -> Result<(), UrlError> {
    for (target, controls, watch, embed) in [
        (
            "one,two,three",
            vec![("playlist", "ignored")],
            "https://www.youtube.com/watch?v=one&playlist=one,two,three",
            "https://www.youtube.com/embed/one?rel=0&loop=1&playlist=one,two,three",
        ),
        (
            "one,two,three/list",
            vec![("list", "ignored")],
            "https://www.youtube.com/watch?v=one,two,three&list=list",
            "https://www.youtube.com/embed/one,two,three?rel=0&loop=1&list=list",
        ),
        (
            "one,two",
            vec![("list", "named"), ("playlist", "ignored")],
            "https://www.youtube.com/watch?v=one,two&list=named",
            "https://www.youtube.com/embed/one,two?rel=0&loop=1&list=named",
        ),
        (
            "one",
            vec![("playlist", "two,three")],
            "https://www.youtube.com/watch?v=one&playlist=one,two,three",
            "https://www.youtube.com/embed/one?rel=0&loop=1&playlist=one,two,three",
        ),
        (
            "one",
            vec![("playlist", "")],
            "https://www.youtube.com/watch?v=one&playlist=one,",
            "https://www.youtube.com/embed/one?rel=0&loop=1&playlist=one,",
        ),
    ] {
        let mut controls = controls;
        controls.push(("poster", "youtube"));
        assert_urls(&video(target, &controls, vec!["loop"]), watch, embed)?;
    }
    Ok(())
}

#[test]
fn youtube_controls_keep_raw_time_values_and_unquoted_language() -> Result<(), UrlError> {
    let video = video(
        "one",
        &[
            ("poster", "\"youtube\""),
            ("start", "\"5\""),
            ("end", ""),
            ("theme", "\"dark\""),
            ("lang", "\"en\""),
        ],
        vec![
            "related",
            "autoplay",
            "loop",
            "muted",
            "nocontrols",
            "nofullscreen",
            "modest",
        ],
    );
    assert_urls(
        &video,
        "https://www.youtube.com/watch?v=one&t=\"5\"&end=",
        "https://www.youtube.com/embed/one?rel=1&start=\"5\"&end=&theme=\"dark\"&autoplay=1&loop=1&playlist=one&mute=1&controls=0&fs=0&hl=en&modestbranding=1",
    )
}

#[test]
fn vimeo_hashes_and_times_keep_watch_and_embed_policies() -> Result<(), UrlError> {
    for (target, hash) in [("123/secret", "ignored"), ("123", "secret")] {
        assert_urls(
            &video(
                target,
                &[("poster", "vimeo"), ("hash", hash), ("start", "\"5\"")],
                vec!["autoplay", "loop", "muted"],
            ),
            "https://vimeo.com/123/secret#t=\"5\"",
            "https://player.vimeo.com/video/123?h=secret&autoplay=1&loop=1&muted=1#at=5",
        )?;
    }
    assert_urls(
        &video(
            "123",
            &[("poster", "vimeo"), ("hash", ""), ("start", "")],
            vec![],
        ),
        "https://vimeo.com/123/#t=",
        "https://player.vimeo.com/video/123?h=#at=",
    )
}

#[test]
fn ranges_keep_absent_empty_nontext_and_quoted_values() -> Result<(), UrlError> {
    for (start, end, expected) in [
        (None, None, "clip,audio.ogg?x=1#part"),
        (Some(""), None, "clip,audio.ogg?x=1#part#t="),
        (None, Some(""), "clip,audio.ogg?x=1#part#t=,"),
        (Some(""), Some(""), "clip,audio.ogg?x=1#part#t=,"),
        (Some("\"5\""), Some("'9'"), "clip,audio.ogg?x=1#part#t=5,9"),
    ] {
        let mut audio = Audio::new(Source::Name("clip,audio.ogg?x=1#part"), Location::default());
        audio
            .metadata
            .attributes
            .insert("start".into(), AttributeValue::Bool(true));
        audio
            .metadata
            .attributes
            .insert("end".into(), AttributeValue::None);
        for (key, value) in [("start", start), ("end", end)] {
            if let Some(value) = value {
                audio.metadata.attributes.set(key.into(), value.into());
            }
        }
        assert_eq!(
            with_time_range(&audio.source.to_string(), &audio.metadata.attributes),
            expected
        );
        assert_eq!(
            with_time_range_from_attributes(
                &audio.source.to_string(),
                AudioRef::from(&audio).attributes()
            ),
            expected
        );
        let mut video = video("clip,audio.ogg?x=1#part", &[], vec![]);
        video.metadata.attributes = audio.metadata.attributes;
        assert_eq!(video.try_url(false)?, expected);
        assert_eq!(video.try_url(true)?, expected);
        assert_eq!(VideoRef::from(&video).try_url(false)?, expected);
        assert_eq!(VideoRef::from(&video).try_url(true)?, expected);
    }
    Ok(())
}

#[test]
fn playback_results_outlive_temporary_targets_and_edited_controls() -> Result<(), UrlError> {
    let (before, after) = {
        let target = String::from("one,two,three");
        let start = String::from("5");
        let mut video = video(&target, &[("poster", "youtube"), ("start", &start)], vec![]);
        let before = VideoRef::from(&video).try_url(true)?;
        assert_eq!(video.try_url(true)?, before);
        video
            .metadata
            .attributes
            .set("start".into(), String::from("9").into());
        video.metadata.options.push("muted");
        let after = VideoRef::from(&video).try_url(true)?;
        assert_eq!(video.try_url(true)?, after);
        (before, after)
    };
    assert_eq!(
        before,
        "https://www.youtube.com/embed/one?rel=0&start=5&playlist=one,two,three"
    );
    assert_eq!(
        after,
        "https://www.youtube.com/embed/one?rel=0&start=9&mute=1&playlist=one,two,three"
    );
    Ok(())
}

#[test]
fn empty_targets_fail_before_provider_specific_controls() {
    for poster in ["", "youtube", "vimeo"] {
        let video = video("", &[("poster", poster)], vec![]);
        for embed in [false, true] {
            assert!(matches!(video.try_url(embed), Err(UrlError::MissingSource)));
            assert!(matches!(
                VideoRef::from(&video).try_url(embed),
                Err(UrlError::MissingSource)
            ));
        }
    }
}

#[test]
fn range_results_outlive_temporary_attribute_values() {
    let range = {
        let source = String::from("track.ogg");
        let start = String::from("\"5\"");
        let mut audio = Audio::new(Source::Name(&source), Location::default());
        audio
            .metadata
            .attributes
            .set("start".into(), start.as_str().into());
        let audio = AudioRef::from(&audio);
        with_time_range_from_attributes(&audio.source().to_string(), audio.attributes())
    };
    assert_eq!(range, "track.ogg#t=5");
}

#[test]
fn nontext_controls_keep_a_complete_local_target() -> Result<(), UrlError> {
    for value in [
        AttributeValue::None,
        AttributeValue::Bool(false),
        AttributeValue::Bool(true),
    ] {
        let mut video = video("one,two,three", &[], vec!["loop", "related"]);
        for key in ["poster", "start", "end"] {
            video.metadata.attributes.set(key.into(), value.clone());
        }
        assert_urls(&video, "one,two,three", "one,two,three")?;
    }
    Ok(())
}
