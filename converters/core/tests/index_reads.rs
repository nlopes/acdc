//! Index discovery keeps occurrence order, byte offsets, and label boundaries.

use acdc_converters_core::{InlineTextTransform, index::visit_index_inlines};
use acdc_parser::{
    CompatibilityMode, Location, Options,
    ast::InlineRef,
    edit::{IndexTerm, IndexTermKind, IndexTermRelationship, InlineNode, Plain},
    parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn visit_index_terms<E>(
    nodes: &[InlineNode<'_>],
    visit: &mut impl FnMut(&IndexTerm<'_>, usize) -> Result<(), E>,
) -> Result<(), E> {
    fn walk<E>(
        nodes: &[InlineNode<'_>],
        transform: &InlineTextTransform<'_, '_>,
        offset: &mut String,
        visit: &mut impl FnMut(&IndexTerm<'_>, usize) -> Result<(), E>,
    ) -> Result<(), E> {
        for node in nodes {
            if let InlineNode::IndexTerm(term) = node {
                visit(term, offset.len())?;
            } else {
                let children = match node.clone() {
                    InlineNode::Bold(mut span) => Some(span.take_content()),
                    InlineNode::Italic(mut span) => Some(span.take_content()),
                    InlineNode::Monospace(mut span) => Some(span.take_content()),
                    InlineNode::Highlight(mut span) => Some(span.take_content()),
                    InlineNode::Subscript(mut span) => Some(span.take_content()),
                    InlineNode::Superscript(mut span) => Some(span.take_content()),
                    InlineNode::DoubleQuotedSpan(mut span) => Some(span.take_content()),
                    InlineNode::SingleQuotedSpan(mut span) => Some(span.take_content()),
                    InlineNode::UrlMacro(mut link) => Some(link.take_text()),
                    InlineNode::Link(mut link) => Some(link.take_text()),
                    InlineNode::Mailto(mut link) => Some(link.take_text()),
                    InlineNode::CrossReference(mut link) => Some(link.take_text()),
                    InlineNode::Footnote(mut note) => note.take_content(),
                    InlineNode::Plain(_)
                    | InlineNode::Symbol(_)
                    | InlineNode::CharacterReference(_)
                    | InlineNode::Raw(_)
                    | InlineNode::Verbatim(_)
                    | InlineNode::StandaloneCurvedApostrophe(_)
                    | InlineNode::LineBreak(_)
                    | InlineNode::Anchor(_)
                    | InlineNode::Autolink(_)
                    | InlineNode::Button(_)
                    | InlineNode::Icon(_)
                    | InlineNode::Image(_)
                    | InlineNode::IndexTerm(_)
                    | InlineNode::KeyboardShortcut(_)
                    | InlineNode::MenuPath(_)
                    | InlineNode::Passthrough(_)
                    | InlineNode::Stem(_)
                    | InlineNode::Callout(_)
                    | _ => None,
                };
                if let Some(children) = children {
                    walk(&children, transform, offset, visit)?;
                    continue;
                }
            }
            let _ = transform.write_inlines(offset, std::iter::once(InlineRef::from(node)));
        }
        Ok(())
    }
    walk(
        nodes,
        &InlineTextTransform::default()
            .line_break("\n")
            .decode_char_refs(true),
        &mut String::new(),
        visit,
    )
}

fn primary_content<'a>(term: &IndexTerm<'a>) -> Vec<InlineNode<'a>> {
    let mut prepared = term.clone();
    if let IndexTermKind::Flow(term) | IndexTermKind::Concealed { term, .. } =
        prepared.replace_kind(IndexTermKind::Flow(Vec::new()))
    {
        term
    } else {
        Vec::new()
    }
}

fn collect(nodes: &[InlineNode<'_>]) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    visit_index_terms(nodes, &mut |term, offset| {
        found.push((
            InlineTextTransform::default().to_string(&primary_content(term)),
            offset,
        ));
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap_or_else(|never| match never {});
    let mut typed = Vec::new();
    visit_index_inlines(nodes.iter().map(InlineRef::from), &mut |term, offset| {
        let text = InlineTextTransform::default().content_to_string(term.term());
        typed.push((text, offset));
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap_or_else(|never| match never {});
    assert_eq!(typed, found);
    found
}

#[test]
fn discovery_preserves_utf8_offsets_visibility_and_newlines() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, expected) in [
            (
                "é indexterm2:[first] indexterm:[hidden] indexterm2:[last]",
                vec![("first", 3), ("hidden", 9), ("last", 10)],
            ),
            (
                "a\nindexterm:[hidden]\nindexterm2:[visible]",
                vec![("hidden", 2), ("visible", 3)],
            ),
        ] {
            let parsed = parse_inline(source, &options)?;
            let parsed_inlines =
                acdc_parser::edit::ContentAttribute::from(parsed.content()).into_inlines();
            assert_eq!(
                collect(parsed_inlines.as_slice()),
                expected
                    .into_iter()
                    .map(|(text, offset)| (text.to_owned(), offset))
                    .collect::<Vec<_>>()
            );
        }
    }
    Ok(())
}

#[test]
fn discovery_descends_only_into_the_supported_display_content() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for source in [
            "*indexterm:[term]*",
            "_indexterm:[term]_",
            "`indexterm:[term]`",
            "#indexterm:[term]#",
            "~indexterm:[term]~",
            "^indexterm:[term]^",
            "\"`indexterm:[term]`\"",
            "'`indexterm:[term]`'",
            "link:page.html[indexterm:[term]]",
            "https://example.org[indexterm:[term]]",
            "mailto:someone@example.org[indexterm:[term]]",
            "xref:target[indexterm:[term]]",
            "footnote:[indexterm:[term]]",
        ] {
            let parsed = parse_inline(source, &options)?;
            let parsed_inlines =
                acdc_parser::edit::ContentAttribute::from(parsed.content()).into_inlines();
            assert_eq!(
                collect(parsed_inlines.as_slice()),
                [(String::from("term"), 0)],
                "{mode:?} {source:?}"
            );
        }
    }
    Ok(())
}

fn text(value: &'static str) -> InlineNode<'static> {
    InlineNode::Plain(Plain::new(value, Location::default()))
}

fn term(visible: bool, label: Vec<InlineNode<'static>>) -> IndexTerm<'static> {
    IndexTerm::new(
        if visible {
            IndexTermKind::Flow(label)
        } else {
            IndexTermKind::Concealed {
                term: label,
                secondary: None,
                tertiary: None,
            }
        },
        Location::default(),
    )
}

fn node(term: IndexTerm<'static>) -> InlineNode<'static> {
    InlineNode::IndexTerm(Box::new(term))
}

#[test]
fn discovery_does_not_register_label_hierarchy_or_relationship_content() {
    let nested = node(term(true, vec![text("Nested")]));
    let mut outer = term(true, vec![nested.clone()]);
    outer.replace_relationship(Some(IndexTermRelationship::See {
        target: vec![nested.clone()],
    }));
    let mut hidden = IndexTerm::new(
        IndexTermKind::Concealed {
            term: vec![text("Hidden")],
            secondary: Some(vec![nested.clone()]),
            tertiary: Some(vec![nested.clone()]),
        },
        Location::default(),
    );
    hidden.replace_relationship(Some(IndexTermRelationship::SeeAlso {
        targets: vec![vec![nested]],
    }));
    let nodes = [
        text("é "),
        node(outer),
        node(hidden),
        node(term(true, vec![text("Last")])),
    ];
    assert_eq!(
        collect(&nodes),
        [
            (String::from("Nested"), 3),
            (String::from("Hidden"), 9),
            (String::from("Last"), 9)
        ]
    );
}

#[test]
fn owner_callback_stops_at_the_first_error() -> Result<(), Error> {
    let parsed = parse_inline(
        "indexterm:[first] *indexterm:[second]* indexterm:[last]",
        &Options::default(),
    )?;
    let parsed_inlines = acdc_parser::edit::ContentAttribute::from(parsed.content()).into_inlines();
    let mut found = Vec::new();
    let result = visit_index_terms(parsed_inlines.as_slice(), &mut |term, offset| {
        found.push((
            InlineTextTransform::default().to_string(&primary_content(term)),
            offset,
        ));
        if found.len() == 2 {
            Err("stop")
        } else {
            Ok(())
        }
    });
    assert_eq!(result, Err("stop"));
    assert_eq!(
        found,
        [(String::from("first"), 0), (String::from("second"), 1)]
    );
    Ok(())
}

#[test]
fn owner_callbacks_keep_the_original_term_references() -> Result<(), Error> {
    let parsed = parse_inline("indexterm:[first] indexterm:[second]", &Options::default())?;
    let parsed_inlines = acdc_parser::edit::ContentAttribute::from(parsed.content()).into_inlines();
    let mut expected = parsed_inlines.as_slice().iter().filter_map(|node| {
        if let InlineNode::IndexTerm(term) = node {
            Some(term.as_ref())
        } else {
            None
        }
    });
    visit_index_terms(parsed_inlines.as_slice(), &mut |term, _| {
        assert!(std::ptr::eq(
            term,
            expected.next().ok_or("unexpected term")?
        ));
        Ok::<(), Error>(())
    })?;
    assert!(expected.next().is_none());
    Ok(())
}

#[test]
fn typed_discovery_stops_without_consuming_later_input() {
    let nodes = [
        text("é "),
        node(term(false, vec![text("First")])),
        text("unused"),
        node(term(false, vec![text("Last")])),
    ];
    let pulled = std::cell::Cell::new(0);
    let input = nodes
        .iter()
        .map(InlineRef::from)
        .inspect(|_| pulled.set(pulled.get() + 1));
    let result = visit_index_inlines(input, &mut |term, offset| {
        assert_eq!(
            InlineTextTransform::default().content_to_string(term.term()),
            "First"
        );
        assert_eq!(offset, 3);
        Err("stop")
    });
    assert_eq!(result, Err("stop"));
    assert_eq!(pulled.get(), 2);
}

#[test]
fn typed_discovery_accepts_temporary_input_and_empty_iterators() -> Result<(), Error> {
    let mut found = Vec::new();
    {
        let source = String::from("é indexterm2:[Temporary] indexterm:[Hidden]");
        let parsed = parse_inline(&source, &Options::default())?;
        visit_index_inlines(parsed.content().inlines(), &mut |term, offset| {
            found.push((
                InlineTextTransform::default().content_to_string(term.term()),
                offset,
            ));
            Ok::<(), Error>(())
        })?;
    }
    assert_eq!(
        found,
        [(String::from("Temporary"), 3), (String::from("Hidden"), 13)]
    );
    visit_index_inlines(std::iter::empty::<InlineRef<'_>>(), &mut |_, _| {
        Err("unexpected term")
    })?;
    Ok(())
}

#[test]
fn discovery_offsets_count_resolved_reference_characters() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_inline(
            r"&#169; indexterm2:[&#65;lpha] \&#169; indexterm:[hidden] *&#x1f600;* indexterm2:[last]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let parsed_inlines =
            acdc_parser::edit::ContentAttribute::from(parsed.content()).into_inlines();
        assert_eq!(
            collect(parsed_inlines.as_slice()),
            [
                ("&#65;lpha".to_owned(), "© ".len()),
                ("hidden".to_owned(), "© Alpha &#169; ".len()),
                ("last".to_owned(), "© Alpha &#169;  😀 ".len()),
            ]
        );
    }
    Ok(())
}
