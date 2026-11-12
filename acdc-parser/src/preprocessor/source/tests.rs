use std::rc::Rc;

use crate::{CompatibilityMode, IncludeLoader, Options, SafeMode};

use super::{Preprocessor, SourceReader};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn emitted_sources_preserve_complete_preprocessing_and_include_separators() -> TestResult {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("part.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Safe)
            .with_include_loader(IncludeLoader::System)
            .with_base_dir(directory.path())
            .build()?;
        for included in [
            "",
            "\n",
            "\n\n",
            "a",
            "a\n",
            "a\n\n",
            "a\n\n\n",
            "é\r\n世界\n",
            ":active:\n\nifdef::active[]\nselected\nendif::[]\n\n",
        ] {
            std::fs::write(&file, included)?;
            for source in [
                "include::part.adoc[]",
                "include::part.adoc[leveloffset=+1]",
                "before\n\ninclude::part.adoc[]\n\nafter",
                "before\n\ninclude::part.adoc[leveloffset=+1]\n\nafter",
                "include::part.adoc[]\ninclude::part.adoc[]\nafter",
            ] {
                let arena = bumpalo::Bump::new();
                let expected = Preprocessor::process(source, &options, Rc::default())?;
                let mut reader = SourceReader::new(source, None, &options, Rc::default(), &arena)?;
                let mut text = String::new();
                while let Some(chunk) = reader.next()? {
                    assert_eq!(
                        chunk.start,
                        text.len(),
                        "{included:?} in {source:?}; emitted {text:?}, next {:?}",
                        chunk.text
                    );
                    text.push_str(chunk.text);
                    assert!(
                        chunk
                            .source_ranges
                            .iter()
                            .all(|range| range.end_offset <= text.len())
                    );
                }
                let actual = reader.finish()?;
                text.truncate(actual.text.len());
                assert_eq!(text, expected.text, "{included:?} in {source:?}");
                assert_eq!(actual.text, expected.text);
                assert_eq!(
                    format!("{:?}", actual.source_ranges),
                    format!("{:?}", expected.source_ranges)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn emitted_nested_include_pauses_before_later_directives() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("outer.adoc"),
        "configure::[]\n\nifdef::enabled[]\ninclude::inner.adoc[leveloffset=+1]\nendif::[]",
    )?;
    std::fs::write(directory.path().join("inner.adoc"), "== Inner\n\nText.")?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Safe)
        .with_include_loader(IncludeLoader::System)
        .with_base_dir(directory.path())
        .build()?;
    let arena = bumpalo::Bump::new();
    let mut reader = SourceReader::new(
        "include::outer.adoc[leveloffset=+1]\n\nAfter.",
        None,
        &options,
        Rc::default(),
        &arena,
    )?;
    let prefix = reader.next()?.ok_or("missing include scope")?;
    assert_eq!(prefix.text, ":leveloffset: +1\n\n");
    assert_eq!(prefix.leveloffset_scopes.len(), 1);
    let first = reader.next()?.ok_or("missing configure macro")?;
    assert_eq!(first.text, "configure::[]\n");
    assert_eq!(
        first
            .source_ranges
            .first()
            .ok_or("missing source range")?
            .file_chain,
        ["outer.adoc"]
    );
    reader
        .options
        .document_attributes
        .set_text("enabled".into(), "".into());
    let mut text = format!("{}{}", prefix.text, first.text);
    let mut inner = false;
    while let Some(chunk) = reader.next()? {
        eprintln!(
            "chunk {} expected {}: {:?}",
            chunk.start,
            text.len(),
            chunk.text
        );
        assert_eq!(chunk.start, text.len());
        for range in &chunk.source_ranges {
            if range.file_chain == ["outer.adoc", "inner.adoc"] {
                inner = true;
                assert_eq!(
                    chunk
                        .leveloffset_ranges
                        .iter()
                        .last()
                        .map(|offset| offset.value),
                    Some(2)
                );
            }
        }
        text.push_str(chunk.text);
    }
    let result = reader.finish()?;
    text.truncate(result.text.len());
    assert_eq!(text, result.text);
    assert!(inner);
    assert!(text.contains("== Inner"));
    Ok(())
}
