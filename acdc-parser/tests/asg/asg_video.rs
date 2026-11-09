#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, Document, Location, Options, Source, SourceUrl, Video, asg,
    ast::BlockKind, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn complete_video_targets_and_provider_controls_survive_both_profiles() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/video_targets.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
        assert_eq!(value, expected, "{mode:?}");
        let mut upstream = asg::export_document(parsed.document())?;
        upstream
            .as_object_mut()
            .ok_or("document")?
            .insert("profile".into(), json!(asg::ACDC_SCHEMA_ID));
        assert_eq!(value, upstream);
        for (index, block) in parsed.document().blocks().enumerate() {
            let BlockKind::Video(video) = block.kind() else {
                return Err("video".into());
            };
            assert_eq!(
                value.pointer(&format!("/blocks/{index}/target")),
                Some(&json!(video.source().to_string()))
            );
            assert!(value.pointer(&format!("/blocks/{index}/sources")).is_none());
        }
        for (path, expected) in [
            ("/blocks/2/target", json!("first,second,third")),
            ("/blocks/3/target", json!("first,,third,")),
            ("/blocks/4/target", json!(",first,,last,")),
            ("/blocks/0/metadata/attributes/poster", json!("cover.png")),
            ("/blocks/2/metadata/attributes/poster", json!("youtube")),
            ("/blocks/5/metadata/attributes/poster", json!("vimeo")),
            ("/blocks/6/metadata/attributes/poster", json!("")),
            ("/blocks/2/metadata/options", json!(["loop", "muted"])),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn video_source_variants_preserve_original_target_text() -> Result<(), Error> {
    for source in [
        Source::Name("first,,third,"),
        Source::Path("movie.mp4".into()),
        Source::Url(SourceUrl::new(
            "HTTPS://Example.org/Video%20A.mp4?x=1,2#t=3",
        )?),
    ] {
        let expected = source.to_string();
        let mut document = Document::default();
        document.push_block(Block::Video(Video::new(source, Location::default())));
        for profile in [asg::Profile::Upstream, asg::Profile::Acdc] {
            let value = asg::export_document_with_profile((&document).into(), profile)?;
            assert_eq!(value.pointer("/blocks/0/target"), Some(&json!(expected)));
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_utf8_video_paths_are_rejected_without_replacement_characters() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};
    let source = Source::Path(PathBuf::from(OsString::from_vec(vec![b'x', 0xff])));
    let mut document = Document::default();
    document.push_block(Block::Video(Video::new(source, Location::default())));
    for profile in [asg::Profile::Upstream, asg::Profile::Acdc] {
        assert!(
            matches!(asg::export_document_with_profile((&document).into(),profile),Err(asg::Error::Unsupported {path,construct:Some("video"),..}) if path=="/blocks/0/target")
        );
    }
}
