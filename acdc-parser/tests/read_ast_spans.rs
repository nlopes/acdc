use acdc_parser::{
    Bold, CompatibilityMode, Form, Highlight, Location, Options, Paragraph, SafeMode,
    ast::{AttributeValueRef, BlockKind, BoldRef, HighlightRef, InlineKind, ParagraphRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn shared<T: Copy + Send + Sync>(_: T) {}

#[rstest::rstest]
#[case::bold("*x*", "bold", Form::Constrained)]
#[case::bold_unconstrained("**x**", "bold", Form::Unconstrained)]
#[case::italic("_x_", "italic", Form::Constrained)]
#[case::italic_unconstrained("__x__", "italic", Form::Unconstrained)]
#[case::monospace("`x`", "monospace", Form::Constrained)]
#[case::monospace_unconstrained("``x``", "monospace", Form::Unconstrained)]
#[case::highlight("#x#", "highlight", Form::Constrained)]
#[case::highlight_unconstrained("##x##", "highlight", Form::Unconstrained)]
#[case::subscript("~x~", "subscript", Form::Unconstrained)]
#[case::superscript("^x^", "superscript", Form::Unconstrained)]
#[case::quotation("\"`x`\"", "quotation", Form::Unconstrained)]
#[case::apostrophe("'`x`'", "apostrophe", Form::Unconstrained)]
fn concrete_spans_keep_content_form_identity_and_tokens(
    #[case] source: &str,
    #[case] expected_kind: &str,
    #[case] expected_form: Form,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("Before [#span.one.two.one]{source} after.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.warnings(), []);
    let before = serde_json::to_value(parsed.document())?;
    let BlockKind::Paragraph(paragraph) = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("paragraph")?
        .kind()
    else {
        return Err("paragraph".into());
    };
    let span = paragraph.content().inlines().nth(1).ok_or("span")?;
    macro_rules! check {
        ($node:expr, $kind:literal) => {{
            let node = $node;
            shared(node);
            assert_eq!($kind, expected_kind);
            assert_eq!(node.form(), expected_form);
            let id = node.id().ok_or("id")?;
            assert_eq!(id.as_str(), "span");
            assert_eq!(id.location().absolute_start(), 9);
            assert_eq!(id.location().absolute_end(), 12);
            assert_eq!(
                node.roles().iter().collect::<Vec<_>>(),
                ["one", "two", "one"]
            );
            assert!(node.roles().contains("two"));
            assert!(!node.roles().contains("one two"));
            let attributes = node.attributes();
            assert_eq!(attributes.get("id"), Some(AttributeValueRef::Text("span")));
            assert_eq!(
                attributes.get("roles"),
                Some(AttributeValueRef::Tokens(node.roles()))
            );
            assert!(!attributes.contains("title"));
            assert!(!attributes.contains("options"));
            assert_eq!(attributes.iter().count(), 2);
            for (name, value) in attributes.iter() {
                assert_eq!(attributes.get(name), Some(value));
            }
            let mut content = node.content().inlines();
            let InlineKind::Plain(text) = content.next().ok_or("text")?.kind() else {
                return Err("text".into());
            };
            assert_eq!(text.text(), "x");
            assert!(content.next().is_none());
            std::thread::scope(|scope| {
                let roles = node.roles().iter();
                scope.spawn(move || assert_eq!(roles.count(), 3));
            });
        }};
    }
    if let InlineKind::Bold(node) = span.kind() {
        check!(node, "bold");
    } else if let InlineKind::Italic(node) = span.kind() {
        check!(node, "italic");
    } else if let InlineKind::Monospace(node) = span.kind() {
        check!(node, "monospace");
    } else if let InlineKind::Highlight(node) = span.kind() {
        assert!(!node.is_marked());
        check!(node, "highlight");
    } else if let InlineKind::Subscript(node) = span.kind() {
        check!(node, "subscript");
    } else if let InlineKind::Superscript(node) = span.kind() {
        check!(node, "superscript");
    } else if let InlineKind::CurvedQuotation(node) = span.kind() {
        check!(node, "quotation");
    } else if let InlineKind::CurvedApostrophe(node) = span.kind() {
        check!(node, "apostrophe");
    } else {
        return Err(format!("unexpected inline: {:?}", span.kind()).into());
    }
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn marks_and_generic_spans_keep_their_meaning(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "#marked# [.red]#role# [#identity]#id# [one\ttwo\u{2003}one]#tokens#",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(paragraph) = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("paragraph")?
        .kind()
    else {
        return Err("paragraph".into());
    };
    let spans = paragraph
        .content()
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::Highlight(span) = inline.kind() {
                Some(span)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 4);
    let mut spans = spans.into_iter();
    let mark = spans.next().ok_or("mark")?;
    assert!(mark.is_marked());
    assert!(mark.attributes().is_empty());
    assert!(!spans.next().ok_or("role")?.is_marked());
    assert!(!spans.next().ok_or("id")?.is_marked());
    assert_eq!(
        spans
            .next()
            .ok_or("tokens")?
            .roles()
            .iter()
            .collect::<Vec<_>>(),
        ["one", "two", "one"]
    );
    Ok(())
}

#[rstest::rstest]
fn nested_spans_keep_concrete_children_and_unicode_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    fn read_bold(span: BoldRef<'_>) -> Result<(), Error> {
        let children = span.content().inlines().collect::<Vec<_>>();
        assert_eq!(children.len(), 3);
        let InlineKind::Italic(italic) = children.get(1).ok_or("italic")?.kind() else {
            return Err("italic".into());
        };
        assert_eq!(italic.form(), Form::Constrained);
        assert!(italic.roles().is_empty());
        assert!(italic.id().is_none());
        assert!(italic.attributes().is_empty());
        let InlineKind::Plain(text) = italic.content().inlines().next().ok_or("text")?.kind()
        else {
            return Err("text".into());
        };
        assert_eq!(text.text(), "中");
        Ok(())
    }
    let parsed = parse(
        "é *外 _中_ 後*",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(paragraph) = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("paragraph")?
        .kind()
    else {
        return Err("paragraph".into());
    };
    let InlineKind::Bold(bold) = paragraph.content().inlines().nth(1).ok_or("bold")?.kind() else {
        return Err("bold".into());
    };
    assert_eq!(
        (
            bold.location().start().line(),
            bold.location().start().column()
        ),
        (1, 3)
    );
    assert_eq!(bold.location().end().column(), 11);
    read_bold(bold)
}

#[rstest::rstest]
fn included_and_table_cell_spans_keep_original_source_context(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("spans.adoc");
    std::fs::write(
        &main,
        "include::spans.adoc[]\n\n[cols=a]\n|===\n|[#cell.one]*cell*\n|===\n",
    )?;
    std::fs::write(&include, "é [#inner.one]_中_\n")?;
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
    let InlineKind::Italic(italic) = paragraph.content().inlines().nth(1).ok_or("italic")?.kind()
    else {
        return Err("italic".into());
    };
    assert_eq!(italic.id().ok_or("id")?.as_str(), "inner");
    assert_eq!(
        parsed.resolve_location(italic.location()).file,
        Some(include)
    );
    assert_eq!(italic.location().start().line(), 1);
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
    let InlineKind::Bold(bold) = paragraph.content().inlines().next().ok_or("bold")?.kind() else {
        return Err("bold".into());
    };
    assert_eq!(bold.id().ok_or("id")?.as_str(), "cell");
    assert!(bold.roles().contains("one"));
    assert_eq!(bold.location().start().line(), 5);
    Ok(())
}

#[test]
fn constructed_spans_keep_empty_identity_and_semantic_token_equality() -> Result<(), Error> {
    let mut bold = Bold {
        id: Some(acdc_parser::ElementId::new("", Location::default())),
        role: Some("one\ttwo\u{2003}one"),
        form: Form::Unconstrained,
        content: Vec::new(),
        location: Location::default(),
    };
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph.metadata.roles = vec!["one", "two", "one"];
    let span = BoldRef::from(&bold);
    assert_eq!(span.id().ok_or("empty id")?.as_str(), "");
    assert!(span.content().is_empty());
    assert_eq!(span.roles(), ParagraphRef::from(&paragraph).roles());
    assert_eq!(
        span.attributes().get("role"),
        ParagraphRef::from(&paragraph).attributes().get("role")
    );
    bold.role = Some(" \t\u{2003}");
    let span = BoldRef::from(&bold);
    assert!(span.roles().is_empty());
    assert!(!span.attributes().contains("role"));
    let mut highlight = Highlight {
        id: None,
        role: None,
        form: Form::Constrained,
        content: Vec::new(),
        location: Location::default(),
    };
    assert!(HighlightRef::from(&highlight).is_marked());
    highlight.role = Some("");
    assert!(!HighlightRef::from(&highlight).is_marked());
    highlight.role = None;
    highlight.id = Some(acdc_parser::ElementId::new("", Location::default()));
    assert!(!HighlightRef::from(&highlight).is_marked());
    assert_eq!(
        HighlightRef::from(&highlight).attributes().get("id"),
        Some(AttributeValueRef::Text(""))
    );
    Ok(())
}
