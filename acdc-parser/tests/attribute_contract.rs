#![cfg(feature = "asg")]

use acdc_parser::{
    CompatibilityMode, Options, asg,
    ast::{AttributeValueRef, BlockKind, BlockRef, CaptionRef, InlineKind, InlineRef},
    parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn attribute_meaning_agrees_across_reads_native_json_and_checked_export() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "= Meanings\n:label: *Declared*\n\n\
             [title='{label}']\nBody.\n\n\
             .Title\n[caption='*Custom* <tag> ']\n====\nCaption body.\n====\n\n\
             [quote,attribution=\"*Literal* -- ...\",citetitle='*Book*']\n____\nQuote body.\n____\n\n\
             image:photo.png[alt='*Bird*',title='_Tip_',width=0120]\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let native = serde_json::to_value(parsed.document())?;
        let graph = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        for (pointer, expected) in [
            ("/blocks/0/title/0/variant", json!("strong")),
            ("/blocks/0/title/0/inlines/0/value", json!("Declared")),
            ("/blocks/1/caption/prefix/0/variant", json!("strong")),
            ("/blocks/1/caption/prefix/1/value", json!(" <tag> ")),
            ("/blocks/2/attribution/0/value", json!("*Literal* -- ...")),
            ("/blocks/2/citetitle/0/variant", json!("strong")),
            ("/blocks/3/inlines/0/alt/0/variant", json!("strong")),
            ("/blocks/3/inlines/0/tooltip/0/variant", json!("emphasis")),
            (
                "/blocks/3/inlines/0/metadata/attributes/width",
                json!("0120"),
            ),
        ] {
            assert_eq!(
                graph.pointer(pointer),
                Some(&expected),
                "{mode:?}: {pointer}"
            );
        }
        let mut blocks = parsed.document().blocks();
        let Some(BlockKind::Paragraph(paragraph)) = blocks.next().map(BlockRef::kind) else {
            return Err("paragraph".into());
        };
        let title = paragraph.title().ok_or("title")?;
        assert_eq!(
            paragraph.attributes().get("title"),
            Some(AttributeValueRef::Content(title))
        );
        assert!(matches!(
            title.inlines().next().map(InlineRef::kind),
            Some(InlineKind::Bold(_))
        ));
        let BlockKind::Example(example) = blocks.next().ok_or("example")?.kind() else {
            return Err("example".into());
        };
        assert!(matches!(
            CaptionRef::resolve(example.attributes(), parsed.document().attributes(), None),
            Some(CaptionRef::CustomContent(_))
        ));
        for (native_path, graph_path) in [
            ("/blocks/0/title", "/blocks/0/title"),
            ("/blocks/1/metadata/caption", "/blocks/1/caption/prefix"),
            ("/blocks/2/metadata/attribution", "/blocks/2/attribution"),
            ("/blocks/2/metadata/citetitle", "/blocks/2/citetitle"),
            ("/blocks/3/inlines/0/alt", "/blocks/3/inlines/0/alt"),
            ("/blocks/3/inlines/0/tooltip", "/blocks/3/inlines/0/tooltip"),
        ] {
            assert!(native.pointer(native_path).is_some(), "{native_path}");
            assert!(graph.pointer(graph_path).is_some(), "{graph_path}");
        }
        assert!(
            native
                .pointer("/blocks/0/metadata/attributes/title")
                .is_none()
        );
        assert!(
            native
                .pointer("/blocks/2/metadata/attributes/attribution")
                .is_none()
        );
        assert_eq!(serde_json::to_value(parsed.document())?, native);
        assert_eq!(
            asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?,
            graph
        );
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/1/metadata/caption")
        );
        let mut wrong = graph.clone();
        *wrong
            .pointer_mut("/blocks/0/title/0/inlines/0/value")
            .ok_or("title value")? = json!("Incorrect");
        let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
        let validator = jsonschema::validator_for(&schema)?;
        assert!(validator.is_valid(&wrong));
        assert_ne!(
            wrong.pointer("/blocks/0/title"),
            graph.pointer("/blocks/0/title")
        );
    }
    Ok(())
}

#[test]
fn snapshot_and_selected_content_survive_projection_without_side_effects() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(
            "= Snapshots\n:inner: Before\n:outer: \\{inner}\n\n\
             [quote,attribution='{outer}']\n:inner: After\n____\nBody.\n____\n\n\
             [title='footnote:discarded[Discarded]']\n\
             [title='*Selected* footnote:kept[Kept]']\nBody.\n",
            &options,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let quote = parsed
            .document()
            .blocks()
            .find_map(|block| {
                if let BlockKind::Quote(block) = block.kind() {
                    Some(block)
                } else {
                    None
                }
            })
            .ok_or("quote")?;
        let attribution = quote.attribution().ok_or("attribution")?;
        let value = asg::inlines(attribution)?;
        assert_eq!(
            value.pointer("/0/value"),
            Some(&json!(if mode == CompatibilityMode::Asciidoctor {
                "Before"
            } else {
                "{inner}"
            }))
        );
        assert_eq!(
            value.pointer("/0/location"),
            Some(&json!([{"line":5,"col":21},{"line":5,"col":27}]))
        );
        assert!(matches!(
            attribution.inlines().next().map(InlineRef::kind),
            Some(InlineKind::Plain(_))
        ));
        assert_eq!(parsed.document().footnotes().count(), 1);
        assert_eq!(
            parsed.document().footnotes().next().ok_or("footnote")?.id(),
            Some("kept")
        );
        let graph = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(graph.pointer("/footnotes/0/id"), Some(&json!("kept")));
        assert_eq!(
            graph.pointer("/footnotes/0/inlines/0/value"),
            Some(&json!("Kept"))
        );
        assert_eq!(
            graph
                .get("footnotes")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn mode_specific_literal_content_and_empty_presence_have_distinct_graphs() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[title=\"*Literal*\"]\nBody.\n\n[title=\"\"]\nBody.\n\nBody.\n\n\
             image:photo.png[alt=\"\",title=\"\"] and image:photo.png[]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let graph = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let title = graph.pointer("/blocks/0/title/0").ok_or("title")?;
        if mode == CompatibilityMode::Asciidoctor {
            assert_eq!(title.get("variant"), Some(&json!("strong")));
            assert_eq!(title.pointer("/inlines/0/value"), Some(&json!("Literal")));
        } else {
            assert_eq!(title.get("name"), Some(&json!("text")));
            assert_eq!(title.get("value"), Some(&json!("*Literal*")));
        }
        assert_eq!(graph.pointer("/blocks/1/title"), Some(&json!([])));
        assert!(graph.pointer("/blocks/2/title").is_none());
        let images = graph
            .pointer("/blocks/3/inlines")
            .and_then(Value::as_array)
            .ok_or("inlines")?;
        let images = images
            .iter()
            .filter(|node| node.get("name") == Some(&json!("image")))
            .collect::<Vec<_>>();
        let [empty, absent] = images.as_slice() else {
            return Err("two images".into());
        };
        assert_eq!(empty.get("alt"), Some(&json!([])));
        assert_eq!(empty.get("tooltip"), Some(&json!([])));
        assert!(absent.get("alt").is_none());
        assert!(absent.get("tooltip").is_none());
    }
    Ok(())
}
