use crate::{Block, CompatibilityMode, InlineNode, Options, parse};
type Error = Box<dyn std::error::Error>;

fn attribute_text<'a>(node: &InlineNode<'a>) -> Result<&'a str, Error> {
    if let InlineNode::PlainText(text) = node {
        Ok(text.content)
    } else if let InlineNode::RawText(text) = node {
        Ok(text.content)
    } else {
        Err("expected literal attribute text".into())
    }
}

#[test]
fn fence_language_does_not_shift_or_map_preceding_positions() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[quote,Author,Book,linenums]\n```rust\nx\n```",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let metadata = parsed
            .document_storage()
            .blocks
            .first()
            .and_then(Block::metadata)
            .ok_or("missing fence")?;
        assert_eq!(
            metadata.attributes.get_string("language").as_deref(),
            Some("rust")
        );
        assert!(!metadata.attributes.contains_key("linenums"));
        assert!(metadata.attribution.is_none());
        assert_eq!(
            metadata.positional_attributes().collect::<Vec<_>>(),
            [Some("Author"), Some("Book"), Some("linenums")]
        );
    }
    Ok(())
}

#[test]
fn draft_maps_preceding_media_positions_after_aggregation() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let source = "[alt=Named]\n[Positional,100,200]\nimage::photo.png[]";
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Some(Block::Image(image)) = parsed.document_storage().blocks.first() else {
            return Err("missing image".into());
        };
        let alt = image.alt.as_ref().ok_or("missing alt")?;
        let text = attribute_text(alt.as_inlines().first().ok_or("missing alt text")?)?;
        assert_eq!(
            text,
            if mode == CompatibilityMode::Draft {
                "Positional"
            } else {
                "Named"
            }
        );
        assert_eq!(
            image.metadata.attributes.get_string("width").as_deref(),
            (mode == CompatibilityMode::Draft).then_some("100")
        );
        assert_eq!(
            image.metadata.positional_attributes().collect::<Vec<_>>(),
            [Some("100"), Some("200")]
        );
    }
    Ok(())
}

#[test]
fn draft_selects_source_and_quote_attributes_in_document_order() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for source in [
            "[source,rust,linenums,language=python,linenums=named]\n----\nx\n----",
            "[source,rust,linenums]\n[language=python,linenums=named]\n----\nx\n----",
        ] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document_storage()
                .blocks
                .first()
                .and_then(Block::metadata)
                .ok_or("missing block")?;
            let expected = if mode == CompatibilityMode::Draft {
                "python"
            } else {
                "rust"
            };
            assert_eq!(
                metadata.attributes.get_string("language").as_deref(),
                Some(expected)
            );
            assert_eq!(
                metadata.positional_attributes().collect::<Vec<_>>(),
                [Some("rust"), Some("linenums")]
            );
        }
        let parsed = parse(
            "[quote,Positional,Citation]\n[attribution=Named,citetitle=NamedTitle]\n____\nText.\n____",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let metadata = parsed
            .document_storage()
            .blocks
            .first()
            .and_then(Block::metadata)
            .ok_or("missing quote")?;
        let attribution = metadata.attribution.as_ref().ok_or("missing attribution")?;
        let text = attribute_text(attribution.as_inlines().first().ok_or("missing text")?)?;
        assert_eq!(
            text,
            if mode == CompatibilityMode::Draft {
                "Named"
            } else {
                "Positional"
            }
        );
    }
    Ok(())
}

#[test]
fn unrelated_block_forms_keep_positions_without_routing_them() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for source in [
            "[quote,Author,Book]\n----\nx\n----",
            "[quote,Author,Book]\n* Item",
            "[source,rust]\n====\nText.\n====",
        ] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let metadata = parsed
                .document_storage()
                .blocks
                .first()
                .and_then(Block::metadata)
                .ok_or("missing block")?;
            assert!(metadata.attribution.is_none(), "{source}");
            assert!(!metadata.attributes.contains_key("language"), "{source}");
            assert!(metadata.positional_attribute(0).is_some());
        }
    }
    Ok(())
}
