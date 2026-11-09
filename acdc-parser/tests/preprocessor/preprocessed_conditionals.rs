use std::{error::Error, fs, io::Cursor};

use acdc_parser::{CompatibilityMode, Options, SafeMode, parse, parse_file, parse_from_reader};
use rstest::rstest;

type TestResult = Result<(), Box<dyn Error>>;

#[rstest]
fn conditional_decisions_keep_source_order_and_recovery(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let source = concat!(
        "= Doc\n:header:\n\n:body:\n",
        "ifdef::body[]\nselected\nendif::[]\n",
        ":header!:\nifdef::header[]\nomitted\nendif::[]\n",
        "ifdef::missing[]\nifdef::body[]\nnested\nendif::[]\nendif::[]\n",
        "ifdef::body[inline]\n\\ifdef::body[]\n",
        "////\nifdef::body[]\n////\n",
        "ifdef::body[]\nrecovered\n",
    );
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    for mut parsed in [
        parse(source, &options)?,
        parse_from_reader(Cursor::new(source), &options)?,
    ] {
        let decisions = parsed
            .conditionals()
            .map(|evaluation| {
                assert!(evaluation.source_file().is_none());
                (
                    evaluation.start_line(),
                    evaluation.end_line(),
                    evaluation.is_active(),
                    evaluation.is_inline(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            decisions,
            [
                (5, Some(7), true, false),
                (9, Some(11), false, false),
                (12, Some(16), false, false),
                (13, Some(15), false, false),
                (17, None, true, true),
                (22, None, true, false),
            ]
        );
        assert!(parsed.preprocessed_source().contains("selected"));
        assert!(!parsed.preprocessed_source().contains("omitted"));
        assert!(!parsed.preprocessed_source().contains("nested"));
        assert!(parsed.source_recovery().is_some());
        assert_eq!(parsed.take_warnings().len(), 1);
        assert_eq!(parsed.conditionals().len(), 6);
    }
    Ok(())
}

#[rstest]
fn conditional_decisions_use_selected_include_attributes_and_original_lines(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> TestResult {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let child = directory.path().join("child.adoc");
    let nested = directory.path().join("nested.adoc");
    fs::write(
        &main,
        "= Doc\n\ninclude::child.adoc[tag=selected]\nifdef::included[]\nIncluded.\nendif::[]\nifdef::omitted[]\nOmitted.\nendif::[]\n",
    )?;
    fs::write(
        &child,
        "// tag::selected[]\ninclude::nested.adoc[lines=2..6]\n// end::selected[]\n:omitted:\n",
    )?;
    fs::write(
        &nested,
        ":omitted:\r\n:included:\r\nifeval::[1 == 1]\r\nifdef::included[]\r\né😀 selected.\r\nendif::[]\r\nendif::[]\r\n",
    )?;
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_safe_mode(SafeMode::Safe)
        .build()?;
    let parsed = parse_file(&main, &options)?;
    let decisions = parsed
        .conditionals()
        .map(|evaluation| {
            (
                evaluation.source_file(),
                evaluation.start_line(),
                evaluation.end_line(),
                evaluation.is_active(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        decisions,
        [
            (Some(nested.as_path()), 3, None, true),
            (Some(nested.as_path()), 4, Some(6), true),
            (Some(main.as_path()), 4, Some(6), true),
            (Some(main.as_path()), 7, Some(9), false),
        ]
    );
    assert!(parsed.preprocessed_source().contains("é😀 selected."));
    assert!(parsed.preprocessed_source().contains("Included."));
    assert!(!parsed.preprocessed_source().contains("Omitted."));
    assert!(parsed.source_recovery().is_some());
    Ok(())
}
