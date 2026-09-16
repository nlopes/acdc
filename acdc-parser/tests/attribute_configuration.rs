use acdc_parser::{AttributeValue, DocumentAttributeValue, Options, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn processor_convenience_flags_use_final_defaults(
    #[values(false, true)] bulk_defaults: bool,
) -> Result<(), Error> {
    let defaults = [
        ("backend", "html5"),
        ("basebackend", "html"),
        ("filetype", "html"),
        ("doctype", "book"),
        ("backend-old", ""),
        ("backend-html5-doctype-article", ""),
        ("basebackend-old", ""),
        ("basebackend-html-doctype-article", ""),
        ("filetype-old", ""),
        ("doctype-article", ""),
    ];
    let builder = Options::builder()
        .with_attribute("backend-forged", "")
        .with_attribute("outfilesuffix", ".custom");
    let options = if bulk_defaults {
        builder.with_defaults(defaults)
    } else {
        defaults
            .into_iter()
            .fold(builder, |builder, (name, value)| {
                builder.with_default_attribute(name, value)
            })
    }
    .build()?;
    let attributes = options.document_attributes();
    for name in [
        "backend-html5",
        "backend-html5-doctype-book",
        "basebackend-html",
        "basebackend-html-doctype-book",
        "filetype-html",
        "doctype-book",
    ] {
        assert!(attributes.contains_key(name), "missing {name}");
    }
    for name in [
        "backend-old",
        "backend-forged",
        "backend-html5-doctype-article",
        "basebackend-old",
        "basebackend-html-doctype-article",
        "filetype-old",
        "doctype-article",
    ] {
        assert!(!attributes.contains_key(name), "stale {name}");
    }
    assert_eq!(
        attributes
            .get("outfilesuffix")
            .and_then(|value| value.text()),
        Some(".custom")
    );
    Ok(())
}

#[test]
fn reopening_options_preserves_defaults_overrides_and_parse_settings() -> Result<(), Error> {
    let options = Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Safe)
        .with_strict()
        .with_defaults([("name", "default")])
        .with_attributes([("locked", "caller"), ("max-include-depth", "064")])
        .build()?;
    let options = options
        .into_builder()
        .with_attribute("locked", "replacement")
        .build()?;
    assert_eq!(options.safe_mode, acdc_parser::SafeMode::Safe);
    assert!(options.strict);
    let parsed = parse(":name: document\n:locked: document\n\nContent.\n", &options)?;
    let attributes = &parsed.document().attributes;
    assert_eq!(
        attributes
            .get("name")
            .and_then(DocumentAttributeValue::as_str),
        Some("document")
    );
    assert_eq!(
        attributes
            .get("locked")
            .and_then(DocumentAttributeValue::as_str),
        Some("replacement")
    );
    assert_eq!(
        attributes
            .get("max-include-depth")
            .and_then(DocumentAttributeValue::text),
        Some("064")
    );
    Ok(())
}

#[test]
fn configuration_is_validated_before_parsing() -> Result<(), Error> {
    let builder = Options::builder().with_attribute("max-include-depth", "invalid");
    assert!(matches!(
        builder.clone().build(),
        Err(acdc_parser::Error::InvalidDocumentAttribute { location: None, .. })
    ));

    let options = builder.with_attribute("max-include-depth", "064").build()?;
    assert_eq!(
        options
            .document_attributes()
            .get("max-include-depth")
            .and_then(|value| value.text()),
        Some("064")
    );
    assert_eq!(
        options
            .document_attributes()
            .get("max-include-depth")
            .and_then(DocumentAttributeValue::as_integer),
        Some(64)
    );
    Ok(())
}

#[test]
fn replacing_inputs_discards_invalid_values() -> Result<(), Error> {
    let inputs = [("name", "caller")];
    let options = Options::builder()
        .with_attribute("max-include-depth", "invalid")
        .with_attributes(inputs)
        .build()?;
    let parsed = parse(":name: document\n\n{name}\n", &options)?;
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("name")
            .and_then(|value| value.text()),
        Some("caller")
    );
    assert!(
        !options
            .document_attributes()
            .is_explicit("max-include-depth")
    );
    Ok(())
}

#[test]
fn defaults_have_the_same_precedence_in_either_builder_order() -> Result<(), Error> {
    let defaults = [
        ("name", "default"),
        ("other", "default"),
        ("backend", "pdf"),
    ];
    let options = [
        Options::builder()
            .with_defaults(defaults)
            .with_attribute("name", "caller")
            .with_attribute("backend", "spoofed")
            .build()?,
        Options::builder()
            .with_attribute("name", "caller")
            .with_attribute("backend", "spoofed")
            .with_defaults(defaults)
            .build()?,
    ];
    for options in options {
        let parsed = parse(
            ":name: document\n:other: document\n:backend: spoofed\n\nContent.\n",
            &options,
        )?;
        assert_eq!(
            parsed
                .document()
                .attributes
                .get("name")
                .and_then(|value| value.text()),
            Some("caller")
        );
        assert_eq!(
            parsed
                .document()
                .attributes
                .get("other")
                .and_then(|value| value.text()),
            Some("document")
        );
        assert_eq!(
            parsed
                .document()
                .attributes
                .get("backend")
                .and_then(|value| value.text()),
            Some("pdf")
        );
    }
    Ok(())
}

#[test]
fn input_removal_restores_defaults_but_unset_masks_them() -> Result<(), Error> {
    let mut inputs = std::collections::HashMap::from([("figure-caption", AttributeValue::None)]);
    assert!(inputs.contains_key("figure-caption"));
    assert_eq!(
        Options::builder()
            .with_attributes(inputs.clone())
            .with_defaults([("figure-caption", "Custom")])
            .build()?
            .document_attributes()
            .get("figure-caption"),
        None
    );

    assert_eq!(inputs.remove("figure-caption"), Some(AttributeValue::None));
    assert_eq!(
        Options::with_attributes(inputs)?
            .document_attributes()
            .get("figure-caption")
            .and_then(|value| value.text()),
        Some("Figure")
    );
    Ok(())
}

#[test]
fn rebuilding_inputs_keeps_values_but_not_previous_assignment_policy() -> Result<(), Error> {
    let parsed = parse(
        ":name: document\n:figure-caption!:\n\nContent.\n",
        &Options::default(),
    )?;
    let inputs = parsed.document().attributes.to_static().into_inputs();
    let options = Options::with_attributes(inputs)?;
    let reparsed = parse(
        ":name: replacement\n:figure-caption: Replacement\n\nContent.\n",
        &options,
    )?;
    assert_eq!(
        reparsed
            .document()
            .attributes
            .get("name")
            .and_then(|value| value.text()),
        Some("document")
    );
    assert_eq!(reparsed.document().attributes.get("figure-caption"), None);
    Ok(())
}

#[test]
fn reusing_a_snapshot_does_not_turn_universal_defaults_into_overrides() -> Result<(), Error> {
    let parsed = parse("", &Options::default())?;
    let inputs = parsed.document().attributes.to_static().into_inputs();
    let options = Options::with_attributes(inputs)?;
    assert!(!options.document_attributes().is_explicit("figure-caption"));
    let parsed = parse(":figure-caption: Custom\n\nContent.\n", &options)?;
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("figure-caption")
            .and_then(|value| value.text()),
        Some("Custom")
    );
    Ok(())
}

#[test]
fn building_inputs_retains_owned_text_allocations() -> Result<(), Error> {
    let name = String::from("custom-name");
    let value = String::from("custom-value");
    let name_address = name.as_ptr();
    let value_address = value.as_ptr();
    let options = Options::with_attributes([(name, value)])?;
    let (name, value) = options
        .document_attributes()
        .iter()
        .find(|(name, _)| *name == "custom-name")
        .ok_or("missing input")?;
    let value = value.as_str().ok_or("expected text")?;
    assert_eq!(name.as_ptr(), name_address);
    assert_eq!(value.as_ptr(), value_address);
    Ok(())
}
