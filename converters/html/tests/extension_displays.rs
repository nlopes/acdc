use std::{error::Error, path::Path};

use acdc_converters_core::Options;
use acdc_converters_dev::fixtures::parse_fixture_with_options;
use acdc_converters_html::{Converter, HtmlVariant, RenderOptions};
use acdc_parser::{CompatibilityMode, Options as ParserOptions, edit::Document};

#[test]
fn extension_displays_repeat_without_refreshing_titles_or_mutating_shared_document()
-> Result<(), Box<dyn Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/source/html/embedded/extension_displays.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?;
        let parsed = parse_fixture_with_options(&path, &options)?;
        let original = Document::from(parsed.document());
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let converter =
                Converter::new_with_variant(Options::default(), ParserOptions::builder(), variant)?;
            let render = RenderOptions {
                embedded: true,
                ..RenderOptions::default()
            };
            let first = converter.convert_to_string(parsed.document(), &render)?;
            let second = converter.convert_to_string(parsed.document(), &render)?;
            assert_eq!(first, second);
            assert!(
                first.contains("href=\"#renamed\">Dot <strong>Early</strong> &lt;&amp;&gt;</a>")
            );
            assert!(first.contains("href=\"#renamed\"><strong>Updated</strong> &lt;&amp;&gt;</a> Dot <strong>Early</strong>"));
            assert!(!first.contains("Custom <strong>reference</strong></a>"));
            assert!(!first.contains("&amp;lt;"));
            assert_eq!(Document::from(parsed.document()), original);
        }
    }
    Ok(())
}
