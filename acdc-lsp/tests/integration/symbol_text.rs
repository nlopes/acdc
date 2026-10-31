use std::error::Error;

use serde_json::json;
use tower_lsp_server::ls_types::DocumentSymbol;

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
