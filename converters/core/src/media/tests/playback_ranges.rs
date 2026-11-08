use acdc_parser::{
    AttributeValue, Audio, Location, Source, Video,
    ast::{AudioRef, VideoRef},
};

use crate::{
    media::{with_time_range, with_time_range_from_attributes},
    video::TryUrl,
};

#[test]
fn ranges_keep_absent_empty_nontext_and_quoted_values() -> Result<(), Box<dyn std::error::Error>> {
    for (start, end, expected) in [
        (None, None, "clip,audio.ogg?x=1#part"),
        (Some(""), None, "clip,audio.ogg?x=1#part#t="),
        (None, Some(""), "clip,audio.ogg?x=1#part#t=,"),
        (Some(""), Some(""), "clip,audio.ogg?x=1#part#t=,"),
        (
            Some("\"5\""),
            Some("'9'"),
            "clip,audio.ogg?x=1#part#t=\"5\",'9'",
        ),
    ] {
        let mut audio = Audio::new(Source::Name("clip,audio.ogg?x=1#part"), Location::default());
        let mut metadata = audio.take_metadata();
        metadata
            .attributes
            .insert("start".into(), AttributeValue::Bool(true));
        metadata
            .attributes
            .insert("end".into(), AttributeValue::None);
        audio.set_metadata(metadata)?;
        for (key, value) in [("start", start), ("end", end)] {
            if let Some(value) = value {
                let mut metadata = audio.take_metadata();
                metadata.attributes.set(key.into(), value.into());
                audio.set_metadata(metadata)?;
            }
        }
        assert_eq!(
            with_time_range(
                &AudioRef::from(&audio).source().to_string(),
                &audio.clone_metadata().attributes
            ),
            expected
        );
        assert_eq!(
            with_time_range_from_attributes(
                &AudioRef::from(&audio).source().to_string(),
                AudioRef::from(&audio).attributes()
            ),
            expected
        );
        let mut video = Video::new(Source::Name("clip,audio.ogg?x=1#part"), Location::default());
        let mut metadata = video.take_metadata();
        metadata.attributes = audio.take_metadata().attributes;
        video.set_metadata(metadata)?;
        assert_eq!(video.try_url(false)?, expected);
        assert_eq!(video.try_url(true)?, expected);
        assert_eq!(VideoRef::from(&video).try_url(false)?, expected);
        assert_eq!(VideoRef::from(&video).try_url(true)?, expected);
    }
    Ok(())
}
