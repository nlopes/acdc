//! Document header parsing in each compatibility mode.

use acdc_parser::{CompatibilityMode, Options, parse};

type Error = Box<dyn std::error::Error>;

const ASCIIDOCTOR: CompatibilityMode = CompatibilityMode::Asciidoctor;
const DRAFT: CompatibilityMode = CompatibilityMode::Draft;

#[test]
fn title_footnote_is_registered_once() -> Result<(), Error> {
    for source in [
        "= Title footnote:[Header note.]\n\nBody\n",
        "[[top]]\n= Title footnote:[Header note.]\n\nBody\n",
        ":a: 1\n\n[[top]]\n\n= Title footnote:[Header note.]\n\nBody\n",
    ] {
        for mode in [ASCIIDOCTOR, DRAFT] {
            let parsed = parse(
                source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            assert_eq!(parsed.document().footnotes().count(), 1, "{mode}: {source}");
        }
    }
    Ok(())
}
