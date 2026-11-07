//! Controls for the pinned upstream schema, independent of native AST serialization.

#[path = "../examples/asg_schema_audit/schema.rs"]
#[allow(dead_code)]
mod schema;

use serde_json::json;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn pinned_schema_accepts_structure_without_claiming_semantics() -> Result<(), Error> {
    let (_, validator, _) = schema::load()?;
    let controls = schema::controls(&validator)?;
    assert_eq!(
        controls.get("semantically_wrong_title_is_schema_valid"),
        Some(&json!(true))
    );
    Ok(())
}

#[test]
fn invalid_nested_titles_and_inline_images_are_rejected() -> Result<(), Error> {
    let (_, validator, _) = schema::load()?;
    for content in [
        json!({"name":"paragraph","type":"block","title":"wrong","inlines":[]}),
        json!({"name":"paragraph","type":"block","inlines":[{"name":"image","type":"inline","target":"photo.png"}]}),
    ] {
        let value = json!({"name":"document","type":"block","blocks":[content]});
        let before = value.clone();
        assert!(!validator.is_valid(&value));
        assert_ne!(schema::failures(&validator, &value), []);
        assert_eq!(value, before);
    }
    Ok(())
}

#[test]
fn location_audit_visits_schema_unsupported_content() -> Result<(), Error> {
    let (_, _, validator) = schema::load()?;
    let value = json!({"name":"unsupported","type":"inline","label":[
        {"name":"text","type":"string","value":"unknown"},
        {"location":[{"line":0,"col":0},{"line":0,"col":0}]},
        {"location":[{"line":2,"col":0},{"line":2,"col":1}]}]});
    let mut findings = Vec::new();
    schema::locations(&value, "", &validator, &mut findings);
    let paths = findings
        .iter()
        .map(|finding| finding.get("path"))
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            Some(&json!("")),
            Some(&json!("/label/0")),
            Some(&json!("/label/1/location"))
        ]
    );
    assert!(!validator.is_valid(&json!([{"line":0,"col":0},{"line":1,"col":1}])));
    Ok(())
}

#[path = "../examples/asg_schema_audit/model.rs"]
#[allow(dead_code)]
mod model;

#[test]
fn support_inventory_covers_every_backing_model_property() -> Result<(), Error> {
    use std::collections::BTreeSet;

    let (schema, _, _) = schema::load()?;
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../schemas/asg/support-inventory.json"))?;
    assert_eq!(
        inventory.get("schema_revision"),
        Some(&json!(schema::REVISION))
    );
    assert_eq!(inventory.get("schema_sha256"), Some(&json!(schema::SHA256)));
    let types = inventory
        .get("types")
        .and_then(serde_json::Value::as_object)
        .ok_or("missing type inventory")?;
    let shapes = model::collect()?;
    assert_eq!(types.len(), shapes.len());
    for (name, shape) in shapes {
        let entry = types.get(&name).ok_or_else(|| format!("missing {name}"))?;
        assert_eq!(
            entry.get("shape"),
            Some(&serde_json::to_value(&shape)?),
            "{name}"
        );
        let mut properties = shape.fields.into_iter().collect::<BTreeSet<_>>();
        for (variant, fields) in shape.variants {
            properties.insert(format!("variant:{variant}"));
            properties.extend(fields.into_iter().map(|field| format!("{variant}.{field}")));
        }
        let actual = entry
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .ok_or("missing property inventory")?;
        assert_eq!(
            actual.keys().cloned().collect::<BTreeSet<_>>(),
            properties,
            "{name}"
        );
        for rule in std::iter::once(
            entry
                .get("representation")
                .ok_or("missing representation")?,
        )
        .chain(actual.values())
        {
            let key = rule
                .get("policy")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing policy")?;
            let policy = inventory
                .get("policies")
                .and_then(|policies| policies.get(key))
                .ok_or("unknown policy")?;
            assert!(
                [
                    "mapping-candidate",
                    "context-dependent",
                    "reject",
                    "source-only",
                    "derived-check"
                ]
                .contains(
                    &policy
                        .get("handling")
                        .and_then(serde_json::Value::as_str)
                        .ok_or("missing handling")?
                )
            );
            assert_ne!(
                policy
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("missing reason")?,
                ""
            );
            if let Some(pointer) = rule.get("upstream").and_then(serde_json::Value::as_str) {
                let pointer = pointer
                    .strip_prefix('#')
                    .ok_or("not a schema-local pointer")?;
                assert!(schema.pointer(pointer).is_some(), "{name}: {pointer}");
            }
        }
    }
    Ok(())
}

#[test]
fn every_inventory_class_has_current_strict_export_evidence() -> Result<(), Error> {
    use std::{collections::BTreeSet, path::Path};
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../schemas/asg/support-inventory.json"))?;
    let groups = inventory
        .get("strict_export_groups")
        .and_then(serde_json::Value::as_object)
        .ok_or("missing export groups")?;
    let types = inventory
        .get("types")
        .and_then(serde_json::Value::as_object)
        .ok_or("missing types")?;
    let mut used = BTreeSet::new();
    for (name, entry) in types {
        let group = entry
            .get("strict_export_group")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("unclassified {name}"))?;
        assert!(groups.contains_key(group), "{name}: unknown {group}");
        used.insert(group);
    }
    assert_eq!(used, groups.keys().map(String::as_str).collect());
    for (name, group) in groups {
        assert!(
            !group
                .get("handling")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing handling")?
                .is_empty(),
            "{name}"
        );
        let tests = group
            .get("tests")
            .and_then(serde_json::Value::as_array)
            .ok_or("missing evidence")?;
        assert!(!tests.is_empty(), "{name}");
        for test in tests {
            let file = test
                .get("file")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing test file")?;
            let case = test
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing test name")?;
            let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file))?;
            assert!(
                source.contains(&format!("fn {case}(")),
                "{name}: stale evidence {file}::{case}"
            );
        }
    }
    Ok(())
}

#[test]
fn extension_inventory_names_real_owners_properties_schema_fields_and_tests() -> Result<(), Error> {
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../schemas/asg/support-inventory.json"))?;
    let types = inventory
        .get("types")
        .and_then(serde_json::Value::as_object)
        .ok_or("types")?;
    let profiles = inventory
        .get("extension_profiles")
        .and_then(serde_json::Value::as_object)
        .ok_or("profiles")?;
    assert!(!profiles.is_empty());
    for (id, profile) in profiles {
        let filename = profile
            .get("schema")
            .and_then(serde_json::Value::as_str)
            .ok_or("schema")?;
        let schema: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("schemas/asg").join(filename))?)?;
        assert_eq!(
            schema.get("$id").and_then(serde_json::Value::as_str),
            Some(id.as_str())
        );
        let additions = profile
            .get("additions")
            .and_then(serde_json::Value::as_object)
            .ok_or("additions")?;
        assert!(!additions.is_empty());
        for (name, addition) in additions {
            let owners = addition
                .get("owners")
                .and_then(serde_json::Value::as_object)
                .ok_or("owners")?;
            assert!(!owners.is_empty(), "{name}");
            for (owner, properties) in owners {
                let matches = types
                    .iter()
                    .filter(|(key, _)| key.ends_with(&format!("::{owner}")))
                    .collect::<Vec<_>>();
                let [(_, entry)] = matches.as_slice() else {
                    return Err(format!("{name}: ambiguous or missing owner {owner}").into());
                };
                let properties = properties.as_array().ok_or("properties")?;
                assert!(!properties.is_empty(), "{name}: {owner}");
                for property in properties {
                    let property = property.as_str().ok_or("property")?;
                    assert!(
                        entry
                            .get("properties")
                            .and_then(|fields| fields.get(property))
                            .is_some(),
                        "{name}: {owner}.{property}"
                    );
                }
            }
            let pointers = addition
                .get("schema_pointers")
                .and_then(serde_json::Value::as_array)
                .ok_or("schema pointers")?;
            assert!(!pointers.is_empty(), "{name}");
            for pointer in pointers {
                let pointer = pointer
                    .as_str()
                    .and_then(|pointer| pointer.strip_prefix('#'))
                    .ok_or("local schema pointer")?;
                assert!(schema.pointer(pointer).is_some(), "{name}: {pointer}");
            }
            let tests = addition
                .get("tests")
                .and_then(serde_json::Value::as_array)
                .ok_or("tests")?;
            assert!(!tests.is_empty(), "{name}");
            for test in tests {
                let (file, case) = test
                    .as_str()
                    .and_then(|test| test.split_once("::"))
                    .ok_or("test reference")?;
                let source = std::fs::read_to_string(root.join("tests").join(file))?;
                assert!(
                    source.contains(&format!("fn {case}(")),
                    "{name}: stale test {file}::{case}"
                );
            }
        }
    }
    Ok(())
}
