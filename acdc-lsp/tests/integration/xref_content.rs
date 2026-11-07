use std::{error::Error, fs};

use serde_json::{Value, json};
use tower_lsp_server::ls_types::{Location, Position, Range, WorkspaceEdit};

use crate::common::{LspTestClient, Project};

use super::source_edits::apply;

type TestResult = Result<(), Box<dyn Error>>;

fn ranges(text: &str, needle: &str) -> Result<Vec<Range>, Box<dyn Error>> {
    text.lines()
        .enumerate()
        .flat_map(|(line, content)| {
            content.match_indices(needle).map(move |(byte, _)| {
                let line = u32::try_from(line)?;
                let start = u32::try_from(content[..byte].encode_utf16().count())?;
                let end = start + u32::try_from(needle.encode_utf16().count())?;
                Ok(Range::new(
                    Position::new(line, start),
                    Position::new(line, end),
                ))
            })
        })
        .collect()
}

fn request(
    client: &mut LspTestClient,
    method: &str,
    uri: &str,
    position: Position,
) -> Result<Value, Box<dyn Error>> {
    Ok(client.send_request(method, json!({"textDocument":{"uri":uri},"position":position,"context":{"includeDeclaration":false},"newName":"renamed"}))?)
}

#[test]
fn xrefs_in_owned_content_agree_on_definition_references_and_rename() -> TestResult {
    let project = Project::new("xref-content")?;
    let uri = project.uri("document.adoc")?;
    let source = concat!(
        "= Document <<target>>\n:toc:\n:experimental:\n\n",
        "[[target]]\n== Target\n\n",
        "== Section <<target>>\n\n",
        ".Block <<target>>\nParagraph.\n\n",
        "[title='Named <<target>>']\nParagraph.\n\n",
        "[caption='Caption <<target>>',alt='Block <<target>>']\n.Captioned\nimage::bird.png[]\n\n",
        "image:bird.png[alt='*Alt <<target>>*',title='Tip <<target>>']\n\n",
        "\"`Curved <<target>>`\" and '`Apostrophe <<target>>`'.\n\n",
        "footnote:n[Note <<target>>.] footnote:n[]\n\n",
        "[quote, 'Credit <<target>>', 'Citation <<target>>']\n____\nBody.\n____\n\n",
        "Term <<target>>:: Description.\n\n",
        "<1> Callout <<target>>\n\n",
        "btn:[Save <<target>>] kbd:[Key <<target>>] menu:File[Item <<target>>]\n\n",
        "[cols=a]\n|===\n|😀 Cell <<target>>\n|===\n",
    );
    let expected = ranges(source, "<<target>>")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    for range in &expected {
        let cursor = Position::new(range.start.line, range.start.character + 3);
        let definition = request(&mut client, "textDocument/definition", &uri, cursor)?;
        assert_eq!(definition.get("uri"), Some(&json!(uri)), "{range:?}");
        assert_eq!(
            definition.pointer("/range/start/line"),
            Some(&json!(4)),
            "{range:?}"
        );
        let hover = request(&mut client, "textDocument/hover", &uri, cursor)?;
        assert_eq!(hover.get("range"), Some(&serde_json::to_value(range)?));
        let references: Vec<Location> = serde_json::from_value(request(
            &mut client,
            "textDocument/references",
            &uri,
            cursor,
        )?)?;
        let mut actual = references.iter().map(|loc| loc.range).collect::<Vec<_>>();
        actual.sort_by_key(|range| (range.start, range.end));
        assert_eq!(actual, expected, "{range:?}");
        assert!(references.iter().all(|loc| loc.uri.as_str() == uri));
        let prepare = request(&mut client, "textDocument/prepareRename", &uri, cursor)?;
        assert_eq!(prepare.get("placeholder"), Some(&json!("target")));
    }
    let cursor = expected.first().ok_or("xref")?.start;
    let edit: WorkspaceEdit =
        serde_json::from_value(request(&mut client, "textDocument/rename", &uri, cursor)?)?;
    let changes = edit.changes.ok_or("rename changes")?;
    let edits = changes.get(&uri.parse()?).ok_or("document edits")?;
    assert_eq!(edits.len(), expected.len() + 1);
    assert_eq!(apply(source, edits)?, source.replace("target", "renamed"));
    client.shutdown();
    Ok(())
}

#[test]
fn nested_xrefs_keep_unsaved_include_origins_without_duplicate_edits() -> TestResult {
    let project = Project::new("xref-content-includes")?;
    let book = project.uri("book.adoc")?;
    let other = project.uri("other.adoc")?;
    let child = project.uri("child.adoc")?;
    fs::write(project.path().join("child.adoc"), "Disk text.\n")?;
    let included = "// omitted\n// tag::keep[]\n    😀 image:bird.png[alt='*Alt <<target>>*',title='Tip <<target>>']\n// end::keep[]\n";
    let source = "[[target]]\n== Target\n\ninclude::child.adoc[tag=keep,indent=0]\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&child, included)?;
    client.open_document(&book, source)?;
    client.open_document(&other, "include::child.adoc[tag=keep,indent=0]\n")?;
    let expected = ranges(included, "<<target>>")?;
    let references: Vec<Location> = serde_json::from_value(request(
        &mut client,
        "textDocument/references",
        &book,
        Position::new(0, 3),
    )?)?;
    let mut actual = references.iter().map(|loc| loc.range).collect::<Vec<_>>();
    actual.sort_by_key(|range| (range.start, range.end));
    assert_eq!(actual, expected);
    assert!(references.iter().all(|loc| loc.uri.as_str() == child));
    let edit: WorkspaceEdit = serde_json::from_value(request(
        &mut client,
        "textDocument/rename",
        &book,
        Position::new(0, 3),
    )?)?;
    let changes = edit.changes.ok_or("rename changes")?;
    assert_eq!(changes.len(), 2);
    assert_eq!(
        apply(included, changes.get(&child.parse()?).ok_or("child edits")?)?,
        included.replace("target", "renamed")
    );
    assert_eq!(
        apply(source, changes.get(&book.parse()?).ok_or("book edits")?)?,
        source.replace("target", "renamed")
    );
    let collision = "// omitted\n// tag::keep[]\n    😀 image:bird.png[alt='*Alt plain text*',title='plain text']\n\ninclude::child.adoc[tag=keep,indent=0]\n";
    let collision_uri = project.uri("collision.adoc")?;
    client.open_document(&collision_uri, collision)?;
    for range in expected {
        assert!(
            request(
                &mut client,
                "textDocument/definition",
                &collision_uri,
                range.start
            )?
            .is_null()
        );
    }
    client.shutdown();
    Ok(())
}

#[test]
fn nested_xref_labels_select_the_innermost_target() -> TestResult {
    let project = Project::new("nested-xref-label")?;
    let uri = project.uri("document.adoc")?;
    let source = "[[outer]]\n== Outer\n\n[[inner]]\n== Inner\n\nxref:outer[See <<inner>>]\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    let cursor = ranges(source, "<<inner>>")?
        .first()
        .ok_or("inner xref")?
        .start;
    let definition = request(&mut client, "textDocument/definition", &uri, cursor)?;
    assert_eq!(definition.pointer("/range/start/line"), Some(&json!(3)));
    let prepare = request(&mut client, "textDocument/prepareRename", &uri, cursor)?;
    assert_eq!(prepare.get("placeholder"), Some(&json!("inner")));
    let edit: WorkspaceEdit =
        serde_json::from_value(request(&mut client, "textDocument/rename", &uri, cursor)?)?;
    let changes = edit.changes.ok_or("rename changes")?;
    assert_eq!(
        apply(source, changes.get(&uri.parse()?).ok_or("edits")?)?,
        source.replace("inner", "renamed")
    );
    client.shutdown();
    Ok(())
}

#[test]
fn rename_declines_when_a_retained_xref_has_no_literal_target() -> TestResult {
    let project = Project::new("expanded-xref-rename")?;
    let uri = project.uri("document.adoc")?;
    let source = ":destination: target\n\n[[target]]\n== Target\n\nimage:x[alt='<<{destination}>>']\n\n<<target>>\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    let cursor = ranges(source, "<<target>>")?
        .first()
        .ok_or("literal xref")?
        .start;
    let refs: Vec<Location> = serde_json::from_value(request(
        &mut client,
        "textDocument/references",
        &uri,
        cursor,
    )?)?;
    assert_eq!(refs.len(), 2);
    assert!(request(&mut client, "textDocument/rename", &uri, cursor)?.is_null());
    client.shutdown();
    Ok(())
}
