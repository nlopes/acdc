use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{AttributeValueRef, BlockKind, ContentRef, ImageRef, InlineKind, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn literal(content: ContentRef<'_>) -> Result<String, Error> {
    content
        .inlines()
        .map(|node| {
            let InlineKind::Plain(text) = node.kind() else {
                return Err("expected literal text".into());
            };
            Ok(text.text())
        })
        .collect()
}

#[rstest::rstest]
fn scalar_media_values_expand_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/document_attribute_media_insertion.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let images: Vec<ImageRef<'_>> = parsed
        .document_ref()
        .blocks()
        .filter_map(|block| {
            if let BlockKind::Image(image) = block.kind() {
                Some(image)
            } else {
                None
            }
        })
        .collect();
    let actual: Vec<_> = images
        .iter()
        .map(|image| {
            Ok((
                image.source().to_string(),
                literal(image.alt().ok_or("missing alt")?)?,
            ))
        })
        .collect::<Result<_, Error>>()?;
    assert_eq!(
        actual,
        [
            ("unquoted.png", "{name}"),
            ("double.png", "{name}"),
            ("single.png", "{name}"),
            ("named.png", "{name}"),
            ("expanded-slots.png", "{name}"),
            ("captured.png", "Early"),
            ("empty.png", ""),
            ("literal-markup.png", "*{name}* & <tag>"),
            ("macro-text.png", "footnote:unused[Ignored.]"),
            ("metadata.png", "{name}"),
        ]
        .map(|(source, alt)| (source.to_owned(), alt.to_owned()))
    );
    let expanded_slots = images.get(4).ok_or("missing expanded slots image")?;
    assert_eq!(
        expanded_slots.attributes().get("width"),
        Some(AttributeValueRef::Text("80"))
    );
    assert_eq!(
        expanded_slots.attributes().get("height"),
        Some(AttributeValueRef::Text("40"))
    );
    assert!(
        images
            .get(6)
            .ok_or("missing empty-alt image")?
            .alt()
            .is_some_and(ContentRef::is_empty)
    );
    assert_eq!(parsed.document().footnotes, []);
    Ok(())
}

#[rstest::rstest]
fn scalar_media_values_keep_single_expansion_in_nested_contexts(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/document_attribute_media_contexts.adoc");
    let parsed = parse_file(
        source,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let actual: Vec<_> = parsed
        .document_ref()
        .descendants()
        .filter_map(|node| {
            let StructuralRef::Block(block) = node else {
                return None;
            };
            let BlockKind::Image(image) = block.kind() else {
                return None;
            };
            Some(
                image
                    .alt()
                    .ok_or_else(|| Error::from("missing explicit alt"))
                    .and_then(literal),
            )
        })
        .collect::<Result<_, _>>()?;
    assert_eq!(actual, ["α {name} café", "β {name}", "{name}"]);
    Ok(())
}
