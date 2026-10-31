use std::error::Error;

use serde_json::{Value, json};
use tower_lsp_server::ls_types::{CodeLens, InlayHint, InlayHintLabel, Position, Range};

use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

fn range(source: &str, needle: &str) -> Result<Range, Box<dyn Error>> {
    let (line, text, byte) = source
        .lines()
        .enumerate()
        .find_map(|(line, text)| text.find(needle).map(|byte| (line, text, byte)))
        .ok_or("source occurrence")?;
    let start = u32::try_from(text.get(..byte).ok_or("prefix")?.encode_utf16().count())?;
    Ok(Range::new(
        Position::new(u32::try_from(line)?, start),
        Position::new(
            u32::try_from(line)?,
            start + u32::try_from(needle.encode_utf16().count())?,
        ),
    ))
}

fn list<T: serde::de::DeserializeOwned>(value: Value) -> Result<Vec<T>, serde_json::Error> {
    if value.is_null() {
        Ok(Vec::new())
    } else {
        serde_json::from_value(value)
    }
}

#[test]
fn inlay_hints_and_code_lenses_keep_their_body_projection() -> TestResult {
    let project = Project::new("reference-display-projection")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    for (context, body, visible) in [
        ("paragraph", "😀 NODE", true),
        ("bold", "*NODE*", true),
        ("italic", "_NODE_", true),
        ("monospace", "`NODE`", true),
        ("highlight", "#NODE#", true),
        ("subscript", "~NODE~", true),
        ("superscript", "^NODE^", true),
        ("curved", "\"`NODE`\"", false),
        ("heading", "=== NODE", false),
        ("title", ".NODE\nBody.", false),
        ("sidebar", "****\nNODE\n****", true),
        ("quote", "____\nNODE\n____", true),
        ("open", "--\nNODE\n--", true),
        ("example", "====\nNODE\n====", true),
        ("admonition", "NOTE: NODE", true),
        ("list", "* NODE", true),
        ("attachment", "* Item\n+\nNODE", true),
        ("description", "Term:: NODE", true),
        ("term", "NODE:: Body.", false),
        ("callout", "<1> NODE", false),
        ("table", "[cols=a]\n|===\n|NODE\n|===", false),
        ("footnote", "footnote:[NODE]", false),
        ("alt", "image:x[alt='NODE']", false),
        ("listing", "[subs=macros]\n----\nNODE\n----", true),
        ("verse", "[verse]\n____\nNODE\n____", true),
    ] {
        let source = format!(
            "[[target]]\n== *Target*\n\n{}\n",
            body.replace("NODE", "[[spot]] <<target>>")
        );
        client.open_document(&uri, &source)?;
        let xref = range(&source, "<<target>>")?;
        let requested = Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX));
        let hints: Vec<InlayHint> = list(client.send_request(
            "textDocument/inlayHint",
            json!({"textDocument":{"uri":uri},"range":requested}),
        )?)?;
        assert_eq!(hints.len(), usize::from(visible), "{context}: {hints:?}");
        if let Some(hint) = hints.first() {
            assert_eq!(hint.position, xref.end, "{context}");
            assert!(matches!(&hint.label, InlayHintLabel::String(label) if label == "→ Target"));
        }
        let anchor = range(&source, "[[spot]]")?;
        let lenses: Vec<CodeLens> = list(
            client.send_request("textDocument/codeLens", json!({"textDocument":{"uri":uri}}))?,
        )?;
        assert_eq!(
            lenses.iter().any(|lens| lens.range.start == anchor.start),
            visible,
            "{context}: {lenses:?}"
        );
    }
    client.shutdown();
    Ok(())
}

#[test]
fn inlay_hints_use_registered_section_labels_and_preserve_range_edges() -> TestResult {
    let project = Project::new("reference-display-catalog")?;
    let uri = project.uri("document.adoc")?;
    let source = concat!(
        "[[alias]]\n[#target,reftext='*Label*']\n== Title\n\n",
        "[#paragraph]\n.Block\nBody.\n\n",
        "😀 <<alias>> <<target>> <<target,Explicit>> <<paragraph>> xref:other.adoc#target[]\n",
    );
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    let all = Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX));
    let hints: Vec<InlayHint> = list(client.send_request(
        "textDocument/inlayHint",
        json!({"textDocument":{"uri":uri},"range":all}),
    )?)?;
    assert_eq!(hints.len(), 1, "{hints:?}");
    let target = range(source, "<<target>>")?;
    let hint = hints.first().ok_or("target hint")?;
    assert!(matches!(&hint.label, InlayHintLabel::String(label) if label == "→ Label"));
    assert_eq!(hint.position, target.end);
    let point = Range::new(target.end, target.end);
    let at_end: Vec<InlayHint> = list(client.send_request(
        "textDocument/inlayHint",
        json!({"textDocument":{"uri":uri},"range":point}),
    )?)?;
    assert_eq!(serde_json::to_value(at_end)?, serde_json::to_value(hints)?);
    let before = Range::new(
        target.start,
        Position::new(target.end.line, target.end.character - 1),
    );
    let excluded: Vec<InlayHint> = list(client.send_request(
        "textDocument/inlayHint",
        json!({"textDocument":{"uri":uri},"range":before}),
    )?)?;
    assert!(excluded.is_empty());
    client.shutdown();
    Ok(())
}
