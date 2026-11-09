use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, ContentRef, DocumentRef, InlineKind, LinkRef, StructuralRef},
    parse, parse_file, parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn links(document: DocumentRef<'_>) -> Vec<LinkRef<'_>> {
    document
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node
                && let BlockKind::Paragraph(paragraph) = block.kind()
            {
                return Some(paragraph.content());
            }
            None
        })
        .flat_map(ContentRef::inlines)
        .filter_map(|inline| {
            if let InlineKind::Link(link) = inline.kind() {
                Some(link)
            } else {
                None
            }
        })
        .collect()
}

#[rstest::rstest]
fn colon_targets_keep_spelling_labels_metadata_and_unicode_spans(
    #[values(
        "a:x",
        "urn:example:target",
        "custom+v1:value",
        "a:",
        "a::x",
        "1:x",
        "-:x",
        "file:///tmp/manual.pdf",
        r"C:\docs\manual.pdf",
        "dir/a:b.html",
        "file.html#part:one",
        "urn:é:東京",
        r"\a:x",
        "a]x:target",
        "a:\"x"
    )]
    target: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let macro_source = format!("link:{target}[*Read*,id=target,role=marked,title=Tip]");
    let source = format!("α {macro_source} ω");
    let parsed = parse_inline(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.content())?;
    let nodes = parsed.content().inlines().collect::<Vec<_>>();
    let [_, inline, _] = nodes.as_slice() else {
        return Err(format!("one link between text: {nodes:?}").into());
    };
    let InlineKind::Link(link) = inline.kind() else {
        return Err(format!("expected link: {inline:?}").into());
    };
    assert_eq!(link.target().to_string(), target);
    assert!(matches!(
        link.text().inlines().next().ok_or("label")?.kind(),
        InlineKind::Bold(_)
    ));
    assert_eq!(link.id().ok_or("id")?.as_str(), "target");
    assert!(link.roles().contains("marked"));
    let InlineKind::Plain(tooltip) = link
        .tooltip()
        .ok_or("tooltip")?
        .inlines()
        .next()
        .ok_or("tooltip text")?
        .kind()
    else {
        return Err("plain tooltip".into());
    };
    assert_eq!(tooltip.text(), "Tip");
    let location = link.location();
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
        Some(macro_source.as_str())
    );
    for (offset, position) in [
        (location.absolute_start(), location.start()),
        (location.absolute_end(), location.end()),
    ] {
        assert_eq!(position.line(), 1);
        assert_eq!(
            position.column() as usize,
            source[..offset].chars().count() + 1
        );
    }
    assert_eq!(serde_json::to_value(parsed.content())?, before);
    Ok(())
}

#[cfg(feature = "asg")]
#[rstest::rstest]
fn colon_targets_export_without_changing_native_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    for (source, target) in [
        ("link:a:x[]", "a:x"),
        (r"link:\file:///tmp/manual.pdf[]", "file:///tmp/manual.pdf"),
        ("link:urn:é:東京[*Read*]", "urn:é:東京"),
    ] {
        let parsed = parse_inline(source, &options)?;
        let before = serde_json::to_value(parsed.content())?;
        for profile in [
            acdc_parser::asg::Profile::Upstream,
            acdc_parser::asg::Profile::Acdc,
        ] {
            let value = acdc_parser::asg::inlines_with_profile(parsed.content(), profile)?;
            assert_eq!(
                value
                    .pointer("/0/target")
                    .and_then(serde_json::Value::as_str),
                Some(target)
            );
            assert_eq!(
                value
                    .pointer("/0/variant")
                    .and_then(serde_json::Value::as_str),
                Some("link")
            );
        }
        assert_eq!(serde_json::to_value(parsed.content())?, before);
    }
    Ok(())
}

#[rstest::rstest]
fn invalid_or_disabled_colon_targets_stay_unlinked(
    #[values(
        "link::x[Label]",
        "link:a:x y[Label]",
        "link:a:x\t[Label]",
        "link:a:x[Unfinished",
        "link:a:x*Label*]",
        "a:x[Label]",
        r"\link:a:x[*Label*]"
    )]
    source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(
        parsed
            .content()
            .inlines()
            .all(|node| node.as_hyperlink().is_none())
    );
    if let Some(unescaped) = source.strip_prefix('\\') {
        let [inline] = &*parsed.content().inlines().collect::<Vec<_>>() else {
            return Err("one escaped macro".into());
        };
        let InlineKind::Plain(plain) = inline.kind() else {
            return Err("escaped macro remains plain".into());
        };
        assert_eq!(plain.text(), unescaped);
        assert_eq!(
            source.get(plain.location().absolute_start()..=plain.location().absolute_end()),
            Some(source)
        );
    }
    Ok(())
}

#[rstest::rstest]
fn colon_targets_honor_macro_order_and_incomplete_nested_labels(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    for label in ["{open}Label]", "[Label{close}", "{open}Label{close}"] {
        for (subs, linked) in [("macros,attributes", false), ("attributes,macros", true)] {
            let source = format!(":open: [\n:close: ]\n\n[subs=\"{subs}\"]\nlink:a:x{label}\n");
            let parsed = parse(&source, &options)?;
            assert_eq!(!links(parsed.document()).is_empty(), linked, "{source}");
        }
    }
    let source = "link:a:x[Before link:urn:valid[Valid]\n";
    let parsed = parse(source, &options)?;
    let actual = links(parsed.document());
    let [link] = actual.as_slice() else {
        return Err("one complete nested link".into());
    };
    assert_eq!(link.target().to_string(), "urn:valid");
    assert_eq!(
        source.get(link.location().absolute_start()..=link.location().absolute_end()),
        Some("link:urn:valid[Valid]")
    );
    Ok(())
}

#[rstest::rstest]
#[case("  α link:urn:é:東京[*Read*]\n", 2, 5)]
#[case("  [cols=a]\n  |===\n  |\n  α link:urn:é:東京[*Read*]\n  |===\n", 5, 5)]
fn included_colon_targets_keep_original_locations_in_paragraphs_and_cells(
    #[case] body: &str,
    #[case] line: u32,
    #[case] column: u32,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let included = directory.path().join("body.adoc");
    std::fs::write(
        &main,
        format!(
            "include::body.adoc[lines=2..{},indent=0]\n",
            body.lines().count() + 1
        ),
    )?;
    std::fs::write(&included, format!("// skipped\n{body}"))?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let actual = links(parsed.document());
    let [link] = actual.as_slice() else {
        return Err("one included link".into());
    };
    assert_eq!(link.target().to_string(), "urn:é:東京");
    let origin = parsed.resolve_location(link.location());
    assert_eq!(origin.file.as_ref(), Some(&included));
    assert_eq!(origin.location.start.line, line);
    assert_eq!(origin.location.start.column, column);
    assert_eq!(origin.location.end.line, line);
    assert_eq!(
        origin.location.end.column,
        column + u32::try_from("link:urn:é:東京[*Read*]".chars().count())? - 1
    );
    Ok(())
}

#[rstest::rstest]
fn repeated_incomplete_link_prefixes_preserve_formatting(
    #[values("", "[unfinished", r"[escaped\]", "[image:icon.png[Alt]")] suffix: &str,
    #[values(false, true)] formatted: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{}{suffix}", "link:".repeat(512));
    let source = if formatted {
        format!("**{source}**")
    } else {
        source
    };
    let parsed = parse_inline(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let content = if formatted {
        let [inline] = &*parsed.content().inlines().collect::<Vec<_>>() else {
            return Err("one formatted span".into());
        };
        let InlineKind::Bold(span) = inline.kind() else {
            return Err("preserved outer formatting".into());
        };
        span.content()
    } else {
        parsed.content()
    };
    assert!(
        content
            .inlines()
            .all(|inline| !matches!(inline.kind(), InlineKind::Link(_)))
    );
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn failed_target_ranges_do_not_hide_later_complete_links(
    #[values(" ", "[unfinished ")] separator: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{}{separator}link:urn:valid[*Valid*]", "link:".repeat(512));
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let actual = links(parsed.document());
    let [link] = actual.as_slice() else {
        return Err("one complete link after the failed target".into());
    };
    assert_eq!(link.target().to_string(), "urn:valid");
    assert_eq!(
        source.get(link.location().absolute_start()..=link.location().absolute_end()),
        Some("link:urn:valid[*Valid*]")
    );
    Ok(())
}
