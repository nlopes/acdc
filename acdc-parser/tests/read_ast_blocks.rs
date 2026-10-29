use acdc_parser::{
    Admonition, AdmonitionVariant, Block, Caption, CompatibilityMode, ContentAttribute,
    DelimitedBlock, DelimitedBlockType, Location, Options, SafeMode, StemNotation,
    ast::{AdmonitionRef, AttributeValueRef, BlockKind, BlockRef, ContentRef, InlineKind},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    let mut output = String::new();
    for inline in content.inlines() {
        let kind = inline.kind();
        if let InlineKind::Plain(node) = kind {
            output.push_str(node.text());
        } else if let InlineKind::Verbatim(node) = kind {
            output.push_str(node.text());
        } else if let InlineKind::Raw(node) = kind {
            output.push_str(node.text());
        } else if let InlineKind::Bold(node) = kind {
            output.push_str(&text(node.content())?);
        } else {
            return Err("unexpected inline kind in test content".into());
        }
    }
    Ok(output)
}

#[rstest::rstest]
fn compound_blocks_expose_nested_content_and_distinct_metadata(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ".Outer\n[#outer,role=container,options=custom,caption='*Prefix*']\n====\n\
         --\n****\n[quote,'*Ada*','*Notes*']\n____\nA *quote*.\n____\n****\n--\n====\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let BlockKind::Example(example) = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("example")?
        .kind()
    else {
        return Err("example".into());
    };
    assert_eq!(text(example.title().ok_or("title")?)?, "Outer");
    assert_eq!(text(example.caption_content().ok_or("caption")?)?, "Prefix");
    assert_eq!(example.id().ok_or("id")?.as_str(), "outer");
    assert!(example.roles().contains("container"));
    assert!(example.options().contains("custom"));
    assert_eq!(
        example.attributes().get("title"),
        example.title().map(AttributeValueRef::Content)
    );
    assert_eq!(example.delimiter(), "====");
    let BlockKind::Open(open) = example.blocks().next().ok_or("open")?.kind() else {
        return Err("open".into());
    };
    let BlockKind::Sidebar(sidebar) = open.blocks().next().ok_or("sidebar")?.kind() else {
        return Err("sidebar".into());
    };
    let BlockKind::Quote(quote) = sidebar.blocks().next().ok_or("quote")?.kind() else {
        return Err("quote".into());
    };
    assert_eq!(text(quote.attribution().ok_or("attribution")?)?, "Ada");
    assert_eq!(text(quote.citetitle().ok_or("citation")?)?, "Notes");
    assert!(quote.title().is_none());
    assert!(quote.caption_content().is_none());
    let BlockKind::Paragraph(body) = quote.blocks().next().ok_or("body")?.kind() else {
        return Err("paragraph".into());
    };
    assert_eq!(text(body.content())?, "A quote.");
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn inline_and_math_bodies_keep_their_content_kinds(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[source,rust]\n----\n  *code* <tag>\n----\n\n\
         ....\n  literal\n....\n\n\
         [verse,'*Ada*','*Notes*']\n____\n*First*\nSecond\n____\n\n\
         ++++\n<b>raw</b>\n++++\n\n\
         [latexmath]\n++++\nx^2\n++++\n\n\
         ////\nHidden\n////\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Listing(listing) = blocks.next().ok_or("listing")?.kind() else {
        return Err("listing".into());
    };
    assert_eq!(listing.style(), Some("source"));
    assert_eq!(
        listing.attributes().get("language"),
        Some(AttributeValueRef::Text("rust"))
    );
    assert_eq!(text(listing.content())?, "  *code* <tag>");
    assert!(
        listing
            .content()
            .inlines()
            .all(|n| matches!(n.kind(), InlineKind::Verbatim(_)))
    );
    assert_eq!(listing.source_text(), Some("  *code* <tag>\n"));
    assert_eq!(
        listing
            .open_delimiter_location()
            .ok_or("open")?
            .start()
            .line(),
        2
    );
    assert_eq!(
        listing
            .close_delimiter_location()
            .ok_or("close")?
            .start()
            .line(),
        4
    );
    let BlockKind::Literal(literal) = blocks.next().ok_or("literal")?.kind() else {
        return Err("literal".into());
    };
    assert_eq!(text(literal.content())?, "  literal");
    let BlockKind::Verse(verse) = blocks.next().ok_or("verse")?.kind() else {
        return Err("verse".into());
    };
    assert_eq!(text(verse.content())?, "First\nSecond");
    assert_eq!(text(verse.attribution().ok_or("attribution")?)?, "Ada");
    assert_eq!(text(verse.citetitle().ok_or("citation")?)?, "Notes");
    let BlockKind::Passthrough(pass) = blocks.next().ok_or("pass")?.kind() else {
        return Err("pass".into());
    };
    assert_eq!(text(pass.content())?, "<b>raw</b>");
    assert!(
        pass.content()
            .inlines()
            .all(|n| matches!(n.kind(), InlineKind::Raw(_)))
    );
    let BlockKind::Stem(stem) = blocks.next().ok_or("stem")?.kind() else {
        return Err("stem".into());
    };
    assert_eq!(stem.text(), "x^2");
    assert_eq!(stem.notation(), StemNotation::Latexmath);
    let BlockKind::DelimitedComment(comment) = blocks.next().ok_or("comment")?.kind() else {
        return Err("comment".into());
    };
    assert_eq!(text(comment.content())?, "Hidden");
    assert!(blocks.next().is_none());
    Ok(())
}

#[rstest::rstest]
fn reads_preserve_mode_policy_and_explicit_body_substitutions(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[title=\"*Title*\",subs=quotes]\n----\n*Formatted*\n----\n\n\
         [verse,attribution='',citetitle='',subs=none]\n____\n*Literal*\n____\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Listing(listing) = blocks.next().ok_or("listing")?.kind() else {
        return Err("listing".into());
    };
    let title = listing.title().ok_or("title")?;
    let kind = title.inlines().next().ok_or("title inline")?.kind();
    match mode {
        CompatibilityMode::Asciidoctor => {
            assert!(matches!(kind, InlineKind::Bold(_)));
            assert_eq!(text(title)?, "Title");
        }
        CompatibilityMode::Draft => {
            assert!(matches!(kind, InlineKind::Raw(_)));
            assert_eq!(text(title)?, "*Title*");
        }
        _ => return Err("unsupported compatibility mode".into()),
    }
    assert!(
        listing
            .content()
            .inlines()
            .any(|n| matches!(n.kind(), InlineKind::Bold(_)))
    );
    assert_eq!(listing.source_text(), Some("*Formatted*\n"));
    let BlockKind::Verse(verse) = blocks.next().ok_or("verse")?.kind() else {
        return Err("verse".into());
    };
    assert!(verse.attribution().ok_or("empty attribution")?.is_empty());
    assert!(verse.citetitle().ok_or("empty citation")?.is_empty());
    assert_eq!(text(verse.content())?, "*Literal*");
    assert!(verse.title().is_none());
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn admonitions_expose_kind_metadata_and_children() -> Result<(), Error> {
    let parsed = parse(
        ".Careful\n[#warning,role=alert]\n[WARNING]\n====\nFirst.\n\nSecond.\n====\n\nNOTE: Inline note.\n",
        &Options::default(),
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Admonition(warning) = blocks.next().ok_or("warning")?.kind() else {
        return Err("warning".into());
    };
    assert_eq!(warning.variant(), AdmonitionVariant::Warning);
    assert_eq!(warning.id().ok_or("id")?.as_str(), "warning");
    assert!(warning.roles().contains("alert"));
    assert_eq!(text(warning.title().ok_or("title")?)?, "Careful");
    assert_eq!(warning.blocks().count(), 2);
    let BlockKind::Admonition(note) = blocks.next().ok_or("note")?.kind() else {
        return Err("note".into());
    };
    assert_eq!(note.variant(), AdmonitionVariant::Note);
    assert_eq!(note.blocks().count(), 1);
    let owned = Admonition::new(AdmonitionVariant::Tip, Vec::new(), Location::default());
    assert_eq!(
        AdmonitionRef::from(&owned).variant(),
        AdmonitionVariant::Tip
    );
    assert!(AdmonitionRef::from(&owned).blocks().next().is_none());
    Ok(())
}

#[test]
fn constructed_blocks_keep_empty_content_and_optional_source() -> Result<(), Error> {
    fn shared<T: Send + Sync>(value: T) -> T {
        value
    }
    let mut owner = DelimitedBlock::new(
        DelimitedBlockType::DelimitedQuote(Vec::new()),
        "",
        Location::default(),
    );
    owner.metadata.attribution = Some(ContentAttribute::default());
    owner.metadata.caption = Some(Caption::CustomContent(ContentAttribute::default()));
    let BlockKind::Quote(quote) = BlockRef::from(&owner).kind() else {
        return Err("quote".into());
    };
    assert!(quote.attribution().ok_or("attribution")?.is_empty());
    assert!(quote.citetitle().is_none());
    assert!(quote.caption_content().ok_or("caption")?.is_empty());
    assert!(quote.blocks().next().is_none());
    assert_eq!(quote.delimiter(), "");
    assert!(quote.open_delimiter_location().is_none());
    assert!(quote.close_delimiter_location().is_none());
    assert!(quote.source_text().is_none());
    let children = shared(quote.blocks());
    std::thread::scope(|scope| {
        scope.spawn(move || assert_eq!(children.count(), 0));
    });
    Ok(())
}

#[test]
fn included_delimiters_resolve_to_the_authored_file() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let included = directory.path().join("body.adoc");
    std::fs::write(&main, "include::body.adoc[]\n")?;
    std::fs::write(&included, "----\nλ code\n----\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let BlockKind::Listing(listing) = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("listing")?
        .kind()
    else {
        return Err("listing".into());
    };
    let opening = parsed.resolve_location(listing.open_delimiter_location().ok_or("opening")?);
    let closing = parsed.resolve_location(listing.close_delimiter_location().ok_or("closing")?);
    assert_eq!(opening.file, Some(included.clone()));
    assert_eq!(closing.file, Some(included));
    assert_eq!(opening.location.start.line, 1);
    assert_eq!(closing.location.start.line, 3);
    assert_eq!(text(listing.content())?, "λ code");
    assert!(matches!(
        parsed.document().blocks.first(),
        Some(Block::DelimitedBlock(_))
    ));
    Ok(())
}
