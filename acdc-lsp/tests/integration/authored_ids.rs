use std::{error::Error, fs};

use serde_json::{Value, json};
use tower_lsp_server::ls_types::{Location, Position, Range, WorkspaceEdit};

use super::source_edits::apply;
use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

fn range(text: &str, needle: &str) -> Result<Range, Box<dyn Error>> {
    for (line, content) in text.lines().enumerate() {
        if let Some(byte) = content.find(needle) {
            let line = u32::try_from(line)?;
            let start = u32::try_from(content.get(..byte).ok_or("prefix")?.encode_utf16().count())?;
            return Ok(Range::new(
                Position::new(line, start),
                Position::new(line, start + u32::try_from(needle.encode_utf16().count())?),
            ));
        }
    }
    Err(format!("missing {needle}").into())
}

fn request(
    client: &mut LspTestClient,
    method: &str,
    uri: &str,
    position: Position,
) -> Result<Value, Box<dyn Error>> {
    Ok(client.send_request(method, json!({"textDocument":{"uri":uri},"position":position,"context":{"includeDeclaration":true},"newName":"renamed"}))?)
}

#[test]
fn authored_ids_navigate_and_rename_across_block_and_inline_owners() -> TestResult {
    let project = Project::new("authored-id-owners")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    let cases = [
        "[id=target]\n= Header",
        "[id=target]\nParagraph.",
        "[id=\"target\"]\n== Section",
        "[id='target']\n[discrete]\n== Discrete",
        "[#target]\nimage::bird.png[]",
        "[id=target]\naudio::song.ogg[]",
        "[id=target]\nvideo::clip.mp4[]",
        "[id=target]\n* Item",
        "[id=target]\n. Item",
        "[id=target]\nTerm:: Description",
        "[id=target]\nNOTE: Note",
        "[id=target]\n----\nCode\n----",
        "[id=target]\n|===\n|Cell\n|===",
        "[id=target]\n<<<\n\nAfter.",
        "é😀 image:bird.png[Alt,id=target]",
        "é😀 icon:heart[id=target]",
        "é😀 link:guide.adoc[Guide,id=target]",
        "é😀 https://example.org[Guide,id=target]",
        "é😀 mailto:a@example.org[Mail,id=target]",
        "é😀 xref:missing[Label,id=target]",
        "é😀 indexterm2:[Term,id=target]",
        "é😀 [#target]*Bold*",
        "é😀 [#target]**Bold**",
        "é😀 [#target]_Italic_",
        "é😀 [#target]__Italic__",
        "é😀 [#target]`Mono`",
        "é😀 [#target]``Mono``",
        "é😀 [#target]#Marked#",
        "é😀 [#target]##Marked##",
        "é😀 [#target]~Sub~",
        "é😀 [#target]^Super^",
        "é😀 [#target]\"`Quoted`\"",
        "é😀 [#target]'`Apostrophe`'",
        "é😀 anchor:target[Label]",
        ".Title [#target]*Bold*\nParagraph.",
        "footnote:[[#target]*Bold*]",
        "image:bird.png[alt='[#target]*Bold*']",
        "[cols=a]\n|===\n|é😀 image:bird.png[Alt,id=target]\n|===",
        "[[target]] Term:: Description",
    ];
    for (index, markup) in cases.iter().enumerate() {
        let id = format!("target{index}");
        let source =
            format!("{}\n\nSee <<{id}>>.\n", markup.replace("target", &id)).replace('\n', "\r\n");
        let uri = project.uri(&format!("case{index}.adoc"))?;
        client.open_document(&uri, &source)?;
        let declared = range(&source, &id)?;
        let usage = range(&source, &format!("<<{id}>>"))?;
        let definition: Location = serde_json::from_value(request(
            &mut client,
            "textDocument/definition",
            &uri,
            usage.start,
        )?)?;
        assert_eq!(definition.uri.as_str(), uri, "{markup}");
        assert_eq!(definition.range.start.line, declared.start.line, "{markup}");
        assert_eq!(definition.range.end.line, declared.end.line, "{markup}");
        assert!(
            definition.range.start <= declared.start && definition.range.end >= declared.end,
            "{markup}"
        );
        for cursor in [declared.start, usage.start] {
            let prepare = request(&mut client, "textDocument/prepareRename", &uri, cursor)?;
            assert_eq!(prepare.get("placeholder"), Some(&json!(id)), "{markup}");
            if cursor == declared.start {
                assert_eq!(
                    prepare.get("range"),
                    Some(&serde_json::to_value(declared)?),
                    "{markup}"
                );
            }
            let references: Vec<Location> = serde_json::from_value(request(
                &mut client,
                "textDocument/references",
                &uri,
                cursor,
            )?)?;
            assert_eq!(references.len(), 2, "{markup}");
            assert!(
                references
                    .iter()
                    .all(|reference| reference.uri.as_str() == uri)
            );
            let edit: WorkspaceEdit =
                serde_json::from_value(request(&mut client, "textDocument/rename", &uri, cursor)?)?;
            let changes = edit.changes.ok_or("rename changes")?;
            assert_eq!(changes.len(), 1);
            assert_eq!(
                apply(&source, changes.get(&uri.parse()?).ok_or("document edits")?)?,
                source.replace(&id, "renamed"),
                "{markup}"
            );
        }
    }
    client.shutdown();
    Ok(())
}

#[test]
fn generated_and_expanded_ids_are_navigable_but_not_editable() -> TestResult {
    let project = Project::new("uneditable-ids")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    let cases = [
        ("== Generated\n\n<<_generated>>\n", "_generated"),
        (
            ":name: block\n\n[id={name}]\nParagraph.\n\n<<block>>\n",
            "block",
        ),
        (":name: image\n\nimage:x[id={name}]\n\n<<image>>\n", "image"),
        (":name: span\n\n[#{name}]*Body*\n\n<<span>>\n", "span"),
    ];
    for (index, (source, id)) in cases.into_iter().enumerate() {
        let uri = project.uri(&format!("case{index}.adoc"))?;
        client.open_document(&uri, source)?;
        let usage = range(source, &format!("<<{id}>>"))?;
        assert!(
            !request(&mut client, "textDocument/definition", &uri, usage.start)?.is_null(),
            "{source}"
        );
        assert!(
            request(&mut client, "textDocument/rename", &uri, usage.start)?.is_null(),
            "{source}"
        );
    }
    client.shutdown();
    Ok(())
}

#[test]
fn aliases_and_duplicate_ids_keep_all_literal_declarations() -> TestResult {
    let project = Project::new("id-aliases")?;
    let uri = project.uri("document.adoc")?;
    let source = "[[alias]]\n[id=primary]\n== Heading\n\n[#duplicate]*One* and [#duplicate]*Two*.\n\n<<alias>> <<primary>> <<duplicate>>\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, source)?;
    for id in ["alias", "primary", "duplicate"] {
        let declared = range(source, id)?;
        let prepare = request(
            &mut client,
            "textDocument/prepareRename",
            &uri,
            declared.start,
        )?;
        assert_eq!(prepare.get("placeholder"), Some(&json!(id)));
        let references: Vec<Location> = serde_json::from_value(request(
            &mut client,
            "textDocument/references",
            &uri,
            declared.start,
        )?)?;
        assert_eq!(references.len(), if id == "duplicate" { 3 } else { 2 });
        let usage = range(source, &format!("<<{id}>>"))?;
        let edit: WorkspaceEdit = serde_json::from_value(request(
            &mut client,
            "textDocument/rename",
            &uri,
            usage.start,
        )?)?;
        let changes = edit.changes.ok_or("rename changes")?;
        assert_eq!(
            apply(source, changes.get(&uri.parse()?).ok_or("edits")?)?,
            source.replace(id, "renamed")
        );
    }
    client.shutdown();
    Ok(())
}

#[test]
fn selected_unsaved_include_ids_use_original_unicode_ranges() -> TestResult {
    let project = Project::new("included-ids")?;
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    fs::write(project.path().join("child.adoc"), "Disk.\n")?;
    let included =
        "// omitted\n// tag::keep[]\n    é😀 [#span]*Body* image:x[id=picture]\n// end::keep[]\n";
    let source = "<<span>> <<picture>>\n\ninclude::child.adoc[tag=keep,indent=0]\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&child, included)?;
    client.open_document(&book, source)?;
    client.open_document(
        &project.uri("second.adoc")?,
        "include::child.adoc[tag=keep,indent=0]\n",
    )?;
    for id in ["span", "picture"] {
        let usage = range(source, &format!("<<{id}>>"))?;
        let definition: Location = serde_json::from_value(request(
            &mut client,
            "textDocument/definition",
            &book,
            usage.start,
        )?)?;
        assert_eq!(definition.uri.as_str(), child);
        assert_eq!(definition.range, range(included, id)?);
        let edit: WorkspaceEdit = serde_json::from_value(request(
            &mut client,
            "textDocument/rename",
            &book,
            usage.start,
        )?)?;
        let changes = edit.changes.ok_or("rename changes")?;
        assert_eq!(changes.len(), 2);
        assert_eq!(
            apply(included, changes.get(&child.parse()?).ok_or("child edits")?)?,
            included.replace(id, "renamed")
        );
        assert_eq!(
            apply(source, changes.get(&book.parse()?).ok_or("book edits")?)?,
            source.replace(id, "renamed")
        );
    }
    client.shutdown();
    Ok(())
}

#[test]
fn named_ids_in_closed_files_resolve_without_selecting_unrelated_text() -> TestResult {
    let project = Project::new("closed-authored-ids")?;
    let target = project.uri("target.adoc")?;
    let reader = project.uri("reader.adoc")?;
    let declaration = "[id=target]\nParagraph text that is not an ID.\n";
    fs::write(project.path().join("target.adoc"), declaration)?;
    let source = "xref:target.adoc#target[]\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&reader, source)?;
    let definition: Location = serde_json::from_value(request(
        &mut client,
        "textDocument/definition",
        &reader,
        Position::new(0, 10),
    )?)?;
    assert_eq!(definition.uri.as_str(), target);
    assert_eq!(definition.range, range(declaration, "target")?);
    client.open_document(&target, declaration)?;
    assert!(
        request(
            &mut client,
            "textDocument/prepareRename",
            &target,
            Position::new(1, 5)
        )?
        .is_null()
    );
    assert!(
        request(
            &mut client,
            "textDocument/hover",
            &target,
            Position::new(1, 5)
        )?
        .is_null()
    );
    client.shutdown();
    Ok(())
}
