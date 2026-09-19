use acdc_parser::{AttributeValue, DocumentAttributes, Options, parse};

type Error = Box<dyn std::error::Error>;

fn documented_attributes() -> Result<Vec<(&'static str, String)>, std::io::Error> {
    let mut attributes = Vec::new();
    for (index, line) in include_str!("../fixtures/document_attributes/policy.tsv")
        .lines()
        .enumerate()
    {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((policy, name)) = line.split_once('\t') else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid policy fixture line {}", index + 1),
            ));
        };
        attributes.push((policy, name.replace('*', "probe")));
    }
    Ok(attributes)
}

#[test]
fn caller_attribute_cannot_be_replaced_or_unset_by_document_entries() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("locked", "caller")
        .build();
    let parsed = parse(
        "= T\n:locked: header\n:locked!:\n\nBefore.\n\n:locked: body\n:locked!:\n\n{locked}\n",
        &options,
    )?;

    assert_eq!(
        parsed.document().attributes.get("locked"),
        Some(&AttributeValue::String("caller".into()))
    );
    Ok(())
}

#[test]
fn caller_unset_attribute_cannot_be_set_by_document_entries() -> Result<(), Error> {
    let mut attributes = DocumentAttributes::default();
    attributes.set("experimental".into(), AttributeValue::None);
    let parsed = parse(
        "= T\n:experimental:\n\nBefore.\n\n:experimental:\n\nkbd:[Ctrl+C]\n",
        &Options::with_attributes(attributes),
    )?;

    assert_eq!(
        parsed.document().attributes.get("experimental"),
        Some(&AttributeValue::None)
    );
    Ok(())
}

#[test]
fn processor_defaults_added_after_options_are_document_overridable() -> Result<(), Error> {
    let mut attributes = DocumentAttributes::default();
    attributes.set("default-name".into(), "processor".into());
    let mut options = Options::builder().build();
    options.document_attributes.merge(attributes);
    let parsed = parse(":default-name: document\n\n{default-name}\n", &options)?;

    assert_eq!(
        parsed.document().attributes.get("default-name"),
        Some(&AttributeValue::String("document".into()))
    );
    Ok(())
}

#[test]
fn every_documented_attribute_from_options_is_locked() -> Result<(), Error> {
    for (_, name) in documented_attributes()?
        .into_iter()
        .filter(|(_, name)| name != "sectnums")
    {
        let options = Options::builder()
            .with_attribute(name.clone(), "caller")
            .build();
        let source = format!(
            ":{name}: header\n:{name}!:\n\nBefore.\n\n:{name}: body\n:{name}!:\n\nAfter.\n"
        );
        let parsed = parse(&source, &options)?;

        assert_eq!(
            parsed.document().attributes.get(&name),
            Some(&AttributeValue::String("caller".into())),
            "{name}"
        );

        if name != "max-include-depth" {
            let options = Options::builder().with_attribute(name.clone(), ()).build();
            let source = format!(":{name}: header\n\n:{name}: body\n");
            let parsed = parse(&source, &options)?;

            assert_eq!(
                parsed.document().attributes.get(&name),
                Some(&AttributeValue::None),
                "hard unset {name}"
            );
        }
    }
    Ok(())
}

#[test]
fn documented_modifiable_attributes_accept_document_values() -> Result<(), Error> {
    for (policy, name) in documented_attributes()?
        .into_iter()
        .filter(|(policy, _)| matches!(*policy, "header" | "body"))
    {
        let source = if policy == "header" {
            format!(":{name}: document\n\nContent.\n")
        } else {
            format!("Content.\n\n:{name}: document\n")
        };
        let parsed = parse(&source, &Options::default())?;

        assert_eq!(
            parsed.document().attributes.get(&name),
            Some(&AttributeValue::String("document".into())),
            "{name}"
        );

        let source = if policy == "header" {
            format!(":{name}: document\n:{name}!:\n\nContent.\n")
        } else {
            format!("Content.\n\n:{name}: document\n:{name}!:\n")
        };
        let parsed = parse(&source, &Options::default())?;

        assert_eq!(
            parsed.document().attributes.get(&name),
            Some(&AttributeValue::Bool(false)),
            "document unset {name}"
        );
    }
    Ok(())
}

#[test]
fn documented_read_only_and_api_only_attributes_ignore_document_values() -> Result<(), Error> {
    for (_, name) in documented_attributes()?
        .into_iter()
        .filter(|(policy, _)| matches!(*policy, "read_only" | "api_only"))
    {
        let source = format!(":{name}: document\n\n:{name}: body\n");
        let parsed = parse(&source, &Options::default())?;

        assert_ne!(
            parsed.document().attributes.get(&name),
            Some(&AttributeValue::String("document".into())),
            "{name}"
        );
        assert_ne!(
            parsed.document().attributes.get(&name),
            Some(&AttributeValue::String("body".into())),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn caller_set_sectnums_is_flexible_after_the_header() -> Result<(), Error> {
    let options = Options::builder().with_attribute("sectnums", true).build();
    let parsed = parse(
        "= T\n:sectnums!:\n\n== One\n\n:sectnums!:\n\n== Two\n\n:sectnums:\n\n== Three\n",
        &options,
    )?;
    let numbers: Vec<_> = parsed
        .document()
        .toc_entries
        .iter()
        .map(acdc_parser::TocEntry::number)
        .collect();

    assert_eq!(numbers, [Some("1"), None, Some("2")]);
    Ok(())
}

#[test]
fn caller_unset_sectnums_remains_locked() -> Result<(), Error> {
    let options = Options::builder().with_attribute("sectnums", ()).build();
    let parsed = parse("= T\n\n:sectnums:\n\n== One\n", &options)?;

    assert_eq!(
        parsed
            .document()
            .toc_entries
            .first()
            .map(acdc_parser::TocEntry::number),
        Some(None),
        "a hard caller unset remains locked"
    );
    Ok(())
}
