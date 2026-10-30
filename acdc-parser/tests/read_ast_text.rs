use acdc_parser::{
    CompatibilityMode, InlineMacro, InlineNode, LineBreak, Location, Options, PassthroughKind,
    Plain, Raw, SafeMode, StandaloneCurvedApostrophe, Substitution, Verbatim,
    ast::{
        BlockKind, ContentRef, DocumentRef, InlineKind, InlineRef, LineBreakRef, PassRef, PlainRef,
        RawRef, StandaloneCurvedApostropheRef, VerbatimRef,
    },
    parse, parse_file, parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn shared<T: Send + Sync>(_: T) {}

fn paragraph(document: DocumentRef<'_>) -> Result<ContentRef<'_>, Error> {
    document
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                Some(paragraph.content())
            } else {
                None
            }
        })
        .ok_or_else(|| "paragraph".into())
}

#[rstest::rstest]
fn plain_reads_keep_escapes_and_character_references_unrendered(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":value: expanded\n\né \\^literal^ &amp; &#169; &#x3c;\n\n{value}\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut text = String::new();
    let mut escaped = false;
    for inline in paragraph(parsed.document_ref())?.inlines() {
        let InlineKind::Plain(plain) = inline.kind() else {
            return Err("expected plain text".into());
        };
        shared(plain);
        text.push_str(plain.text());
        if plain.is_escaped() {
            assert!(plain.text().contains('^'));
            escaped = true;
        }
    }
    assert_eq!(text, "é ^literal^ &amp; &#169; &#x3c;");
    assert!(escaped);
    let BlockKind::Paragraph(expanded) = parsed
        .document_ref()
        .blocks()
        .nth(1)
        .ok_or("expanded paragraph")?
        .kind()
    else {
        return Err("expanded paragraph".into());
    };
    let inline = expanded.content().inlines().next().ok_or("expanded text")?;
    let InlineKind::Plain(plain) = inline.kind() else {
        return Err("expanded text".into());
    };
    assert_eq!(plain.text(), "expanded");
    assert_eq!(
        source.get(plain.location().absolute_start()..=plain.location().absolute_end()),
        Some("{value}")
    );
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
#[case("+<é> *raw*+", vec![Substitution::SpecialChars])]
#[case("++<é> *raw*++", vec![Substitution::SpecialChars])]
#[case("+++<é> *raw*+++", vec![])]
#[case("pass:[<é> *raw*]", vec![])]
#[case("pass:c[<é> *raw*]", vec![Substitution::SpecialChars])]
fn document_passthroughs_expose_lowered_text_and_remaining_substitutions(
    #[case] source: &str,
    #[case] substitutions: Vec<Substitution>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let nodes = paragraph(parsed.document_ref())?
        .inlines()
        .collect::<Vec<_>>();
    let [inline] = nodes.as_slice() else {
        return Err(format!("expected one raw node: {nodes:?}").into());
    };
    let InlineKind::Raw(raw) = inline.kind() else {
        return Err("expected lowered passthrough".into());
    };
    shared(raw);
    shared(raw.substitutions());
    assert_eq!(raw.text(), "<é> *raw*");
    assert_eq!(raw.substitutions().collect::<Vec<_>>(), substitutions);
    assert_eq!(
        source.get(raw.location().absolute_start()..=raw.location().absolute_end()),
        Some("<é> *raw*")
    );
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn document_passthrough_substitutions_can_produce_structure(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "pass:q[*bold*]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let inline = paragraph(parsed.document_ref())?
        .inlines()
        .next()
        .ok_or("bold")?;
    let InlineKind::Bold(bold) = inline.kind() else {
        return Err("expected parsed bold content, not a pass macro".into());
    };
    assert_eq!(
        source.get(bold.location().absolute_start()..=bold.location().absolute_end()),
        Some("*bold*")
    );
    assert_eq!(bold.content().inlines().count(), 1);
    Ok(())
}

#[rstest::rstest]
fn standalone_pass_reads_preserve_unprocessed_content_and_empty_values(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "pass:q[*bold*] pass:[]";
    let parsed = parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [
        InlineNode::Macro(InlineMacro::Pass(owner)),
        _,
        InlineNode::Macro(InlineMacro::Pass(empty)),
    ] = parsed.inlines()
    else {
        return Err("expected retained pass macros".into());
    };
    let InlineKind::Pass(pass) =
        InlineRef::from(parsed.inlines().first().ok_or("pass macro")?).kind()
    else {
        return Err("pass kind".into());
    };
    shared(pass);
    shared(pass.substitutions());
    assert_eq!(pass.text(), Some("*bold*"));
    assert_eq!(pass.kind(), PassthroughKind::Macro);
    assert_eq!(
        pass.substitutions().collect::<Vec<_>>(),
        [Substitution::Quotes]
    );
    assert_eq!(
        source.get(pass.location().absolute_start()..=pass.location().absolute_end()),
        Some("pass:q[*bold*]")
    );
    assert_eq!(PassRef::from(empty).text(), Some(""));
    assert!(PassRef::from(empty).substitutions().next().is_none());
    let mut absent = owner.clone();
    absent.text = None;
    absent.kind = PassthroughKind::AttributeRef;
    absent.substitutions = vec![
        Substitution::Replacements,
        Substitution::SpecialChars,
        Substitution::Replacements,
    ];
    assert_eq!(PassRef::from(&absent).text(), None);
    assert_eq!(PassRef::from(&absent).kind(), PassthroughKind::AttributeRef);
    assert_eq!(
        PassRef::from(&absent).substitutions().collect::<Vec<_>>(),
        absent.substitutions
    );
    assert!(serde_json::to_value(parsed.inlines()).is_err());
    Ok(())
}

#[rstest::rstest]
fn typography_and_breaks_keep_semantic_kinds_and_authored_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "é \"`double`\" '`single`' `' +\nnext";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut spans = Vec::new();
    for inline in paragraph(parsed.document_ref())?.inlines() {
        if let InlineKind::CurvedQuotation(span) = inline.kind() {
            assert_eq!(span.content().inlines().count(), 1);
            spans.push("\"`double`\"");
        } else if let InlineKind::CurvedApostrophe(span) = inline.kind() {
            assert_eq!(span.content().inlines().count(), 1);
            spans.push("'`single`'");
        } else if let InlineKind::StandaloneCurvedApostrophe(marker) = inline.kind() {
            shared(marker);
            spans.push("`'");
        } else if let InlineKind::LineBreak(marker) = inline.kind() {
            shared(marker);
            spans.push(" +");
        } else {
            assert!(matches!(inline.kind(), InlineKind::Plain(_)));
            continue;
        }
        assert_eq!(
            source.get(inline.location().absolute_start()..=inline.location().absolute_end()),
            spans.last().copied()
        );
    }
    assert_eq!(spans.len(), 4);
    Ok(())
}

#[test]
fn owner_reads_keep_distinct_text_policies_and_ordered_substitutions() {
    let location = Location::default();
    let plain = Plain {
        content: "*x*",
        location: location.clone(),
        escaped: true,
    };
    let raw = Raw {
        content: "*x*",
        location: location.clone(),
        subs: vec![
            Substitution::SpecialChars,
            Substitution::Replacements,
            Substitution::SpecialChars,
        ],
    };
    let verbatim = Verbatim {
        content: "  *x*\n",
        location: location.clone(),
    };
    assert!(PlainRef::from(&plain).is_escaped());
    assert_eq!(PlainRef::from(&plain).text(), RawRef::from(&raw).text());
    let substitutions = {
        let read = RawRef::from(&raw);
        read.substitutions()
    };
    assert_eq!(substitutions.collect::<Vec<_>>(), raw.subs);
    assert_eq!(VerbatimRef::from(&verbatim).text(), "  *x*\n");
    let apostrophe = StandaloneCurvedApostrophe {
        location: location.clone(),
    };
    let line_break = LineBreak { location };
    assert_eq!(
        StandaloneCurvedApostropheRef::from(&apostrophe)
            .location()
            .absolute_start(),
        apostrophe.location.absolute_start
    );
    assert_eq!(
        LineBreakRef::from(&line_break).location().absolute_start(),
        line_break.location.absolute_start
    );
}

#[rstest::rstest]
fn text_reads_preserve_includes_table_cells_and_listing_whitespace(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("text.adoc");
    std::fs::write(
        &main,
        "include::text.adoc[]\n\n[cols=a]\n|===\n|é +<cell>+\n|===\n\n----\n  <literal> *code*\n----\n",
    )?;
    std::fs::write(&include, "é +<included>+\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let raw = paragraph(parsed.document_ref())?
        .inlines()
        .find_map(|inline| {
            if let InlineKind::Raw(raw) = inline.kind() {
                Some(raw)
            } else {
                None
            }
        })
        .ok_or("included raw text")?;
    assert_eq!(raw.text(), "<included>");
    let location = parsed.resolve_location(raw.location());
    assert_eq!(location.file, Some(include));
    assert_eq!(location.location.start.column, 4);
    let mut blocks = parsed.document_ref().blocks().skip(1);
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
    let BlockKind::Paragraph(paragraph) = cell.blocks().next().ok_or("cell paragraph")?.kind()
    else {
        return Err("cell paragraph".into());
    };
    let raw = paragraph
        .content()
        .inlines()
        .find_map(|inline| {
            if let InlineKind::Raw(raw) = inline.kind() {
                Some(raw)
            } else {
                None
            }
        })
        .ok_or("cell raw text")?;
    assert_eq!(raw.text(), "<cell>");
    assert_eq!(parsed.resolve_location(raw.location()).file, Some(main));
    let BlockKind::Listing(listing) = blocks.next().ok_or("listing")?.kind() else {
        return Err("listing".into());
    };
    let text = listing
        .content()
        .inlines()
        .map(|inline| {
            if let InlineKind::Verbatim(text) = inline.kind() {
                Ok(text.text())
            } else {
                Err("verbatim text")
            }
        })
        .collect::<Result<String, _>>()?;
    assert_eq!(text, "  <literal> *code*");
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}
