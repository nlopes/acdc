use std::{error::Error, path::Path};

use acdc_parser::{Block, InlineNode, Options, ParseResult, parse_file};

type TestResult = Result<(), Box<dyn Error>>;

fn paragraph_texts(result: &ParseResult) -> Result<Vec<&str>, Box<dyn Error>> {
    result
        .document()
        .blocks
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return Err(format!("expected paragraph, got {block:?}").into());
            };
            let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
                return Err(format!("expected plain paragraph text, got {paragraph:?}").into());
            };
            Ok(text.content)
        })
        .collect()
}

#[test]
fn tag_selection_diagnostics_are_structured_and_located() -> TestResult {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/preprocessor");
    let main = fixture_dir.join("include_tag_diagnostics_main.adoc");
    let outer = fixture_dir.join("include_tag_diagnostics_outer.adoc");
    let target = fixture_dir.join("include_tag_diagnostics_target.adoc");

    let result = parse_file(&main, &Options::default())?;

    assert_eq!(
        paragraph_texts(&result)?,
        [
            "MAIN BEFORE",
            "OUTER BEFORE",
            "Selected.",
            "Between.",
            "OUTER AFTER",
            "MAIN AFTER"
        ]
    );

    let [unexpected, missing] = result.warnings() else {
        return Err(format!("expected two warnings, got {:?}", result.warnings()).into());
    };
    assert_eq!(
        unexpected.kind.to_string(),
        format!(
            "unexpected end tag 'wanted' at line 5 of include file: {}",
            target.display()
        )
    );
    assert_eq!(
        missing.kind.to_string(),
        format!(
            "tag 'missing' not found in include file: {}",
            target.display()
        )
    );

    for (warning, line) in [(unexpected, 3), (missing, 7)] {
        let Some(location) = warning.source_location() else {
            return Err("expected tag warning to have a source location".into());
        };
        assert_eq!(location.file.as_deref(), Some(outer.as_path()));
        assert_eq!(location.location.start.line, line);
    }

    Ok(())
}

#[test]
fn tag_selection_happens_before_nested_preprocessing() -> TestResult {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/preprocessor");
    let main = fixture_dir.join("include_selection_before_processing_main.adoc");
    let target = fixture_dir.join("include_selection_before_processing_target.adoc");
    let missing_inside = fixture_dir.join("missing-inside-selection.adoc");

    let result = parse_file(&main, &Options::default())?;

    assert_eq!(
        paragraph_texts(&result)?,
        [
            "MAIN BEFORE",
            "SELECTED BEFORE",
            "INNER SELECTED",
            "Unresolved directive in include_selection_before_processing_target.adoc - include::missing-inside-selection.adoc[]",
            "SELECTED AFTER.",
            "MAIN AFTER"
        ]
    );

    let [warning] = result.warnings() else {
        return Err(format!("expected one warning, got {:?}", result.warnings()).into());
    };
    assert_eq!(
        warning.kind.to_string(),
        format!("include file not found: {}", missing_inside.display())
    );
    let location = warning
        .source_location()
        .ok_or("expected selected nested-include warning location")?;
    assert_eq!(location.file.as_deref(), Some(target.as_path()));
    assert_eq!(location.location.start.line, 10);

    let inner = result
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            matches!(
                paragraph.content.as_slice(),
                [InlineNode::PlainText(text)] if text.content == "INNER SELECTED"
            )
            .then_some(paragraph)
        })
        .ok_or("expected selected nested paragraph")?;
    assert_eq!(inner.location.start.line, 1);
    assert_eq!(
        inner
            .location
            .start
            .file
            .as_ref()
            .map(|chain| chain.as_slice()),
        Some(
            [
                "include_selection_before_processing_target.adoc".to_string(),
                "include_selection_before_processing_inner.adoc".to_string(),
            ]
            .as_slice()
        )
    );

    Ok(())
}

#[test]
fn malformed_selected_tag_boundaries_are_structured_and_located() -> TestResult {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/preprocessor");
    let main = fixture_dir.join("include_tag_state_main.adoc");
    let mismatch = fixture_dir.join("include_tag_state_mismatch.adoc");
    let unclosed = fixture_dir.join("include_tag_state_unclosed.adoc");

    let result = parse_file(&main, &Options::default())?;

    assert_eq!(
        paragraph_texts(&result)?,
        [
            "MAIN BEFORE",
            "OUTER.",
            "INNER.",
            "STILL INNER.",
            "BETWEEN",
            "UNCLOSED CONTENT",
            "MAIN AFTER"
        ]
    );
    let [mismatched, detected_unclosed] = result.warnings() else {
        return Err(format!("expected two warnings, got {:?}", result.warnings()).into());
    };
    assert_eq!(
        mismatched.kind.to_string(),
        format!(
            "mismatched end tag (expected 'inner' but found 'outer') at line 7 of include file: {}",
            mismatch.display()
        )
    );
    assert_eq!(
        detected_unclosed.kind.to_string(),
        format!(
            "detected unclosed tag 'unclosed' starting at line 1 of include file: {}",
            unclosed.display()
        )
    );

    for (warning, line) in [(mismatched, 3), (detected_unclosed, 7)] {
        let location = warning
            .source_location()
            .ok_or("expected malformed tag warning location")?;
        assert_eq!(location.file.as_deref(), Some(main.as_path()));
        assert_eq!(location.location.start.line, line);
    }

    Ok(())
}
