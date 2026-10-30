use acdc_parser::{
    CompatibilityMode, InlineMacro, InlineNode, Link, Location, Mailto, Options, SafeMode, Source,
    Url,
    ast::{
        AutolinkRef, BlockKind, ElementIdRef, HyperlinkRef, InlineKind, InlineRef, LinkRef,
        MailtoRef, UrlRef,
    },
    parse, parse_file, parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn shared<T: Copy + Send + Sync>(_: T) {}

#[rstest::rstest]
#[case("link:guide.adoc[*Read*]", "guide.adoc", "link", false)]
#[case("https://example.org[*Read*]", "https://example.org", "url", false)]
#[case(
    "mailto:team@example.org[*Read*]",
    "mailto:team@example.org",
    "mailto",
    false
)]
#[case("link:https://example.org[]", "https://example.org", "link", true)]
#[case("https://example.org[]", "https://example.org", "url", true)]
#[case("mailto:team@example.org[]", "mailto:team@example.org", "mailto", true)]
#[case("https://example.org", "https://example.org", "bare", true)]
#[case("<https://example.org>", "https://example.org", "angle", true)]
#[case("team@example.org", "mailto:team@example.org", "bare", true)]
#[case("<team@example.org>", "mailto:team@example.org", "angle", true)]
fn common_reads_preserve_targets_labels_and_source_forms(
    #[case] source: &str,
    #[case] target: &str,
    #[case] form: &str,
    #[case] automatic: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.inlines())?;
    let [owner] = parsed.inlines() else {
        return Err("one hyperlink".into());
    };
    let inline = InlineRef::from(owner);
    let link = inline.as_hyperlink().ok_or("hyperlink")?;
    shared(link);
    assert_eq!(link.target().to_string(), target);
    assert_eq!(link.text().is_empty(), automatic);
    if !automatic {
        assert!(matches!(
            link.text().inlines().next().ok_or("label")?.kind(),
            InlineKind::Bold(_)
        ));
    }
    let actual = if let HyperlinkRef::Link(concrete) = link {
        assert_eq!(concrete.text(), link.text());
        "link"
    } else if let HyperlinkRef::Url(concrete) = link {
        assert_eq!(concrete.text(), link.text());
        "url"
    } else if let HyperlinkRef::Mailto(concrete) = link {
        assert_eq!(concrete.text(), link.text());
        assert_eq!(concrete.subject(), None);
        assert_eq!(concrete.body(), None);
        "mailto"
    } else if let HyperlinkRef::Autolink(concrete) = link {
        assert_eq!(concrete.target().to_string(), target);
        assert!(!concrete.hides_uri_scheme());
        if concrete.is_bracketed() {
            "angle"
        } else {
            "bare"
        }
    } else {
        return Err("hyperlink form".into());
    };
    assert_eq!(actual, form);
    assert_eq!(
        link.location().absolute_start(),
        inline.location().absolute_start()
    );
    assert_eq!(
        source.get(link.location().absolute_start()..=link.location().absolute_end()),
        Some(source)
    );
    assert_eq!(serde_json::to_value(parsed.inlines())?, before);
    Ok(())
}

#[rstest::rstest]
fn common_reads_keep_mail_payload_and_metadata_on_the_concrete_form(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for (source, subject, body) in [
        (
            "mailto:team@example.org[Team,Hello & café,Body + text,id=mail,role=contact,title=Tip,window=reading,custom=kept]",
            Some("Hello & café"),
            Some("Body + text"),
        ),
        ("mailto:team@example.org[,\"\",\"\"]", Some(""), Some("")),
        ("mailto:team@example.org[]", None, None),
    ] {
        let parsed = parse_inline(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let link = InlineRef::from(parsed.inlines().first().ok_or("inline")?)
            .as_hyperlink()
            .ok_or("hyperlink")?;
        let HyperlinkRef::Mailto(mail) = link else {
            return Err("mail".into());
        };
        assert_eq!(link.target().to_string(), "mailto:team@example.org");
        assert_eq!(mail.subject(), subject);
        assert_eq!(mail.body(), body);
        if source.contains("id=mail") {
            assert_eq!(mail.id().map(ElementIdRef::as_str), Some("mail"));
            assert!(mail.roles().contains("contact"));
            assert!(mail.tooltip().is_some());
            assert_eq!(mail.window(), Some("reading"));
            assert!(mail.attributes().contains("custom"));
        }
    }
    Ok(())
}

#[rstest::rstest]
fn display_policy_is_captured_per_occurrence_without_changing_the_target(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let links = "link:https://example.org[] https://example.org[] https://example.org <https://example.org> team@example.org <team@example.org>";
    let source =
        format!("{links}\n\n:hide-uri-scheme:\n\n{links}\n\n:hide-uri-scheme!:\n\n{links}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut groups = Vec::new();
    for block in parsed.document_ref().blocks() {
        let BlockKind::Paragraph(paragraph) = block.kind() else {
            continue;
        };
        let mut flags = Vec::new();
        for link in paragraph
            .content()
            .inlines()
            .filter_map(InlineRef::as_hyperlink)
        {
            assert!(link.text().is_empty());
            assert!(
                link.target().to_string().starts_with("https:")
                    || link.target().to_string().starts_with("mailto:")
            );
            let hidden = if let HyperlinkRef::Link(link) = link {
                link.hides_uri_scheme()
            } else if let HyperlinkRef::Url(link) = link {
                link.hides_uri_scheme()
            } else if let HyperlinkRef::Autolink(link) = link {
                link.hides_uri_scheme()
            } else {
                return Err("unexpected mail macro".into());
            };
            flags.push(hidden);
        }
        groups.push(flags);
    }
    assert_eq!(groups, [vec![false; 6], vec![true; 6], vec![false; 6]]);
    Ok(())
}

#[test]
fn typed_owner_conversions_and_xref_exclusion_keep_distinct_contracts() -> Result<(), Error> {
    let link = Link::new(Source::Name("guide"), Location::default());
    let url = Url::new(Source::Name("target"), Location::default());
    let mail = Mailto::new(Source::Name("team"), Location::default());
    for link in [
        HyperlinkRef::from(LinkRef::from(&link)),
        HyperlinkRef::from(UrlRef::from(&url)),
        HyperlinkRef::from(MailtoRef::from(&mail)),
    ] {
        assert!(link.text().is_empty());
    }
    let parsed = parse_inline("<team@example.org>", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::Autolink(owner))] = parsed.inlines() else {
        return Err("autolink".into());
    };
    let concrete = AutolinkRef::from(owner);
    assert!(concrete.is_bracketed());
    let link = HyperlinkRef::from(concrete);
    assert_eq!(link.target().to_string(), "mailto:team@example.org");
    let content = {
        let copy = link;
        copy.text().inlines()
    };
    assert_eq!(content.count(), 0);

    let parsed = parse_inline(
        "xref:part[] xref:guide.adoc#part[] image:a.png[] text",
        &Options::default(),
    )?;
    assert!(
        parsed
            .inlines()
            .iter()
            .all(|inline| InlineRef::from(inline).as_hyperlink().is_none())
    );
    assert_eq!(
        parsed
            .inlines()
            .iter()
            .filter(|inline| matches!(
                InlineRef::from(*inline).kind(),
                InlineKind::CrossReference(_)
            ))
            .count(),
        2
    );
    Ok(())
}

#[rstest::rstest]
fn shared_reads_keep_unicode_include_and_table_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("links.adoc");
    std::fs::write(
        &main,
        "include::links.adoc[]\n\n[cols=a]\n|===\n|é <team@example.org>\n|===\n",
    )?;
    std::fs::write(&include, "é https://example.org\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Paragraph(paragraph) = blocks.next().ok_or("paragraph")?.kind() else {
        return Err("paragraph".into());
    };
    let link = paragraph
        .content()
        .inlines()
        .find_map(InlineRef::as_hyperlink)
        .ok_or("included link")?;
    let location = parsed.resolve_location(link.location());
    assert_eq!(location.file, Some(include));
    assert_eq!(location.location.start.column, 3);
    assert_eq!(link.target().to_string(), "https://example.org");
    let BlockKind::Table(table) = blocks.next().ok_or("table")?.kind() else {
        return Err("table".into());
    };
    let cell = table
        .rows()
        .next()
        .ok_or("row")?
        .cells()
        .next()
        .ok_or("cell")?;
    let BlockKind::Paragraph(paragraph) = cell.blocks().next().ok_or("paragraph")?.kind() else {
        return Err("paragraph".into());
    };
    let link = paragraph
        .content()
        .inlines()
        .find_map(InlineRef::as_hyperlink)
        .ok_or("cell link")?;
    assert_eq!(parsed.resolve_location(link.location()).file, Some(main));
    let HyperlinkRef::Autolink(autolink) = link else {
        return Err("cell autolink".into());
    };
    assert!(autolink.is_bracketed());
    assert_eq!(autolink.target().to_string(), "mailto:team@example.org");
    Ok(())
}
