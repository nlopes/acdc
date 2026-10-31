use std::error::Error;

use serde_json::json;
use tower_lsp_server::ls_types::{DocumentSymbol, SymbolInformation};

use crate::common::{LspTestClient, Project};

#[test]
fn block_symbol_titles_preserve_display_content_and_empty_fallbacks() -> Result<(), Box<dyn Error>>
{
    let project = Project::new("symbol-text")?;
    let uri = project.uri("document.adoc")?;
    let source = r#".😀 *Bold* image:bird.png[alt='Bird',title='Hidden tooltip'] <<target>> --
****
Body.
****

[title=""]
****
Empty title.
****

[title=""]
image::fallback.png[]
"#;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    let symbols: Vec<DocumentSymbol> = serde_json::from_value(client.send_request(
        "textDocument/documentSymbol",
        json!({"textDocument":{"uri":uri}}),
    )?)?;
    let names = symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["😀 Bold [Bird] target --", "Sidebar", "fallback.png"]
    );
    client.shutdown();
    Ok(())
}

#[test]
fn outline_and_search_keep_their_structural_coverage() -> Result<(), Box<dyn Error>> {
    let project = Project::new("symbol-coverage")?;
    let uri = project.uri("document.adoc")?;
    let source = r"= Document

[[kept-alias-one]]
[[kept-alias-two]]
[#kept-section-id]
== Kept section

.Kept example
====
.Kept paragraph
Paragraph.
====

.Kept list
* Item
+
.Kept attachment
Attached.

.Kept description
Term:: Definition.
+
.Kept description attachment
Attached.

[cols=a]
|===
|
.Hidden cell title
Cell.
|===

<1> Explanation
+
.Hidden callout attachment
Attached.

// Hidden comment
";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    let symbols: Vec<DocumentSymbol> = serde_json::from_value(client.send_request(
        "textDocument/documentSymbol",
        json!({"textDocument":{"uri":uri}}),
    )?)?;
    let section = symbols
        .iter()
        .find(|symbol| symbol.name == "Kept section")
        .ok_or("section")?;
    let children = section.children.as_deref().ok_or("section children")?;
    for (container, child) in [
        ("Kept example", "Kept paragraph"),
        ("Kept list", "Kept attachment"),
        ("Kept description", "Kept description attachment"),
    ] {
        let parent = children
            .iter()
            .find(|symbol| symbol.name == container)
            .ok_or("container")?;
        assert!(
            parent
                .children
                .as_deref()
                .is_some_and(|children| children.iter().any(|symbol| symbol.name == child)),
            "{container}"
        );
    }
    for name in ["Table", "Callout list"] {
        assert!(
            children
                .iter()
                .any(|symbol| symbol.name == name && symbol.children.is_none()),
            "{name}"
        );
    }
    let symbols: Vec<SymbolInformation> =
        serde_json::from_value(client.send_request("workspace/symbol", json!({"query":"kept"}))?)?;
    let mut names = symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "Kept attachment",
            "Kept description attachment",
            "Kept example",
            "Kept paragraph",
            "Kept section",
            "kept-alias-one",
            "kept-alias-two",
            "kept-section-id",
            "kept-section-id"
        ]
    );
    let hidden = client.send_request("workspace/symbol", json!({"query":"Hidden"}))?;
    assert!(hidden.is_null() || hidden.as_array().is_some_and(Vec::is_empty));
    assert!(
        symbols
            .iter()
            .all(|symbol| symbol.location.uri.as_str() == uri)
    );
    client.shutdown();
    Ok(())
}
