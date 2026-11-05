use acdc_parser::{Anchor, ElementId, Location};
type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn unknown_identity_ranges_are_absent_in_native_json() -> Result<(), Error> {
    for value in [
        serde_json::to_value(Anchor::new("anchor", Location::default()))?,
        serde_json::to_value(ElementId::new("inline", Location::default()))?,
    ] {
        assert!(value.get("location").is_none(), "{value}");
        assert!(value.get("id").is_some());
    }
    Ok(())
}

#[test]
#[cfg(feature = "asg")]
fn partial_identity_ranges_remain_visible_and_fail_export() -> Result<(), Error> {
    use acdc_parser::{Block, Document, InlineNode, Paragraph, asg};
    use serde_json::json;
    let mut location = Location::default();
    location.end.line = 1;
    let anchor = Anchor::new("partial", location);
    assert_eq!(
        serde_json::to_value(&anchor)?.pointer("/location/0/line"),
        Some(&json!(0))
    );
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        vec![InlineNode::InlineAnchor(anchor)],
        Location::default(),
    )));
    assert!(
        matches!(asg::document_with_profile((&document).into(), asg::Profile::Acdc), Err(asg::Error::InvalidLocation{path,..}) if path == "/blocks/0/inlines/0/location/0")
    );
    Ok(())
}

#[test]
fn parsed_ids_distinguish_authored_ranges_from_generated_values() -> Result<(), Error> {
    use acdc_parser::{
        CompatibilityMode, Options, SafeMode,
        ast::{BlockKind, IdOrigin},
        parse_file,
    };
    use std::path::Path;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests/anchor_provenance.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &path,
            &Options::builder()
                .with_safe_mode(SafeMode::Unsafe)
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let mut found = Vec::new();
        for block in parsed.document().blocks() {
            let Some(id) = block.id() else {
                continue;
            };
            found.push(id.as_str());
            if id.origin() == IdOrigin::Generated {
                assert_eq!(id.as_str(), "_generated_title");
                assert!(id.location().is_none());
                assert!(block.location().start().line() > 0);
                assert!(block.primary_anchor().is_none());
            } else {
                let location = id.location().ok_or("authored ID location")?;
                assert!(location.start().line() > 0);
                if id.as_str() == "included-é" {
                    assert_eq!(location.start().line(), 3);
                    assert!(location.start().include_chain().is_some());
                    assert!(
                        parsed
                            .resolve_location(location)
                            .file
                            .is_some_and(|file| file.ends_with("includes/anchor_provenance.adoc"))
                    );
                }
            }
            if let BlockKind::Section(section) = block.kind() {
                assert!(section.title().is_some());
            }
        }
        assert_eq!(found, ["authored", "_generated_title", "included-é"]);
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn prepared_identity_reads_keep_absent_and_partial_ranges_distinct() {
    use acdc_parser::ast::{AnchorRef, ElementIdRef, IdOrigin};
    let anchor = Anchor::new("caller", Location::default());
    let id = AnchorRef::from(&anchor).element_id();
    assert_eq!(id.origin(), IdOrigin::Explicit);
    assert!(id.location().is_none());
    let owner = ElementId::new("inline", Location::default());
    assert!(ElementIdRef::from(&owner).location().is_none());
    let mut partial = Location::default();
    partial.end.line = 1;
    assert!(!partial.is_unknown());
    let anchor = Anchor::new("partial", partial);
    assert!(AnchorRef::from(&anchor).element_id().location().is_some());
    assert!(Location::default().is_unknown());
}

#[test]
#[cfg(feature = "asg")]
fn generated_ids_have_no_token_spans_and_unknown_anchors_export_without_them() -> Result<(), Error>
{
    use acdc_parser::{Block, Document, InlineNode, Options, Paragraph, asg, parse};
    use serde_json::json;
    let parsed = parse("== Generated\n\nBody.", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/blocks/0/id"), Some(&json!("_generated")));
    assert!(value.pointer("/blocks/0/id_location").is_none());
    assert!(value.pointer("/references/_generated/location").is_some());
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        vec![InlineNode::InlineAnchor(Anchor::new(
            "caller",
            Location::default(),
        ))],
        Location::default(),
    )));
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/id"),
        Some(&json!("caller"))
    );
    assert!(value.pointer("/blocks/0/inlines/0/location").is_none());
    Ok(())
}
