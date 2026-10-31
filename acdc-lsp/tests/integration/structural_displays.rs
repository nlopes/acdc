use std::{error::Error, fs};

use serde_json::json;

use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn folding_keeps_block_coverage_order_and_line_only_ranges() -> TestResult {
    let project = Project::new("structural-folds")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    for (body, expected) in [
        ("----\nCode.\n----", vec![(2, 4, "region")]),
        ("....\nLiteral.\n....", vec![(2, 4, "region")]),
        ("++++\nRaw.\n++++", vec![(2, 4, "region")]),
        ("____\nQuote.\n____", vec![(2, 4, "region")]),
        ("****\nSidebar.\n****", vec![(2, 4, "region")]),
        ("====\nExample.\n====", vec![(2, 4, "region")]),
        ("--\nOpen.\n--", vec![(2, 4, "region")]),
        ("[verse]\n____\nVerse.\n____", vec![(2, 5, "region")]),
        ("[stem]\n++++\nx^2\n++++", vec![(2, 5, "region")]),
        ("////\nComment.\n////", vec![(2, 4, "comment")]),
        ("* One\n* Two", vec![(2, 3, "region")]),
        (". One\n. Two", vec![(2, 3, "region")]),
        ("One:: First\nTwo:: Second", vec![(2, 3, "region")]),
        ("NOTE: First\nSecond", vec![(2, 3, "region")]),
        ("== Heading\n\nBody.", vec![(2, 4, "region")]),
        (
            "--\n\n----\nCode.\n----\n\n--",
            vec![(2, 8, "region"), (4, 6, "region")],
        ),
        (
            "* Item\n+\n----\nCode.\n----",
            vec![(2, 6, "region"), (4, 6, "region")],
        ),
        (
            "[cols=a]\n|===\n|\n----\nCode.\n----\n|===",
            vec![(2, 8, "region")],
        ),
        ("<1> Item\n+\n----\nCode.\n----", vec![]),
        ("// Single comment", vec![]),
        ("[discrete]\n== Heading", vec![]),
    ] {
        let source = format!("= Doc\n\n{body}\n");
        client.open_document(&uri, &source)?;
        let folds = client.send_request(
            "textDocument/foldingRange",
            json!({"textDocument":{"uri":uri}}),
        )?;
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(start, end, kind)| json!({"startLine":start,"endLine":end,"kind":kind}))
            .collect();
        assert_eq!(folds, json!(expected), "{body}");
    }
    client.shutdown();
    Ok(())
}

#[test]
fn diagnostics_and_folds_preserve_included_heading_sources() -> TestResult {
    let project = Project::new("structural-includes")?;
    let uri = project.uri("parent.adoc")?;
    fs::write(
        project.path().join("child.adoc"),
        "// omitted\n// omitted\n    ==== 😀 Skipped\n\n    Foreign.\n",
    )?;
    let source = "= Doc\r\n\r\n== Parent\r\n\r\ninclude::child.adoc[lines=3..5,indent=0]\r\n\r\n😀 Local.\r\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    let diagnostics = client.wait_for_diagnostics(&uri)?;
    assert_eq!(diagnostics.get("diagnostics"), Some(&json!([])));
    let folds = client.send_request(
        "textDocument/foldingRange",
        json!({"textDocument":{"uri":uri}}),
    )?;
    assert_eq!(folds, json!([{"startLine":2,"endLine":6,"kind":"region"}]));
    client.shutdown();
    Ok(())
}

#[test]
fn section_diagnostics_keep_skip_messages_and_original_utf16_ranges() -> TestResult {
    let project = Project::new("structural-section-levels")?;
    let uri = project.uri("document.adoc")?;
    let source = "= Doc\r\n\r\n== Parent\r\n\r\n==== 😀 Skipped\r\n\r\nBody.\r\n\r\n== Back\r\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    let diagnostics = client.wait_for_diagnostics(&uri)?;
    assert_eq!(
        diagnostics.get("diagnostics"),
        Some(&json!([
            {"message":"section title out of sequence: expected level 2, got level 3", "severity":2, "source":"acdc", "range":{"start":{"line":4,"character":0},"end":{"line":4,"character":4}}},
            {"message":"Section level skipped: expected level 3 (`===`) but found level 4 (`====`)", "severity":2, "source":"acdc", "range":{"start":{"line":4,"character":0},"end":{"line":6,"character":5}}}
        ]))
    );
    client.shutdown();
    Ok(())
}
