use crate::{
    Anchor, BlockMetadata, Location, Options,
    ast::{IdOrigin, SectionRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn retained_id_origin_survives_direct_metadata_edits() -> Result<(), Error> {
    let parsed = parse(
        "[id=explicit]\n== Heading\n\n== Generated\n",
        &Options::default(),
    )?;
    let mut owners = parsed.document_storage().blocks.iter();
    let crate::Block::Section(explicit) = owners.next().ok_or("explicit")? else {
        return Err("section".into());
    };
    let mut explicit = explicit.clone();
    explicit.metadata = BlockMetadata::default();
    let id = SectionRef::from(&explicit)
        .id()
        .ok_or("retained explicit ID")?;
    assert_eq!((id.as_str(), id.origin()), ("explicit", IdOrigin::Explicit));
    let crate::Block::Section(generated) = owners.next().ok_or("generated")? else {
        return Err("section".into());
    };
    let mut generated = generated.clone();
    generated.metadata.id = Some(Anchor::new("override", Location::default()));
    assert_eq!(
        SectionRef::from(&generated)
            .id()
            .ok_or("override")?
            .origin(),
        IdOrigin::Explicit
    );
    generated.metadata.id = None;
    let id = SectionRef::from(&generated)
        .id()
        .ok_or("retained generated ID")?;
    assert_eq!(
        (id.as_str(), id.origin()),
        ("_generated", IdOrigin::Generated)
    );
    assert!(
        SectionRef::from(&generated.with_metadata(BlockMetadata::default())?)
            .id()
            .is_some_and(|id| id.as_str() == "_generated" && id.origin() == IdOrigin::Generated)
    );
    Ok(())
}
