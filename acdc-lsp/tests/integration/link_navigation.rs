use std::{error::Error, fs};

use serde_json::json;
use tower_lsp_server::ls_types::{DocumentLink, Position, Range};

use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

fn range(source: &str, needle: &str) -> Result<Range, Box<dyn Error>> {
    let (line, text, byte) = source
        .lines()
        .enumerate()
        .find_map(|(line, text)| text.find(needle).map(|byte| (line, text, byte)))
        .ok_or("source occurrence")?;
    let start = u32::try_from(text.get(..byte).ok_or("prefix")?.encode_utf16().count())?;
    let end = start + u32::try_from(needle.encode_utf16().count())?;
    Ok(Range::new(
        Position::new(u32::try_from(line)?, start),
        Position::new(u32::try_from(line)?, end),
    ))
}

#[test]
fn link_navigation_keeps_its_body_and_formatting_coverage() -> TestResult {
    let project = Project::new("link-navigation-coverage")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    let markup = "link:https://example.org/target[*Shown*,title='Tooltip']";
    for (context, body, visible) in [
        ("paragraph", "😀 LINK", true),
        ("bold", "*LINK*", true),
        ("italic", "_LINK_", true),
        ("monospace", "`LINK`", true),
        ("highlight", "#LINK#", true),
        ("subscript", "~LINK~", true),
        ("superscript", "^LINK^", true),
        ("curved quote", "\"`LINK`\"", false),
        ("section body", "== Heading\n\nLINK", true),
        ("section title", "== LINK\n\nBody.", false),
        ("block title", ".LINK\nBody.", false),
        ("sidebar", "****\nLINK\n****", true),
        ("quote", "____\nLINK\n____", true),
        ("open", "--\nLINK\n--", true),
        ("example", "====\nLINK\n====", true),
        ("admonition", "NOTE: LINK", true),
        ("list principal", "* LINK", true),
        ("list attachment", "* Item\n+\nLINK", true),
        ("description principal", "Term:: LINK", true),
        ("description term", "LINK:: Body.", false),
        ("callout", "<1> LINK", false),
        ("table", "[cols=a]\n|===\n|LINK\n|===", false),
        ("footnote", "footnote:[LINK]", false),
        ("image alt", "image:bird.png[alt='LINK']", false),
        ("listing macros", "[subs=macros]\n----\nLINK\n----", true),
        ("verse", "[verse]\n____\nLINK\n____", true),
    ] {
        let source = format!("= Document\n\n{}\n\n", body.replace("LINK", markup));
        client.open_document(&uri, &source)?;
        let links: Vec<DocumentLink> = serde_json::from_value(client.send_request(
            "textDocument/documentLink",
            json!({"textDocument":{"uri":uri}}),
        )?)?;
        assert_eq!(links.len(), usize::from(visible), "{context}");
        let expected = range(&source, markup)?;
        if let Some(link) = links.first() {
            assert_eq!(link.range, expected, "{context}");
            let label = if context == "listing macros" {
                "*Shown*"
            } else {
                "Shown"
            };
            assert_eq!(link.tooltip.as_deref(), Some(label), "{context}");
            assert_eq!(
                link.target.as_ref().map(|uri| uri.as_str()),
                Some("https://example.org/target")
            );
        }
        let position = Position::new(expected.start.line, expected.start.character + 6);
        let hover = client.send_request(
            "textDocument/hover",
            json!({"textDocument":{"uri":uri},"position":position}),
        )?;
        assert_eq!(
            hover
                .pointer("/contents/value")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.starts_with("**Link**")),
            visible,
            "{context}: {hover}"
        );
        if visible {
            assert_eq!(
                hover.get("range"),
                Some(&serde_json::to_value(expected)?),
                "{context}"
            );
        }
    }
    client.shutdown();
    Ok(())
}

#[test]
fn link_targets_keep_mailto_image_and_include_policies() -> TestResult {
    let project = Project::new("link-navigation-targets")?;
    let uri = project.uri("document.adoc")?;
    fs::write(
        project.path().join("chapter.adoc"),
        "// omitted\n// omitted\n    link:https://example.org/included[Included]\n",
    )?;
    let source = concat!(
        "= Document\n\n",
        "😀 link:local.adoc[Local] https://example.org/url[URL] <https://example.org/auto> mailto:team@example.org[Team]\n\n",
        "image::bird.png[]\n\n",
        "include::chapter.adoc[lines=3,indent=0]\n",
    );
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    let links: Vec<DocumentLink> = serde_json::from_value(client.send_request(
        "textDocument/documentLink",
        json!({"textDocument":{"uri":uri}}),
    )?)?;
    let actual = links
        .iter()
        .map(|link| {
            (
                link.target.as_ref().map(|uri| uri.as_str()),
                link.tooltip.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    let local = project.uri("local.adoc")?;
    let image = project.uri("bird.png")?;
    let included = project.uri("chapter.adoc")?;
    assert_eq!(
        actual,
        [
            (Some(local.as_str()), Some("Local")),
            (Some("https://example.org/url"), None),
            (Some("https://example.org/auto"), None),
            (Some("mailto:team@example.org"), None),
            (Some(image.as_str()), Some("Open image")),
            (
                Some(included.as_str()),
                Some("Open included file: chapter.adoc")
            ),
        ]
    );
    for markup in ["mailto:team@example.org[Team]", "image::bird.png[]"] {
        let expected = range(source, markup)?;
        let position = Position::new(expected.start.line, expected.start.character + 3);
        assert!(
            client
                .send_request(
                    "textDocument/hover",
                    json!({"textDocument":{"uri":uri},"position":position})
                )?
                .is_null()
        );
    }
    client.shutdown();
    Ok(())
}
