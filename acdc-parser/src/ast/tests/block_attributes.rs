use crate::{
    CompatibilityMode, Options,
    ast::{AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, SubstitutionSpecRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn inventory(attributes: AttributesRef<'_>) {
    let entries = attributes.iter().collect::<Vec<_>>();
    for (name, value) in &entries {
        assert_eq!(attributes.get(name), Some(*value));
        assert!(attributes.contains(name));
        assert_eq!(entries.iter().filter(|(key, _)| key == name).count(), 1);
    }
    assert_eq!(attributes.is_empty(), entries.is_empty());
    assert_eq!(attributes.get("roles"), attributes.get("role"));
    assert_eq!(attributes.get("options"), attributes.get("opts"));
    assert!(!attributes.contains("missing"));
}

#[rstest::rstest]
#[case("Text.")]
#[case("== Heading\n\nText.")]
#[case("====\nBody.\n====")]
#[case("----\nCode.\n----")]
#[case("....\nLiteral.\n....")]
#[case("--\nBody.\n--")]
#[case("****\nBody.\n****")]
#[case("++++\nRaw.\n++++")]
#[case("____\nWords.\n____")]
#[case("[verse]\n____\nWords.\n____")]
#[case("[stem]\n++++\nx\n++++")]
#[case("NOTE: Words.")]
#[case("* Item")]
#[case(". Item")]
#[case("<1> Callout")]
#[case("Term:: Description")]
#[case("image::bird.png[Alt]")]
#[case("audio::sound.ogg[]")]
#[case("video::movie.mp4[]")]
#[case("[discrete]\n== Heading")]
#[case("toc::[]")]
#[case("<<<\n\nAfter.")]
#[case("////\nComment\n////")]
#[case("|===\n|Cell\n|===")]
fn block_inventories_project_the_retained_content_owners(
    #[case] body: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "[attribution='*Ada*',citetitle='_Book_',caption='Note: ',reftext='*Reference*',subs=none,role='one two',opts='first,second']\n{body}\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document_storage())?;
    let owner = parsed.document_storage().blocks.first().ok_or("block")?;
    let metadata = owner.metadata().ok_or("block metadata")?;
    let attributes = BlockRef::from(owner)
        .attributes()
        .ok_or("attribute reads")?;
    inventory(attributes);
    for (name, content) in [
        ("attribution", metadata.attribution.as_ref()),
        ("citetitle", metadata.citetitle.as_ref()),
        ("caption", metadata.caption_content()),
        ("reftext", metadata.reftext.as_ref()),
    ] {
        assert!(content.is_some(), "missing retained {name} for {body}");
        assert_eq!(
            attributes.get(name),
            content.map(|content| AttributeValueRef::Content(ContentRef::from(content)))
        );
    }
    assert_eq!(
        attributes.get("subs"),
        metadata
            .substitutions
            .as_ref()
            .map(|spec| AttributeValueRef::Substitutions(SubstitutionSpecRef::from(spec)))
    );
    assert!(attributes.contains("subs"));
    assert_eq!(serde_json::to_value(parsed.document_storage())?, before);
    Ok(())
}

#[rstest::rstest]
fn captions_and_anchor_labels_do_not_fabricate_named_attributes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ".Numbered\nimage::bird.png[]\n\n:caption: Global: \n\n.Document caption\nimage::bird.png[]\n\n[caption='']\n.Empty\nimage::bird.png[]\n\n[[target,Anchor label]]\nParagraph.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let images = parsed
        .document()
        .blocks()
        .filter_map(|block| {
            if let BlockKind::Image(image) = block.kind() {
                Some(image)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [numbered, global, empty] = images.as_slice() else {
        return Err("three images".into());
    };
    assert!(!numbered.attributes().contains("caption"));
    assert!(!global.attributes().contains("caption"));
    let Some(AttributeValueRef::Content(caption)) = empty.attributes().get("caption") else {
        return Err("explicit empty caption".into());
    };
    assert!(caption.is_empty());
    let paragraph = parsed.document_storage().blocks.last().ok_or("paragraph")?;
    assert!(
        !BlockRef::from(paragraph)
            .attributes()
            .ok_or("attributes")?
            .contains("reftext")
    );
    assert!(
        paragraph
            .metadata()
            .ok_or("metadata")?
            .anchors
            .last()
            .ok_or("anchor")?
            .xreflabel
            .is_some()
    );
    Ok(())
}
