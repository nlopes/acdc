//! Text extraction keeps the parser's selected substitution order.

use acdc_converters_core::InlineTextTransform;
use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn projection_preserves_text_when_replacements_are_disabled_or_already_finished()
-> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for source in [
            "[subs=none]\n(C) ->\n",
            ":literal: (C) ->\n\n[subs='replacements,attributes']\n{literal}\n",
        ] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let Some(BlockKind::Paragraph(paragraph)) =
                parsed.document().blocks().next().map(BlockRef::kind)
            else {
                return Err("paragraph".into());
            };
            let text = InlineTextTransform::default().content_to_string(paragraph.content());
            assert_eq!(text, "(C) ->", "{mode:?}: {source}");
            assert_eq!(
                InlineTextTransform::default()
                    .rendered()
                    .content_to_string(paragraph.content()),
                text,
                "{mode:?}: {source}"
            );
        }
    }
    Ok(())
}
