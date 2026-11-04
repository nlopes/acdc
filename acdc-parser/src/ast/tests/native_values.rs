use crate::{Block, ContentAttribute, Options, Title, parse};
use serde_json::{Value, json};

type TestError = Box<dyn std::error::Error>;

#[test]
fn block_image_alt_cannot_reappear_from_the_scalar_map() -> Result<(), TestError> {
    let parsed = parse("image::photo.png[Alt]", &Options::default())?;
    let [Block::Image(owner)] = parsed.document_storage().blocks.as_slice() else {
        return Err("image".into());
    };
    let mut owner = owner.clone();
    owner.alt = None;
    owner.metadata.attributes.set("alt".into(), "stale".into());
    assert!(serde_json::to_value(owner).is_err());
    Ok(())
}

#[test]
fn content_replacement_and_removal_do_not_resurrect_old_values() -> Result<(), TestError> {
    let parsed = parse("image::photo.png[Alt,title=Title]", &Options::default())?;
    let [Block::Image(owner)] = parsed.document_storage().blocks.as_slice() else {
        return Err("image".into());
    };
    let mut owner = owner.clone();
    let original = serde_json::to_value(&owner)?;
    assert!(original.get("alt").is_some());
    owner.alt = Some(ContentAttribute::default());
    owner.title = Title::new(Vec::new());
    let empty = serde_json::to_value(&owner)?;
    assert_eq!(empty.get("alt"), Some(&json!([])));
    assert_eq!(empty.get("title"), Some(&json!([])));
    owner.alt = None;
    owner.title = Title::default();
    let absent = serde_json::to_value(&owner)?;
    assert!(absent.get("alt").is_none());
    assert!(absent.get("title").is_none());
    assert!(absent.pointer("/metadata/attributes/alt").is_none());
    assert!(absent.pointer("/metadata/attributes/title").is_none());
    assert_eq!(
        original,
        serde_json::to_value(
            parsed
                .document_storage()
                .blocks
                .first()
                .ok_or("original image")?
        )?
    );
    Ok(())
}

#[test]
fn content_metadata_has_the_same_presence_rules_without_its_owner() -> Result<(), TestError> {
    let parsed = parse(
        "[quote,attribution='Author',citetitle='Book',reftext='Label',caption='Prefix']\n____\nBody.\n____",
        &Options::default(),
    )?;
    let owner = parsed.document_storage().blocks.first().ok_or("quote")?;
    let mut metadata = owner.metadata().ok_or("metadata")?.clone();
    let direct = serde_json::to_value(&metadata)?;
    assert_eq!(serde_json::to_value(owner)?.get("metadata"), Some(&direct));
    for name in ["attribution", "citetitle", "reftext", "caption"] {
        assert!(
            direct
                .get(name)
                .and_then(Value::as_array)
                .is_some_and(|nodes| !nodes.is_empty())
        );
        assert!(direct.pointer(&format!("/attributes/{name}")).is_none());
    }
    metadata.attribution = Some(ContentAttribute::default());
    metadata.citetitle = Some(ContentAttribute::default());
    metadata.reftext = Some(ContentAttribute::default());
    metadata.caption = Some(crate::Caption::CustomContent(ContentAttribute::default()));
    let empty = serde_json::to_value(&metadata)?;
    for name in ["attribution", "citetitle", "reftext", "caption"] {
        assert_eq!(empty.get(name), Some(&json!([])));
    }
    metadata.attribution = None;
    metadata.citetitle = None;
    metadata.reftext = None;
    metadata.caption = None;
    let absent = serde_json::to_value(&metadata)?;
    for name in ["attribution", "citetitle", "reftext", "caption"] {
        assert!(absent.get(name).is_none());
    }
    Ok(())
}
