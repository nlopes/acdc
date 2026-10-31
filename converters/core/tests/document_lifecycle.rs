//! Document hooks preserve framing, attribute replay, and first-error stopping.

use acdc_converters_core::{
    TraversalContext,
    visitor::{BlockVisitor, DocumentVisitor, Visitor},
};
use acdc_parser::{
    Block, CompatibilityMode, Document, DocumentAttribute, Header, Options, Paragraph, Section,
    ast::{BlockKind, BlockRef, ContentRef, DocumentRef, HeaderRef},
    parse,
};

type Error = Box<dyn std::error::Error>;
type Entry = (&'static str, Option<String>);

const SOURCE: &str =
    "= Doc\n:value: header\n\nIntro.\n\n:value: middle\n\n== Section\n\n:value!:\n\nBody.\n";

#[derive(Default)]
struct Trace {
    entries: Vec<Entry>,
    fail_at: Option<usize>,
}

impl Trace {
    fn record(
        &mut self,
        phase: &'static str,
        context: &TraversalContext<'_>,
    ) -> Result<(), &'static str> {
        let index = self.entries.len();
        self.entries.push((
            phase,
            context
                .get("value")
                .and_then(|value| value.text())
                .map(str::to_owned),
        ));
        if self.fail_at == Some(index) {
            Err("stop")
        } else {
            Ok(())
        }
    }
}

macro_rules! frame_hooks {
    ($document:ty) => {
        fn visit_document_start(
            &mut self,
            context: &mut TraversalContext<'doc>,
            _: $document,
        ) -> Result<(), Self::Error> {
            self.0.record("start", context)
        }

        fn visit_body_content_start(
            &mut self,
            context: &mut TraversalContext<'doc>,
            _: $document,
        ) -> Result<(), Self::Error> {
            self.0.record("body-start", context)
        }

        fn visit_preamble_start(
            &mut self,
            context: &mut TraversalContext<'doc>,
            _: $document,
        ) -> Result<(), Self::Error> {
            self.0.record("preamble-start", context)
        }

        fn visit_preamble_end(
            &mut self,
            context: &mut TraversalContext<'doc>,
            _: $document,
        ) -> Result<(), Self::Error> {
            self.0.record("preamble-end", context)
        }

        fn visit_document_supplements(
            &mut self,
            context: &mut TraversalContext<'doc>,
            _: $document,
        ) -> Result<(), Self::Error> {
            self.0.record("supplements", context)
        }

        fn visit_document_end(
            &mut self,
            context: &mut TraversalContext<'doc>,
            _: $document,
        ) -> Result<(), Self::Error> {
            self.0.record("end", context)
        }
    };
}

#[derive(Default)]
struct OwnerTrace(Trace);

impl<'doc> Visitor<'doc> for OwnerTrace {
    type Error = &'static str;

    frame_hooks!(&'doc Document<'doc>);

    fn visit_header(
        &mut self,
        context: &mut TraversalContext<'doc>,
        _: &Header<'_>,
    ) -> Result<(), Self::Error> {
        self.0.record("header", context)
    }

    fn before_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        _: &'doc Block<'doc>,
    ) -> Result<(), Self::Error> {
        self.0.record("before", context)
    }

    fn visit_document_attribute(
        &mut self,
        context: &mut TraversalContext<'doc>,
        _: &'doc DocumentAttribute<'doc>,
    ) -> Result<(), Self::Error> {
        self.0.record("attribute", context)
    }

    fn visit_paragraph(
        &mut self,
        context: &mut TraversalContext<'doc>,
        _: &Paragraph<'_>,
    ) -> Result<(), Self::Error> {
        self.0.record("paragraph", context)
    }

    fn visit_section(
        &mut self,
        context: &mut TraversalContext<'doc>,
        section: &'doc Section<'doc>,
    ) -> Result<(), Self::Error> {
        self.0.record("section", context)?;
        context.visit_blocks(self, &section.content)
    }
}

#[test]
fn owner_lifecycle_orders_framing_around_attribute_replay() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut context = TraversalContext::new(parsed.document_ref().attributes());
    let mut visitor = OwnerTrace::default();
    visitor.visit_document(&mut context, parsed.document())?;
    let expected = [
        ("start", Some("header")),
        ("header", Some("header")),
        ("body-start", Some("header")),
        ("preamble-start", Some("header")),
        ("before", Some("header")),
        ("paragraph", Some("header")),
        ("before", Some("header")),
        ("attribute", Some("middle")),
        ("preamble-end", Some("middle")),
        ("before", Some("middle")),
        ("section", Some("middle")),
        ("before", Some("middle")),
        ("attribute", None),
        ("before", None),
        ("paragraph", None),
        ("supplements", None),
        ("end", None),
    ];
    assert_eq!(
        visitor.0.entries,
        expected.map(|(phase, value)| (phase, value.map(str::to_owned)))
    );
    Ok(())
}

const PREAMBLES: &[(&str, bool)] = &[
    ("", false),
    ("= Doc\n", false),
    ("= Doc\n\nIntro.\n", false),
    ("Intro.\n\n== Section\n\nBody.\n", false),
    ("= Doc\n\n== Section\n\nBody.\n", false),
    ("= Doc\n\n// Lead\n\n== Section\n\nBody.\n", false),
    ("= Doc\n\nIntro.\n\n== Section\n\nBody.\n", true),
    (
        "= Doc\n\n[discrete]\n=== Heading\n\n== Section\n\nBody.\n",
        true,
    ),
];

#[test]
fn owner_preamble_requires_a_header_section_and_substantive_content() -> Result<(), Error> {
    for (source, wrapped) in PREAMBLES {
        let parsed = parse(source, &Options::default())?;
        let mut context = TraversalContext::new(parsed.document_ref().attributes());
        let mut visitor = OwnerTrace::default();
        visitor.visit_document(&mut context, parsed.document())?;
        for phase in ["preamble-start", "preamble-end"] {
            assert_eq!(
                visitor
                    .0
                    .entries
                    .iter()
                    .filter(|entry| entry.0 == phase)
                    .count(),
                usize::from(*wrapped),
                "{phase}: {source:?}"
            );
        }
        assert_eq!(visitor.0.entries.last().map(|entry| entry.0), Some("end"));
    }
    Ok(())
}

#[test]
fn owner_errors_stop_all_later_document_hooks() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut complete = OwnerTrace::default();
    complete.visit_document(
        &mut TraversalContext::new(parsed.document_ref().attributes()),
        parsed.document(),
    )?;
    for fail_at in 0..complete.0.entries.len() {
        let mut visitor = OwnerTrace(Trace {
            fail_at: Some(fail_at),
            ..Trace::default()
        });
        let mut context = TraversalContext::new(parsed.document_ref().attributes());
        assert_eq!(
            visitor.visit_document(&mut context, parsed.document()),
            Err("stop")
        );
        assert_eq!(
            visitor.0.entries.as_slice(),
            complete.0.entries.get(..=fail_at).ok_or("failure prefix")?
        );
    }
    Ok(())
}

#[derive(Default)]
struct TypedTrace<'doc>(Trace, Option<ContentRef<'doc>>);

impl<'doc> BlockVisitor<'doc> for TypedTrace<'doc> {
    type Error = &'static str;

    fn before_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        _: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        self.0.record("before", context)
    }

    fn visit_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error> {
        if let BlockKind::Section(section) = block.kind() {
            self.0.record("section", context)?;
            context.visit_blocks(self, section.blocks())
        } else if matches!(block.kind(), BlockKind::Paragraph(_)) {
            self.0.record("paragraph", context)
        } else if matches!(block.kind(), BlockKind::DocumentAttribute(_)) {
            self.0.record("attribute", context)
        } else {
            Ok(())
        }
    }
}

impl<'doc> DocumentVisitor<'doc> for TypedTrace<'doc> {
    frame_hooks!(DocumentRef<'doc>);

    fn visit_header(
        &mut self,
        context: &mut TraversalContext<'doc>,
        header: HeaderRef<'doc>,
    ) -> Result<(), Self::Error> {
        self.1 = header.title();
        self.0.record("header", context)
    }
}

#[test]
fn typed_and_owner_lifecycles_agree_in_both_modes() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for source in PREAMBLES.iter().map(|case| case.0).chain([SOURCE]) {
            let parsed = parse(source, &options)?;
            let mut owner = OwnerTrace::default();
            let mut typed = TypedTrace::default();
            let mut owner_context = TraversalContext::new(parsed.document_ref().attributes());
            let mut typed_context = TraversalContext::new(parsed.document_ref().attributes());
            owner.visit_document(&mut owner_context, parsed.document())?;
            typed.visit_document(&mut typed_context, parsed.document_ref())?;
            assert_eq!(typed.0.entries, owner.0.entries, "{mode:?}: {source:?}");
            assert_eq!(typed.1, parsed.document_ref().title());
        }
    }
    Ok(())
}

#[test]
fn attribute_only_preambles_remain_unwrapped_after_owner_edits() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut document = Document::default();
    let header = parsed.document().header.as_ref().ok_or("header")?;
    document.header = Some(Header::new(header.title.clone(), header.location.clone()));
    document.attributes = parsed.document().attributes.clone();
    document.blocks = parsed.document().blocks.clone();
    document
        .blocks
        .retain(|block| !matches!(block, Block::Paragraph(_)));
    let read = DocumentRef::from(&document);
    let mut owner = OwnerTrace::default();
    let mut typed = TypedTrace::default();
    owner.visit_document(&mut TraversalContext::new(read.attributes()), &document)?;
    typed.visit_document(&mut TraversalContext::new(read.attributes()), read)?;
    assert_eq!(typed.0.entries, owner.0.entries);
    assert!(typed.0.entries.iter().any(|entry| entry.0 == "attribute"));
    assert!(
        typed
            .0
            .entries
            .iter()
            .all(|entry| !entry.0.starts_with("preamble-"))
    );
    Ok(())
}

#[test]
fn typed_and_owner_errors_preserve_the_same_assignment_and_hook_prefix() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut complete = OwnerTrace::default();
    complete.visit_document(
        &mut TraversalContext::new(parsed.document_ref().attributes()),
        parsed.document(),
    )?;
    for fail_at in 0..complete.0.entries.len() {
        let trace = || Trace {
            fail_at: Some(fail_at),
            ..Trace::default()
        };
        let mut owner = OwnerTrace(trace());
        let mut typed = TypedTrace(trace(), None);
        let mut owner_context = TraversalContext::new(parsed.document_ref().attributes());
        let mut typed_context = TraversalContext::new(parsed.document_ref().attributes());
        assert_eq!(
            owner.visit_document(&mut owner_context, parsed.document()),
            Err("stop")
        );
        assert_eq!(
            typed.visit_document(&mut typed_context, parsed.document_ref()),
            Err("stop")
        );
        assert_eq!(typed.0.entries, owner.0.entries);
        assert_eq!(
            typed_context
                .attribute_assignment("value")
                .map(std::ptr::from_ref),
            owner_context
                .attribute_assignment("value")
                .map(std::ptr::from_ref)
        );
    }
    Ok(())
}

#[test]
fn typed_document_trait_objects_restore_parent_scopes_after_success_and_errors() -> Result<(), Error>
{
    let parent = parse("= Parent\n:value: parent\n\nBody.\n", &Options::default())?;
    let child = parse(SOURCE, &Options::default())?;
    let mut context = TraversalContext::new(parent.document_ref().attributes());
    for fail_at in [None, Some(8)] {
        let mut trace = TypedTrace(
            Trace {
                fail_at,
                ..Trace::default()
            },
            None,
        );
        let visitor: &mut dyn DocumentVisitor<'_, Error = &'static str> = &mut trace;
        let result = context.with_scope(|context| {
            let result = visitor.visit_document(context, child.document_ref());
            assert!(context.is_nested_document());
            assert_eq!(
                context.get("value").and_then(|value| value.text()),
                fail_at.map(|_| "middle")
            );
            result
        });
        assert_eq!(result, fail_at.map_or(Ok(()), |_| Err("stop")));
        assert_eq!(
            context.get("value").and_then(|value| value.text()),
            Some("parent")
        );
        assert!(!context.is_nested_document());
    }
    Ok(())
}
