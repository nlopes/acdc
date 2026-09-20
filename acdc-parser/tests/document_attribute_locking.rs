use acdc_parser::{AttributeValue, Block, DocumentAttributeValue, Options, parse};

type Error = Box<dyn std::error::Error>;

fn documented_attributes() -> Vec<(&'static str, String)> {
    [
        ("header", "
            backend docdate docdatetime doctime docyear localdate localdatetime localtime localyear
            outfilesuffix experimental reproducible lang last-update-label manname-title nolang
            toc-title untitled-label version-label app-name author authorinitials authors copyright
            doctitle description email firstname keywords lastname middlename orgname revdate revremark
            revnumber title title-separator toc toclevels fragment asset-uri-scheme cache-uri data-uri
            docinfo docinfodir docinfosubs doctype eqnums media nofooter nofootnotes noheader notitle
            pagewidth showtitle stem webfonts iconfont-cdn iconfont-name iconfont-remote icons
            coderay-css coderay-unavailable highlightjsdir highlightjs-theme prettifydir prettify-theme
            pygments-css pygments-style pygments-unavailable rouge-css rouge-style rouge-unavailable
            source-highlighter copycss css-signature linkcss max-width stylesdir stylesheet toc-class
            mantitle manvolnum manname manpurpose man-linkstyle mansource manmanual
        "),
        ("read_only", "
            backend-* basebackend basebackend-* doctype-* embedded filetype-* htmlsyntax outdir outfile
            safe-mode-level safe-mode-name safe-mode-unsafe safe-mode-safe safe-mode-server
            safe-mode-secure user-home asciidoctor asciidoctor-version
        "),
        ("api_only", "
            docdir docfile docfilesuffix docname filetype skip-front-matter allow-uri-read
            max-attribute-value-size max-include-depth
        "),
        ("body", "
            attribute-missing attribute-undefined compat-mode appendix-caption appendix-number
            appendix-refsig caution-caption chapter-number chapter-refsig chapter-signifier
            example-caption example-number figure-caption figure-number footnote-number
            important-caption listing-caption listing-number note-caption part-refsig part-signifier
            preface-title section-refsig table-caption table-number tip-caption warning-caption
            front-matter idprefix idseparator leveloffset partnums sectanchors sectids sectlinks
            sectnums sectnumlevels hardbreaks-option hide-uri-scheme relfileprefix relfilesuffix
            show-link-uri table-frame table-grid table-stripes tabsize xrefstyle iconsdir icontype
            imagesdir coderay-linenums-mode prewrap pygments-linenums-mode rouge-linenums-mode
            source-indent source-language source-linenums-option blank empty sp nbsp zwsp wj apos quot
            lsquo rsquo ldquo rdquo deg plus brvbar vbar amp lt gt startsb endsb caret asterisk tilde
            backslash backtick two-colons two-semicolons cpp cxx pp
        "),
    ]
    .into_iter()
    .flat_map(|(policy, names)| {
        names.split_whitespace().map(move |name| (policy, name.replace('*', "probe")))
    })
    .collect()
}

#[test]
fn caller_attribute_cannot_be_replaced_or_unset_by_document_entries() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("locked", "caller")
        .build()?;
    let parsed = parse(
        "= T\n:locked: header\n:locked!:\n\nBefore.\n\n:locked: body\n:locked!:\n\n{locked}\n",
        &options,
    )?;

    assert_eq!(
        parsed
            .document()
            .attributes
            .get("locked")
            .and_then(|value| value.text()),
        Some("caller")
    );
    Ok(())
}

#[test]
fn caller_unset_attribute_cannot_be_set_by_document_entries() -> Result<(), Error> {
    let mut attributes = std::collections::HashMap::<
        std::borrow::Cow<'_, str>,
        acdc_parser::AttributeValue<'_>,
    >::new();
    attributes.insert("experimental".into(), AttributeValue::None);
    let parsed = parse(
        "= T\n:experimental:\n\nBefore.\n\n:experimental:\n\nkbd:[Ctrl+C]\n",
        &Options::with_attributes(attributes)?,
    )?;

    assert!(parsed.document().attributes.is_explicit("experimental"));
    assert_eq!(parsed.document().attributes.get("experimental"), None);
    Ok(())
}

#[test]
fn processor_defaults_are_document_overridable() -> Result<(), Error> {
    let mut attributes = std::collections::HashMap::<
        std::borrow::Cow<'_, str>,
        acdc_parser::AttributeValue<'_>,
    >::new();
    attributes.insert("default-name".into(), "processor".into());
    let options = Options::builder().with_defaults(attributes).build()?;
    let parsed = parse(":default-name: document\n\n{default-name}\n", &options)?;

    assert_eq!(
        parsed
            .document()
            .attributes
            .get("default-name")
            .and_then(|value| value.text()),
        Some("document")
    );
    Ok(())
}

#[test]
fn every_documented_attribute_from_options_is_locked() -> Result<(), Error> {
    for (_, name) in documented_attributes()
        .into_iter()
        .filter(|(_, name)| name != "sectnums")
    {
        let caller_value = if name == "max-include-depth" {
            "1"
        } else {
            "caller"
        };
        let options = Options::builder()
            .with_attribute(name.clone(), caller_value)
            .build()?;
        let baseline = parse("", &options)?;
        let baseline_effective = baseline.document().attributes.get(&name);
        let source = format!(
            ":{name}: header\n:{name}!:\n\nBefore.\n\n:{name}: body\n:{name}!:\n\nAfter.\n"
        );
        let parsed = parse(&source, &options)?;

        assert_eq!(
            parsed.document().attributes.get(&name),
            baseline_effective,
            "effective {name}"
        );

        if name != "max-include-depth" {
            let options = Options::builder()
                .with_attribute(name.clone(), ())
                .build()?;
            let baseline = parse("", &options)?;
            let baseline_effective = baseline.document().attributes.get(&name);
            let source = format!(":{name}: header\n\n:{name}: body\n");
            let parsed = parse(&source, &options)?;

            assert_eq!(
                parsed.document().attributes.get(&name),
                baseline_effective,
                "effective hard unset {name}"
            );
        }
    }
    Ok(())
}

#[test]
fn documented_modifiable_attributes_accept_document_values() -> Result<(), Error> {
    for (policy, name) in documented_attributes()
        .into_iter()
        .filter(|(policy, _)| matches!(*policy, "header" | "body"))
    {
        let baseline = parse("", &Options::default())?;
        let baseline_text = baseline
            .document()
            .attributes
            .get(&name)
            .and_then(|value| value.text());
        let source = if policy == "header" {
            format!(":{name}: document\n\nContent.\n")
        } else {
            format!("Content.\n\n:{name}: document\n")
        };
        let parsed = parse(&source, &Options::default())?;

        if name == "toc" {
            assert!(
                parsed
                    .document()
                    .attributes
                    .get(&name)
                    .is_some_and(DocumentAttributeValue::is_presence)
            );
        } else {
            assert_eq!(
                parsed
                    .document()
                    .attributes
                    .get(&name)
                    .and_then(|value| value.text()),
                if policy == "header" {
                    Some("document")
                } else {
                    baseline_text
                },
                "{name}"
            );
        }
        if policy == "body" {
            assert!(parsed.document().blocks.iter().any(|block| {
                matches!(
                    block,
                    Block::DocumentAttribute(attribute)
                        if attribute.name == name && attribute.assignment().value().and_then(acdc_parser::DocumentAttributeValue::text) == Some("document")
                )
            }));
        }

        let source = if policy == "header" {
            format!(":{name}: document\n:{name}!:\n\nContent.\n")
        } else {
            format!("Content.\n\n:{name}: document\n:{name}!:\n")
        };
        let parsed = parse(&source, &Options::default())?;

        if policy == "header" {
            assert!(
                parsed.document().attributes.is_explicit(&name),
                "document unset {name}"
            );
            assert_eq!(parsed.document().attributes.get(&name), None, "{name}");
        } else {
            assert_eq!(
                parsed
                    .document()
                    .attributes
                    .get(&name)
                    .and_then(|value| value.text()),
                baseline_text,
                "{name}"
            );
            assert!(matches!(
                parsed.document().blocks.last(),
                Some(Block::DocumentAttribute(attribute))
                    if attribute.name == name
                        && matches!(
                            attribute.assignment(),
                            acdc_parser::DocumentAttributeAssignment::Unset
                        )
            ));
        }
    }
    Ok(())
}

#[test]
fn documented_header_attributes_keep_the_header_snapshot_after_body_values() -> Result<(), Error> {
    for (_, name) in documented_attributes()
        .into_iter()
        .filter(|(policy, _)| *policy == "header")
    {
        let source = format!(":{name}: header\n\nBefore.\n\n:{name}: body\n:{name}!:\n\nAfter.\n");
        let parsed = parse(&source, &Options::default())?;

        if name == "toc" {
            assert!(
                parsed
                    .document()
                    .attributes
                    .get(&name)
                    .is_some_and(DocumentAttributeValue::is_presence)
            );
        } else {
            assert_eq!(
                parsed
                    .document()
                    .attributes
                    .get(&name)
                    .and_then(|value| value.text()),
                Some("header"),
                "{name}"
            );
        }
        let events: Vec<_> = parsed
            .document()
            .blocks
            .iter()
            .filter_map(|block| {
                let Block::DocumentAttribute(attribute) = block else {
                    return None;
                };
                (attribute.name == name).then_some(attribute)
            })
            .collect();
        assert!(
            matches!(
                events.as_slice(),
                [set, unset]
                    if set.assignment().value().and_then(acdc_parser::DocumentAttributeValue::text) == Some("body")
                        && matches!(
                            unset.assignment(),
                            acdc_parser::DocumentAttributeAssignment::Unset
                        )
            ),
            "body events for {name}: {events:?}"
        );
    }
    Ok(())
}

#[test]
fn documented_read_only_and_api_only_attributes_ignore_document_values() -> Result<(), Error> {
    for (_, name) in documented_attributes()
        .into_iter()
        .filter(|(policy, _)| matches!(*policy, "read_only" | "api_only"))
    {
        let source = format!(":{name}: document\n:{name}!:\n\nText.\n\n:{name}: body\n:{name}!:\n");
        let parsed = parse(&source, &Options::default())?;

        assert_ne!(
            parsed
                .document()
                .attributes
                .get(&name)
                .and_then(|value| value.text()),
            Some("document"),
            "{name}"
        );
        assert_ne!(
            parsed
                .document()
                .attributes
                .get(&name)
                .and_then(|value| value.text()),
            Some("body"),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn caller_set_sectnums_is_flexible_after_the_header() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("sectnums", true)
        .build()?;
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
    let options = Options::builder().with_attribute("sectnums", ()).build()?;
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
