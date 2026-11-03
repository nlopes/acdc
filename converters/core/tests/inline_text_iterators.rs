//! Text extraction retains its policy across whole content and isolated fragments.

use std::fmt;

use acdc_converters_core::{InlineTextTransform, substitutions::TextBoundaries};
use acdc_parser::{
    Block, CompatibilityMode, Options,
    ast::{InlineRef, ParagraphRef},
    parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn text_policy_preserves_content_and_fragment_context() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, semantic, rendered) in [
            (
                "A *bold* image:bird.svg[Bird] indexterm2:[--] end.",
                "A bold [Bird] -- end.",
                "A bold [Bird]\u{2009}—\u{2009}end.",
            ),
            (
                "prefixindexterm2:[--]tail",
                "prefix--tail",
                "prefix—\u{200b}tail",
            ),
            (
                r"pass:r[(C) \(R) \*literal\* \->]",
                r"© (R) \*literal\* \->",
                r"© (R) \*literal\* \->",
            ),
            ("First +\nsecond.", "First\nsecond.", "First\nsecond."),
        ] {
            let parsed = parse(source, &options)?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("one paragraph".into());
            };
            for (transform, expected) in [
                (InlineTextTransform::default().line_break("\n"), semantic),
                (
                    InlineTextTransform::default()
                        .line_break("\n")
                        .rendered_replacements(true),
                    rendered,
                ),
            ] {
                let mut content = String::new();
                transform.write_inlines(
                    &mut content,
                    ParagraphRef::from(paragraph).content().inlines(),
                )?;
                let mut owner_content = String::new();
                transform.write(&mut owner_content, &paragraph.content)?;
                assert_eq!(content, owner_content);
                assert_eq!(content, expected, "{source:?} {mode:?}");
                let mut fragments = String::new();
                for (index, node) in paragraph.content.iter().enumerate() {
                    transform
                        .text_boundaries(TextBoundaries::BOTH.for_inline(&paragraph.content, index))
                        .write_inlines(&mut fragments, std::iter::once(InlineRef::from(node)))?;
                }
                assert_eq!(fragments, content);
            }
        }
    }
    Ok(())
}

#[test]
fn text_writing_stops_at_the_first_writer_error() -> Result<(), Error> {
    struct RejectText(usize);
    impl fmt::Write for RejectText {
        fn write_str(&mut self, _: &str) -> fmt::Result {
            self.0 += 1;
            Err(fmt::Error)
        }
    }
    let parsed = parse_inline("First *nested* last.", &Options::default())?;
    let mut writer = RejectText(0);
    assert!(
        InlineTextTransform::default()
            .write_inlines(&mut writer, parsed.content().inlines())
            .is_err()
    );
    assert_eq!(writer.0, 1);
    let mut empty = String::new();
    InlineTextTransform::default()
        .write_inlines(&mut empty, std::iter::empty::<InlineRef<'_>>())?;
    assert_eq!(empty, "");
    Ok(())
}

#[test]
fn typed_text_writing_accepts_temporary_inputs_and_appends_to_the_writer() -> Result<(), Error> {
    let document = parse("[[target]]\n== Destination\n", &Options::default())?;
    let transform = InlineTextTransform::default().references(&document.document().references);
    let mut output = String::from("Prefix: ");
    {
        let source = String::from("*Temporary* <<target>>");
        let parsed = parse_inline(&source, &Options::default())?;
        transform.write_inlines(&mut output, parsed.content().inlines())?;
    }
    assert_eq!(output, "Prefix: Temporary Destination");
    Ok(())
}
