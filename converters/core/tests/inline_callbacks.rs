//! Inline callbacks keep document attributes while visiting temporary content.

use acdc_converters_core::{
    TraversalContext,
    visitor::{BlockVisitor, DocumentVisitor, InlineVisitor, Visitor, is_formatting_span},
};
use acdc_parser::{
    CompatibilityMode, ContentAttribute, InlineNode, Location, Options, Plain,
    ast::{BlockKind, BlockRef, ContentRef, HeaderRef, InlineKind, InlineRef},
    parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "*Bold* footnote:[Hidden]";

#[derive(Default)]
struct Trace<'doc> {
    entries: Vec<(&'static str, Option<&'doc str>)>,
    text: String,
    fail_at: Option<usize>,
}

impl<'doc> Trace<'doc> {
    fn record(
        &mut self,
        context: &TraversalContext<'doc>,
        node: InlineRef<'_>,
    ) -> Result<(), &'static str> {
        let label = if let InlineKind::Plain(plain) = node.kind() {
            self.text.push_str(plain.text());
            "plain"
        } else if matches!(node.kind(), InlineKind::Bold(_)) {
            "bold"
        } else if matches!(node.kind(), InlineKind::Footnote(_)) {
            "footnote"
        } else {
            "other"
        };
        let index = self.entries.len();
        self.entries
            .push((label, context.get("marker").and_then(|value| value.text())));
        if self.fail_at == Some(index) {
            Err("stop")
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct OwnerTrace<'doc>(Trace<'doc>);

impl<'doc> Visitor<'doc> for OwnerTrace<'doc> {
    type Error = &'static str;

    fn visit_inline_node(
        &mut self,
        context: &mut TraversalContext<'doc>,
        node: &InlineNode<'_>,
    ) -> Result<(), Self::Error> {
        self.0.record(context, node.into())
    }
}

#[test]
fn owner_inline_sequence_visits_only_selected_direct_nodes() -> Result<(), Error> {
    let document = parse("= Doc\n:marker: retained\n\nBody.\n", &Options::default())?;
    let mut context = TraversalContext::new(document.document_ref().attributes());
    let mut visitor = OwnerTrace::default();
    {
        let fragment = parse_inline(SOURCE, &Options::default())?;
        visitor.visit_inline_nodes(&mut context, fragment.inlines())?;
    }
    assert_eq!(
        visitor.0.entries,
        [
            ("bold", Some("retained")),
            ("plain", Some("retained")),
            ("footnote", Some("retained")),
        ]
    );
    assert_eq!(visitor.0.text, " ");
    Ok(())
}

#[test]
fn owner_inline_errors_stop_the_sequence_and_retain_attribute_borrows() -> Result<(), Error> {
    let document = parse("= Doc\n:marker: retained\n\nBody.\n", &Options::default())?;
    let mut context = TraversalContext::new(document.document_ref().attributes());
    for fail_at in 0..3 {
        let mut visitor = OwnerTrace(Trace {
            fail_at: Some(fail_at),
            ..Trace::default()
        });
        {
            let fragment = parse_inline(SOURCE, &Options::default())?;
            assert_eq!(
                visitor.visit_inline_nodes(&mut context, fragment.inlines()),
                Err("stop")
            );
        }
        assert_eq!(visitor.0.entries.len(), fail_at + 1);
        assert!(
            visitor
                .0
                .entries
                .iter()
                .all(|entry| entry.1 == Some("retained"))
        );
    }
    Ok(())
}

const SPANS: &[(&str, bool)] = &[
    ("*bold*", true),
    ("_italic_", true),
    ("`monospace`", true),
    ("#highlight#", true),
    ("~subscript~", true),
    ("^superscript^", true),
    ("\"`quotation`\"", true),
    ("'`apostrophe`'", true),
    ("plain", false),
    ("é 😀", false),
    ("word's", false),
    ("image:photo.png[Alt]", false),
    ("footnote:[*Nested*]", false),
    ("https://example.org[*Label*]", false),
];

#[test]
fn owner_formatting_span_policy_covers_eight_spans_and_excludes_nested_fields() -> Result<(), Error>
{
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, expected) in SPANS {
            let parsed = parse_inline(source, &options)?;
            assert_eq!(
                parsed.inlines().iter().any(is_formatting_span),
                *expected,
                "{mode:?}: {source}"
            );
        }
    }
    Ok(())
}

#[derive(Default)]
struct TypedTrace<'doc>(Trace<'doc>);

#[test]
fn typed_document_block_and_inline_callbacks_compose_in_one_converter() -> Result<(), Error> {
    #[derive(Default)]
    struct Rendered(String);

    impl<'doc> BlockVisitor<'doc> for Rendered {
        type Error = &'static str;

        fn visit_block(
            &mut self,
            context: &mut TraversalContext<'doc>,
            block: BlockRef<'doc>,
        ) -> Result<(), Self::Error> {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                self.visit_inline_nodes(context, paragraph.content())?;
            }
            Ok(())
        }
    }

    impl<'doc> DocumentVisitor<'doc> for Rendered {
        fn visit_header(
            &mut self,
            context: &mut TraversalContext<'doc>,
            header: HeaderRef<'doc>,
        ) -> Result<(), Self::Error> {
            if let Some(title) = header.title() {
                self.visit_inline_nodes(context, title)?;
            }
            Ok(())
        }
    }

    impl<'doc> InlineVisitor<'doc> for Rendered {
        type Error = &'static str;

        fn visit_inline_node(
            &mut self,
            context: &mut TraversalContext<'doc>,
            node: InlineRef<'_>,
        ) -> Result<(), Self::Error> {
            if let InlineKind::Plain(plain) = node.kind() {
                self.0.push_str(plain.text());
            } else if let InlineKind::Bold(bold) = node.kind() {
                self.visit_inline_nodes(context, bold.content())?;
            }
            Ok(())
        }
    }

    let parsed = parse("= Header\n\nBody *Bold*.\n", &Options::default())?;
    let mut rendered = Rendered::default();
    rendered.visit_document(
        &mut TraversalContext::new(parsed.document_ref().attributes()),
        parsed.document_ref(),
    )?;
    assert_eq!(rendered.0, "HeaderBody Bold.");
    Ok(())
}

impl<'doc> InlineVisitor<'doc> for TypedTrace<'doc> {
    type Error = &'static str;

    fn visit_inline_node(
        &mut self,
        context: &mut TraversalContext<'doc>,
        node: InlineRef<'_>,
    ) -> Result<(), Self::Error> {
        self.0.record(context, node)
    }
}

#[test]
fn typed_and_owner_inline_sequences_agree_after_temporary_fragments_are_dropped()
-> Result<(), Error> {
    let document = parse("= Doc\n:marker: retained\n\nBody.\n", &Options::default())?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for fail_at in [None, Some(0), Some(1), Some(2)] {
            let trace = || Trace {
                fail_at,
                ..Trace::default()
            };
            let mut owner = OwnerTrace(trace());
            let mut typed = TypedTrace(trace());
            let mut context = TraversalContext::new(document.document_ref().attributes());
            {
                let source = String::from(SOURCE);
                let fragment = parse_inline(&source, &options)?;
                let expected = owner.visit_inline_nodes(&mut context, fragment.inlines());
                let actual = typed.visit_inline_nodes(&mut context, fragment.content());
                assert_eq!(actual, expected);
            }
            assert_eq!(typed.0.entries, owner.0.entries);
            assert_eq!(typed.0.text, owner.0.text);
            assert!(
                typed
                    .0
                    .entries
                    .iter()
                    .all(|entry| entry.1 == Some("retained"))
            );
        }
    }
    Ok(())
}

#[test]
fn typed_trait_objects_accept_constructed_text_shorter_than_the_document() -> Result<(), Error> {
    let document = parse("= Doc\n:marker: retained\n\nBody.\n", &Options::default())?;
    let mut context = TraversalContext::new(document.document_ref().attributes());
    let mut trace = TypedTrace::default();
    let visitor: &mut dyn InlineVisitor<'_, Error = &'static str> = &mut trace;
    {
        let normalized = String::from("normalized");
        let fragment = ContentAttribute::new(vec![InlineNode::PlainText(Plain {
            content: &normalized,
            location: Location::default(),
            escaped: false,
        })]);
        visitor.visit_inline_nodes(&mut context, ContentRef::from(&fragment))?;
    }
    {
        let fragment = parse_inline(" fragment", &Options::default())?;
        visitor.visit_inline_nodes(&mut context, fragment.content())?;
    }
    let empty = ContentAttribute::default();
    visitor.visit_inline_nodes(&mut context, ContentRef::from(&empty))?;
    assert_eq!(trace.0.text, "normalized fragment");
    assert_eq!(trace.0.entries.len(), 2);
    assert!(
        trace
            .0
            .entries
            .iter()
            .all(|entry| entry.1 == Some("retained"))
    );
    Ok(())
}

#[test]
fn typed_callbacks_choose_span_children_without_visiting_footnote_bodies() -> Result<(), Error> {
    #[derive(Default)]
    struct Selected<'doc>(Trace<'doc>);

    impl<'doc> InlineVisitor<'doc> for Selected<'doc> {
        type Error = &'static str;

        fn visit_inline_node(
            &mut self,
            context: &mut TraversalContext<'doc>,
            node: InlineRef<'_>,
        ) -> Result<(), Self::Error> {
            self.0.record(context, node)?;
            if let InlineKind::Bold(span) = node.kind() {
                self.visit_inline_nodes(context, span.content())?;
            }
            Ok(())
        }
    }

    let document = parse("Body.\n", &Options::default())?;
    let mut context = TraversalContext::new(document.document_ref().attributes());
    let mut selected = Selected::default();
    let fragment = parse_inline(SOURCE, &Options::default())?;
    selected.visit_inline_nodes(&mut context, fragment.content())?;
    assert_eq!(selected.0.text, "Bold ");
    assert_eq!(
        selected
            .0
            .entries
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        ["bold", "plain", "plain", "footnote"]
    );
    Ok(())
}

#[test]
fn standalone_content_reads_preserve_source_spans_and_borrowed_text() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, expected) in SPANS {
            let fragment = parse_inline(source, &options)?;
            let read = fragment.content();
            assert_eq!(read.inlines().len(), fragment.inlines().len());
            for (typed, owner) in read.inlines().zip(fragment.inlines()) {
                assert_eq!(Location::from(typed.location()), *owner.location());
                if let (InlineKind::Plain(typed), InlineNode::PlainText(owner)) =
                    (typed.kind(), owner)
                {
                    assert!(std::ptr::eq(typed.text(), owner.content));
                }
            }
            assert_eq!(
                read.inlines().any(is_formatting_span),
                *expected,
                "{mode:?}: {source}"
            );
        }
    }
    Ok(())
}
