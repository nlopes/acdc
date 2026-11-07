#![cfg(feature = "asg")]
use acdc_parser::{
    Block, CompatibilityMode, Document, ListItem, ListItemCheckedStatus, Location, Options,
    OrderedList, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
type Error = Box<dyn std::error::Error + Send + Sync>;
#[test]
fn checklists_preserve_state_separately_from_item_content() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/checklists.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        assert!(value.pointer("/blocks/0/items/0/checked").is_none());
        assert!(value.pointer("/blocks/0/items/5/checked").is_none());
        for (path, checked) in [
            ("/blocks/0/items/1/checked", false),
            ("/blocks/0/items/2/checked", true),
            ("/blocks/0/items/3/checked", true),
            ("/blocks/0/items/4/checked", true),
            ("/blocks/0/items/1/blocks/0/items/0/checked", true),
        ] {
            assert_eq!(value.pointer(path), Some(&json!(checked)));
        }
        assert_eq!(
            value.pointer("/blocks/0/items/1/principal/1/variant"),
            Some(&json!("strong"))
        );
        assert_eq!(
            value.pointer("/blocks/0/items/2/blocks/0/name"),
            Some(&json!("open"))
        );
        assert_eq!(
            value.pointer("/blocks/0/items/1/blocks/0/items/0/marker"),
            Some(&json!("**"))
        );
    }
    Ok(())
}

#[test]
fn prepared_item_state_is_independent_of_marker_and_principal() -> Result<(), Error> {
    for (state, expected) in [
        (None, None),
        (Some(ListItemCheckedStatus::Unchecked), Some(false)),
        (Some(ListItemCheckedStatus::Checked), Some(true)),
    ] {
        let mut item = ListItem::new(Vec::new(), 1, ".", Location::default());
        item.set_checked(state);
        let mut document = Document::default();
        document.push_block(Block::OrderedList(OrderedList::new(
            vec![item],
            ".",
            Location::default(),
        )));
        let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
        assert_eq!(
            value.pointer("/blocks/0/items/0/checked"),
            expected.map(|checked| json!(checked)).as_ref()
        );
        assert_eq!(
            value.pointer("/blocks/0/items/0/principal"),
            Some(&json!([]))
        );
        assert_eq!(value.pointer("/blocks/0/items/0/marker"), Some(&json!(".")));
    }
    Ok(())
}

#[test]
fn strict_checklists_reject_while_nested_icons_are_preserved() -> Result<(), Error> {
    for source in ["* [ ] Pending", "* [x] Done"] {
        let parsed = parse(source, &Options::default())?;
        assert!(
            matches!(asg::document(parsed.document()),Err(asg::Error::Unsupported {path, construct:Some("list item"), ..}) if path == "/blocks/0/items/0/checked")
        );
    }
    let parsed = parse("* [x] icon:heart[]", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/items/0/principal/0/target"),
        Some(&json!("heart"))
    );
    Ok(())
}

#[test]
fn checklist_schema_rejects_wrong_states_and_callout_checkboxes() -> Result<(), Error> {
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let parsed = parse("* [ ] Pending", &Options::default())?;
    let valid = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert!(validator.is_valid(&valid));
    for state in [
        json!("checked"),
        json!(null),
        json!(0),
        json!({"checked":true}),
    ] {
        let mut invalid = valid.clone();
        *invalid
            .pointer_mut("/blocks/0/items/0/checked")
            .ok_or("checked")? = state;
        assert!(!validator.is_valid(&invalid));
    }
    let mut invalid = valid;
    *invalid.pointer_mut("/blocks/0/variant").ok_or("variant")? = json!("callout");
    assert!(!validator.is_valid(&invalid));
    Ok(())
}
