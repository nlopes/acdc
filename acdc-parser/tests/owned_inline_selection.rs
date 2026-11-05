//! Inline selection copies only the selected value; typed content exports native JSON.

use std::{alloc::System, hint::black_box};

use acdc_parser::{
    self as parser, ContentAttribute, Form, InlineMacro, InlineNode, Location, Options, Plain,
    ast::{ContentRef, InlineKind, InlineRef},
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

fn plain(text: &str) -> InlineNode<'_> {
    InlineNode::PlainText(Plain::new(text, Location::default()).with_escaped(true))
}

fn atoms_and_spans(source: &str) -> Result<Vec<InlineNode<'_>>, Error> {
    let loc = Location::default();
    let mut values = vec![
        plain(source),
        InlineNode::Symbol(parser::Symbol::new(
            parser::SymbolKind::Copyright,
            loc.clone(),
        )),
        InlineNode::CharacterReference(
            parser::CharacterReference::new(
                parser::CharacterReferenceKind::Named("copy"),
                loc.clone(),
            )
            .ok_or("character reference")?,
        ),
        InlineNode::RawText(parser::Raw::new(
            source,
            vec![parser::Substitution::Attributes],
            loc.clone(),
        )),
        InlineNode::VerbatimText(parser::Verbatim::new(source, loc.clone())),
        InlineNode::StandaloneCurvedApostrophe(parser::StandaloneCurvedApostrophe::new(
            loc.clone(),
        )),
        InlineNode::LineBreak(parser::LineBreak::new(loc.clone())),
        InlineNode::InlineAnchor(parser::Anchor::new(source, loc.clone())),
        InlineNode::CalloutRef(parser::CalloutRef::auto(3, loc.clone())),
    ];
    macro_rules! span {
        ($owner:ident, $variant:ident) => {
            for form in [Form::Constrained, Form::Unconstrained] {
                values.push(InlineNode::$variant(parser::$owner::new(
                    vec![plain(source)],
                    form,
                    loc.clone(),
                )));
            }
        };
    }
    span!(Bold, BoldText);
    span!(Italic, ItalicText);
    span!(Monospace, MonospaceText);
    span!(Highlight, HighlightText);
    span!(Subscript, SubscriptText);
    span!(Superscript, SuperscriptText);
    span!(CurvedQuotation, CurvedQuotationText);
    span!(CurvedApostrophe, CurvedApostropheText);
    Ok(values)
}

fn macro_values(source: &str) -> Vec<InlineNode<'_>> {
    let loc = Location::default();
    let label = || ContentAttribute::new(vec![plain(source)]);
    vec![
        InlineNode::Macro(InlineMacro::Footnote(parser::Footnote::new(
            vec![plain(source)],
            7,
            loc.clone(),
        ))),
        InlineNode::Macro(InlineMacro::Icon(Box::new(parser::Icon::new(
            parser::Source::Name(source),
            loc.clone(),
        )))),
        InlineNode::Macro(InlineMacro::Image(Box::new(parser::InlineImage::new(
            parser::Source::Name(source),
            loc.clone(),
        )))),
        InlineNode::Macro(InlineMacro::Keyboard(parser::Keyboard::new(
            vec![label(), ContentAttribute::default()],
            loc.clone(),
        ))),
        InlineNode::Macro(InlineMacro::Button(parser::Button::new(
            label(),
            loc.clone(),
        ))),
        InlineNode::Macro(InlineMacro::Menu(parser::Menu::new(
            label(),
            vec![label(), ContentAttribute::default()],
            loc.clone(),
        ))),
        InlineNode::Macro(InlineMacro::Url(Box::new(parser::Url::new(
            parser::Source::Name(source),
            loc.clone(),
        )))),
        InlineNode::Macro(InlineMacro::Link(Box::new(parser::Link::new(
            parser::Source::Name(source),
            loc.clone(),
        )))),
        InlineNode::Macro(InlineMacro::Mailto(Box::new(parser::Mailto::new(
            parser::Source::Name(source),
            loc.clone(),
        )))),
        InlineNode::Macro(InlineMacro::Autolink(parser::Autolink::new(
            parser::Source::Name(source),
            loc.clone(),
        ))),
        InlineNode::Macro(InlineMacro::CrossReference(Box::new(
            parser::CrossReference::new(source, loc.clone()).with_text(vec![plain(source)]),
        ))),
        InlineNode::Macro(InlineMacro::Pass(parser::Pass::new(
            Some(source),
            vec![parser::Substitution::Quotes],
            parser::PassthroughKind::Macro,
            loc.clone(),
        ))),
        InlineNode::Macro(InlineMacro::Stem(parser::Stem::new(
            source,
            parser::StemNotation::Latexmath,
            loc.clone(),
        ))),
        InlineNode::Macro(InlineMacro::IndexTerm(Box::new(parser::IndexTerm::new(
            parser::IndexTermKind::Concealed {
                term: vec![plain(source)],
                secondary: Some(Vec::new()),
                tertiary: None,
            },
            loc,
        )))),
    ]
}

#[test]
fn generic_selected_copy_preserves_every_concrete_kind() -> Result<(), Error> {
    let source = String::from("λ *prepared* {entry} footnote:[literal]");
    let mut values = atoms_and_spans(&source)?;
    values.extend(macro_values(&source));
    assert_eq!(values.len(), 39);
    for original in &values {
        let read = InlineRef::from(original);
        let copy = InlineNode::from(read);
        assert_eq!(&copy, original);
        assert_eq!(
            serde_json::to_value(&copy)?,
            serde_json::to_value(original)?
        );
        assert_eq!(
            Location::from(InlineRef::from(&copy).location()),
            Location::from(read.location())
        );
    }
    Ok(())
}

#[test]
fn selecting_an_atom_does_not_copy_unrelated_siblings() -> Result<(), Error> {
    for count in [1, 1_000] {
        let content = ContentAttribute::new((0..count).map(|_| plain("*literal* λ")).collect());
        let first = ContentRef::from(&content)
            .inlines()
            .next()
            .ok_or("first inline")?;
        let region = Region::new(ALLOCATOR);
        let copied = black_box(InlineNode::from(first));
        let stats = region.change();
        assert_eq!(stats.allocations, 0);
        let InlineKind::Plain(text) = InlineRef::from(&copied).kind() else {
            return Err("plain copy".into());
        };
        assert_eq!(text.text(), "*literal* λ");
        assert!(text.is_escaped());
        assert_eq!(ContentRef::from(&content).inlines().count(), count);
    }
    Ok(())
}

#[test]
fn native_content_serialization_does_not_clone_the_tree() -> Result<(), Error> {
    let content = ContentAttribute::new((0..1_000).map(|_| plain("λ *literal*")).collect());
    let read = ContentRef::from(&content);
    let region = Region::new(ALLOCATOR);
    let result = serde_json::to_writer(std::io::sink(), &read);
    let stats = region.change();
    result?;
    assert_eq!(stats.allocations, 0);
    assert_eq!(
        serde_json::to_value(read)?,
        serde_json::to_value(content.into_inlines())?
    );
    Ok(())
}

#[rstest::rstest]
fn inline_parse_content_keeps_native_shape_and_selected_lifetimes(
    #[values(
        parser::CompatibilityMode::Asciidoctor,
        parser::CompatibilityMode::Draft
    )]
    mode: parser::CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parser::parse_inline(
        "*Bold λ* and image:photo.svg[Alt,title=Tip] footnote:n[Note] footnote:n[]",
        &options,
    )?;
    let before = serde_json::to_value(parsed.content())?;
    let copied = ContentAttribute::from(parsed.content()).into_inlines();
    assert_eq!(serde_json::to_value(&copied)?, before);
    assert_eq!(serde_json::to_value(parsed.content())?, before);
    assert_eq!(parsed.warnings(), []);
    #[cfg(feature = "asg")]
    {
        let parsed = parser::parse_inline("*ASG λ* and text", &options)?;
        let before = parser::asg::inlines(parsed.content())?;
        let copied = ContentAttribute::from(parsed.content());
        assert_eq!(parser::asg::inlines(ContentRef::from(&copied))?, before);
    }
    Ok(())
}
