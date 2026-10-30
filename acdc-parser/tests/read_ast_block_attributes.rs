use acdc_parser::{
    Caption, CompatibilityMode, ContentAttribute, InlineImage, Location, Options, Paragraph,
    SafeMode, Source, Substitution, SubstitutionOp, SubstitutionSpec,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, HeaderRef,
        InlineImageRef, InlineKind, ParagraphRef, SubstitutionEntryRef, SubstitutionSpecKind,
        SubstitutionSpecRef,
    },
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn header_metadata_and_document_assignments_stay_distinct() -> Result<(), Error> {
    let parsed = parse(
        "= Document\n:reftext: document value\n\nBody.",
        &Options::default(),
    )?;
    let mut header = acdc_parser::Header::new(acdc_parser::Title::default(), Location::default());
    header.metadata.reftext = Some(ContentAttribute::default());
    header.metadata.attribution = Some(ContentAttribute::default());
    header.metadata.citetitle = Some(ContentAttribute::default());
    header.metadata.caption = Some(Caption::CustomContent(ContentAttribute::default()));
    header.metadata.substitutions = Some(SubstitutionSpec::Modifiers(Vec::new()));
    let attributes = HeaderRef::from(&header).attributes();
    inventory(attributes);
    for name in ["reftext", "attribution", "citetitle", "caption"] {
        let Some(AttributeValueRef::Content(content)) = attributes.get(name) else {
            return Err("header content".into());
        };
        assert!(content.is_empty());
    }
    assert!(attributes.contains("subs"));
    assert_eq!(
        parsed
            .document_ref()
            .attributes()
            .get("reftext")
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some("document value")
    );
    Ok(())
}

fn attributes(block: BlockRef<'_>) -> Option<AttributesRef<'_>> {
    macro_rules! read {
        ($($kind:ident),+) => {
            match block.kind() {
                $(BlockKind::$kind(node) => Some(node.attributes()),)+
                BlockKind::Comment(_) | BlockKind::DocumentAttribute(_) => None,
                _ => None,
            }
        };
    }
    read!(
        TableOfContents,
        Admonition,
        DiscreteHeader,
        ThematicBreak,
        PageBreak,
        UnorderedList,
        OrderedList,
        CalloutList,
        DescriptionList,
        Section,
        Paragraph,
        Image,
        Audio,
        Video,
        DelimitedComment,
        Example,
        Listing,
        Literal,
        Open,
        Sidebar,
        Table,
        Passthrough,
        Quote,
        Verse,
        Stem
    )
}

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
    let before = serde_json::to_value(parsed.document())?;
    let owner = parsed.document().blocks.first().ok_or("block")?;
    let metadata = owner.metadata().ok_or("block metadata")?;
    let attributes = attributes(BlockRef::from(owner)).ok_or("attribute reads")?;
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
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn quote_content_reads_preserve_formatting_and_present_empty_values(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[quote,attribution='*Ada*',citetitle='_Book_']\n____\nText.\n____\n\n[verse,attribution='',citetitle='']\n____\nText.\n____",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Quote(quote) = blocks.next().ok_or("quote")?.kind() else {
        return Err("quote".into());
    };
    assert_eq!(
        quote.attributes().get("attribution"),
        quote.attribution().map(AttributeValueRef::Content)
    );
    assert_eq!(
        quote.attributes().get("citetitle"),
        quote.citetitle().map(AttributeValueRef::Content)
    );
    let Some(AttributeValueRef::Content(author)) = quote.attributes().get("attribution") else {
        return Err("author".into());
    };
    assert!(matches!(
        author.inlines().next().ok_or("author text")?.kind(),
        InlineKind::Bold(_)
    ));
    let Some(AttributeValueRef::Content(citation)) = quote.attributes().get("citetitle") else {
        return Err("citation".into());
    };
    assert!(matches!(
        citation.inlines().next().ok_or("citation text")?.kind(),
        InlineKind::Italic(_)
    ));
    let BlockKind::Verse(verse) = blocks.next().ok_or("verse")?.kind() else {
        return Err("verse".into());
    };
    for name in ["attribution", "citetitle"] {
        let Some(AttributeValueRef::Content(content)) = verse.attributes().get(name) else {
            return Err("present empty content".into());
        };
        assert!(content.is_empty());
    }
    Ok(())
}

#[test]
fn authoritative_fields_suppress_stale_scalars_and_generated_captions() -> Result<(), Error> {
    let mut owner = Paragraph::new(Vec::new(), Location::default());
    for name in ["attribution", "citetitle", "caption", "reftext", "subs"] {
        owner.metadata.attributes.set(name.into(), "stale".into());
    }
    let absent = ParagraphRef::from(&owner).attributes();
    inventory(absent);
    assert!(absent.is_empty());
    owner.metadata.attribution = Some(ContentAttribute::default());
    owner.metadata.citetitle = Some(ContentAttribute::default());
    owner.metadata.reftext = Some(ContentAttribute::default());
    owner.metadata.caption = Some(Caption::CustomContent(ContentAttribute::default()));
    owner.metadata.substitutions = Some(SubstitutionSpec::Explicit(Vec::new()));
    let present = ParagraphRef::from(&owner).attributes();
    inventory(present);
    assert_eq!(present.iter().count(), 5);
    for name in ["attribution", "citetitle", "caption", "reftext"] {
        let Some(AttributeValueRef::Content(content)) = present.get(name) else {
            return Err("empty content".into());
        };
        assert!(content.is_empty());
    }
    let Some(AttributeValueRef::Substitutions(spec)) = present.get("subs") else {
        return Err("subs".into());
    };
    assert_eq!(spec.kind(), SubstitutionSpecKind::Explicit);
    assert!(spec.is_empty());
    owner.metadata.caption = Some(Caption::Custom("document prefix".into()));
    assert!(!ParagraphRef::from(&owner).attributes().contains("caption"));

    let mut inline = InlineImage::new(Source::Name("bird"), Location::default());
    for name in ["attribution", "citetitle", "caption", "reftext", "subs"] {
        inline.set_attribute(name.into(), "extension".into())?;
        assert_eq!(
            InlineImageRef::from(&inline).attributes().get(name),
            Some(AttributeValueRef::Text("extension"))
        );
    }
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
        .document_ref()
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
    let paragraph = parsed.document().blocks.last().ok_or("paragraph")?;
    assert!(
        !attributes(BlockRef::from(paragraph))
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

#[rstest::rstest]
#[case("", SubstitutionSpecKind::Explicit, vec![])]
#[case("none", SubstitutionSpecKind::Explicit, vec![])]
#[case("q,quotes,+macros,-callouts,quotes+,,unknown,q", SubstitutionSpecKind::Source, vec!["q", "quotes", "+macros", "-callouts", "quotes+", "", "unknown", "q"])]
fn parsed_substitution_reads_keep_authored_entries(
    #[case] value: &str,
    #[case] kind: SubstitutionSpecKind,
    #[case] expected: Vec<&str>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("[subs=\"{value}\"]\nText.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let attrs = attributes(parsed.document_ref().blocks().next().ok_or("paragraph")?)
        .ok_or("attributes")?;
    inventory(attrs);
    let Some(AttributeValueRef::Substitutions(spec)) = attrs.get("subs") else {
        return Err("subs".into());
    };
    assert_eq!(spec.kind(), kind);
    assert_eq!(spec.is_empty(), expected.is_empty());
    assert_eq!(
        spec.entries().collect::<Vec<_>>(),
        expected
            .into_iter()
            .map(SubstitutionEntryRef::Source)
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn typed_substitution_specs_keep_empty_forms_and_order() {
    let entry = String::from("unknown");
    let pointer = entry.as_ptr();
    let source = SubstitutionSpec::Source(vec![entry]);
    assert!(
        matches!(SubstitutionSpecRef::from(&source).entries().next(),
        Some(SubstitutionEntryRef::Source(text)) if text == "unknown" && text.as_ptr() == pointer)
    );
    let specifications = [
        SubstitutionSpec::Source(Vec::new()),
        SubstitutionSpec::Explicit(Vec::new()),
        SubstitutionSpec::Modifiers(Vec::new()),
    ];
    for (owner, kind) in specifications.iter().zip([
        SubstitutionSpecKind::Source,
        SubstitutionSpecKind::Explicit,
        SubstitutionSpecKind::Modifiers,
    ]) {
        let spec = SubstitutionSpecRef::from(owner);
        assert!(spec.is_empty());
        assert_eq!(spec.kind(), kind);
    }
    let explicit = SubstitutionSpec::Explicit(vec![
        Substitution::Quotes,
        Substitution::Normal,
        Substitution::Quotes,
    ]);
    assert_eq!(
        SubstitutionSpecRef::from(&explicit)
            .entries()
            .collect::<Vec<_>>(),
        [
            SubstitutionEntryRef::Substitution(Substitution::Quotes),
            SubstitutionEntryRef::Substitution(Substitution::Normal),
            SubstitutionEntryRef::Substitution(Substitution::Quotes)
        ]
    );
    let operations = vec![
        SubstitutionOp::Append(Substitution::Quotes),
        SubstitutionOp::Remove(Substitution::Macros),
        SubstitutionOp::Prepend(Substitution::Quotes),
    ];
    let modifiers = SubstitutionSpec::Modifiers(operations.clone());
    let entries = {
        let copy = SubstitutionSpecRef::from(&modifiers);
        copy.entries()
    };
    assert_eq!(
        entries.collect::<Vec<_>>(),
        operations
            .into_iter()
            .map(SubstitutionEntryRef::Modifier)
            .collect::<Vec<_>>()
    );
}

#[rstest::rstest]
fn content_locations_survive_includes_and_nested_cells(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("quoted.adoc");
    std::fs::write(
        &main,
        "include::quoted.adoc[]\n\n[cols=a]\n|===\n|[reftext='*Référence*']\nCell.\n|===\n",
    )?;
    std::fs::write(&include, "[quote,attribution='*Éva*']\n____\nText.\n____\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let attrs = attributes(blocks.next().ok_or("quote")?).ok_or("attributes")?;
    let Some(AttributeValueRef::Content(author)) = attrs.get("attribution") else {
        return Err("author".into());
    };
    let author = author.inlines().next().ok_or("author span")?;
    assert!(matches!(author.kind(), InlineKind::Bold(_)));
    assert_eq!(
        parsed.resolve_location(author.location()).file,
        Some(include)
    );
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
    let attrs =
        attributes(cell.blocks().next().ok_or("cell paragraph")?).ok_or("cell attributes")?;
    let Some(AttributeValueRef::Content(label)) = attrs.get("reftext") else {
        return Err("reference text".into());
    };
    let label = label.inlines().next().ok_or("reference span")?;
    assert!(matches!(label.kind(), InlineKind::Bold(_)));
    assert_eq!(parsed.resolve_location(label.location()).file, Some(main));
    Ok(())
}
