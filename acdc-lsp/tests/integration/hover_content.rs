use std::{error::Error, fs};

use serde_json::{Value, json};
use tower_lsp_server::ls_types::{Position, Range};

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

fn hover(
    client: &mut LspTestClient,
    uri: &str,
    position: Position,
) -> Result<Value, Box<dyn Error>> {
    Ok(client.send_request(
        "textDocument/hover",
        json!({"textDocument":{"uri":uri},"position":position}),
    )?)
}

#[test]
fn hover_attributes_keep_header_values_and_priority_over_link_text() -> TestResult {
    let project = Project::new("hover-attribute-content")?;
    fs::write(
        project.path().join("attributes.adoc"),
        ":included: From include\n",
    )?;
    let uri = project.uri("document.adoc")?;
    let source = concat!(
        "= Document\n:choice: *Header*\n:empty:\ninclude::attributes.adoc[]\n\n",
        ":choice: Body\n\n",
        "😀 {choice} {empty} {included} {missing} {max-include-depth}\n\n",
        "link:https://example.org[{choice}]\n",
    );
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    for (name, value) in [
        ("choice", "*Header*"),
        ("empty", ""),
        ("included", "From include"),
        ("max-include-depth", "64"),
    ] {
        let expected = range(source, &format!("{{{name}}}"))?;
        let result = hover(&mut client, &uri, expected.start)?;
        assert_eq!(
            result.pointer("/contents/value"),
            Some(&json!(format!("**Attribute**\n\n`:{name}:` = `{value}`")))
        );
        assert_eq!(result.get("range"), Some(&serde_json::to_value(expected)?));
    }
    let undefined = hover(&mut client, &uri, range(source, "{missing}")?.start)?;
    assert_eq!(
        undefined.pointer("/contents/value"),
        Some(&json!("**Attribute** (undefined)\n\n`{missing}`"))
    );
    let label = range(source, "link:https://example.org[{choice}]")?;
    let result = hover(
        &mut client,
        &uri,
        Position::new(label.start.line, label.start.character + 25),
    )?;
    assert_eq!(
        result.pointer("/contents/value"),
        Some(&json!("**Attribute**\n\n`:choice:` = `*Header*`"))
    );
    client.shutdown();
    Ok(())
}

#[test]
fn hover_distinguishes_reference_targets_from_occurrence_ids() -> TestResult {
    let project = Project::new("hover-reference-content")?;
    let uri = project.uri("document.adoc")?;
    let source = concat!(
        "[[alias]]\n[id=target]\n== *Rich* `Title`\n\n",
        "[id=paragraph]\nBody.\n\n",
        "😀 <<alias>> <<target>> <<paragraph>> <<missing>>\n\n",
        "xref:target[Label,id=occurrence]\n\n<<occurrence>>\n",
    );
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    for (id, suffix) in [
        ("alias", "\n\nSection: Rich Title"),
        ("target", "\n\nSection: Rich Title"),
        ("paragraph", ""),
    ] {
        let expected = range(source, &format!("<<{id}>>"))?;
        let result = hover(&mut client, &uri, expected.start)?;
        assert_eq!(
            result.pointer("/contents/value"),
            Some(&json!(format!(
                "**Cross-reference**\n\nTarget: `{id}`{suffix}"
            )))
        );
        assert_eq!(result.get("range"), Some(&serde_json::to_value(expected)?));
    }
    let missing = hover(&mut client, &uri, range(source, "<<missing>>")?.start)?;
    assert_eq!(
        missing.pointer("/contents/value"),
        Some(&json!(
            "**Cross-reference** (unresolved)\n\nTarget: `missing`"
        ))
    );
    let occurrence = range(source, "occurrence")?;
    let id = hover(&mut client, &uri, occurrence.start)?;
    assert_eq!(
        id.pointer("/contents/value"),
        Some(&json!("**Anchor**\n\nID: `occurrence`\n\n1 reference"))
    );
    assert_eq!(id.get("range"), Some(&serde_json::to_value(occurrence)?));
    let target = hover(&mut client, &uri, range(source, "xref:target")?.start)?;
    assert_eq!(
        target.pointer("/contents/value"),
        Some(&json!(
            "**Cross-reference**\n\nTarget: `target`\n\nSection: Rich Title"
        ))
    );
    client.shutdown();
    Ok(())
}

#[test]
fn hover_keeps_disk_and_open_document_resolution() -> TestResult {
    let project = Project::new("hover-cross-file-content")?;
    fs::write(project.path().join("chapter.adoc"), "[[disk]]\n== Disk\n")?;
    let uri = project.uri("document.adoc")?;
    let chapter = project.uri("chapter.adoc")?;
    let source = "😀 xref:chapter.adoc#disk[] xref:chapter.adoc#buffer[] xref:chapter.adoc[] <<elsewhere>>\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    for (id, prefix) in [
        ("disk", "**Cross-reference**"),
        ("buffer", "**Cross-reference** (anchor not found)"),
    ] {
        let result = hover(
            &mut client,
            &uri,
            range(source, &format!("xref:chapter.adoc#{id}"))?.start,
        )?;
        assert_eq!(
            result.pointer("/contents/value"),
            Some(&json!(format!(
                "{prefix}\n\nFile: `chapter.adoc`\n\nTarget: `{id}`"
            )))
        );
    }
    client.open_document(&chapter, "[[buffer]]\n== Unsaved\n\n[[elsewhere]]\nBody.\n")?;
    let buffer = hover(
        &mut client,
        &uri,
        range(source, "xref:chapter.adoc#buffer")?.start,
    )?;
    assert_eq!(
        buffer.pointer("/contents/value"),
        Some(&json!(
            "**Cross-reference**\n\nFile: `chapter.adoc`\n\nTarget: `buffer`"
        ))
    );
    let file = hover(
        &mut client,
        &uri,
        range(source, "xref:chapter.adoc[]")?.start,
    )?;
    assert_eq!(
        file.pointer("/contents/value"),
        Some(&json!("**Cross-reference**\n\nFile: `chapter.adoc`"))
    );
    let global = hover(&mut client, &uri, range(source, "<<elsewhere>>")?.start)?;
    assert_eq!(
        global.pointer("/contents/value"),
        Some(&json!(format!(
            "**Cross-reference**\n\nTarget: `elsewhere`\n\nDefined in: `{chapter}`"
        )))
    );
    client.shutdown();
    Ok(())
}
