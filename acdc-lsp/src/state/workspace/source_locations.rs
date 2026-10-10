use std::{collections::HashMap, error::Error, path::PathBuf};

use acdc_parser::{
    IncludeLoader, IncludeSource, IncludeSourceError, IncludeSourceErrorKind, IncludeSourceTarget,
    Options, SafeMode,
};
use tower_lsp_server::ls_types::{FileRename, Position, TextEdit, Uri};

use super::Workspace;
use crate::{
    capabilities::{definition, file_rename, references, rename},
    convert::position_to_offset,
};

const CHAPTER: &str = "// omitted\n// omitted\n[[chapter]]\n== Chapter\n\nSee <<chapter>>.\n";
const BOOK: &str = "See <<chapter>>.\n\ninclude::chapter.adoc[lines=3..6]\n";

pub(super) fn file_uri(name: &str) -> Result<Uri, Box<dyn Error>> {
    Uri::from_file_path(
        std::env::temp_dir()
            .join("acdc-lsp-source-locations")
            .join(name),
    )
    .ok_or_else(|| "invalid file URI".into())
}

fn options() -> Result<Options<'static>, Box<dyn Error>> {
    options_with_sources(&[("chapter.adoc", CHAPTER)])
}

pub(super) fn options_with_sources(
    sources: &[(&str, &str)],
) -> Result<Options<'static>, Box<dyn Error>> {
    let base = file_uri("book.adoc")?
        .to_file_path()
        .and_then(|path| path.parent().map(PathBuf::from))
        .ok_or("missing base directory")?;
    let files: HashMap<_, _> = sources
        .iter()
        .map(|(name, text)| (base.join(name), (*text).to_string()))
        .collect();
    Ok(Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_base_dir(base)
        .with_include_loader(IncludeLoader::custom(
            move |target: &IncludeSourceTarget| {
                if let IncludeSourceTarget::File(path) = target
                    && let Some(text) = files.get(path)
                {
                    return Ok(IncludeSource::from_string(text.clone()));
                }
                Err(IncludeSourceError::new(
                    IncludeSourceErrorKind::NotFound,
                    "unknown test file",
                ))
            },
        ))
        .build()?)
}

#[test]
fn definition_uses_included_source_location() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let book = file_uri("book.adoc")?;
    workspace.update_document_with_options(book.clone(), BOOK.into(), 1, &options()?);
    let doc = workspace.get_document(&book).ok_or("missing book")?;
    let (uri, location) =
        definition::find_definition_at_position(&doc, &book, &workspace, Position::new(0, 8))
            .ok_or("missing definition")?;
    assert_eq!(uri, file_uri("chapter.adoc")?);
    assert_eq!(location.start.line, 3);
    Ok(())
}

#[test]
fn rename_and_references_use_included_source_locations_once() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let book = file_uri("book.adoc")?;
    let chapter = file_uri("chapter.adoc")?;
    workspace.update_document_with_options(book.clone(), BOOK.into(), 1, &options()?);
    workspace.update_document(chapter.clone(), CHAPTER.into(), 1);
    let doc = workspace.get_document(&book).ok_or("missing book")?;
    let edits = rename::compute_rename(&doc, &book, &workspace, Position::new(0, 8), "renamed")
        .and_then(|edit| edit.changes)
        .ok_or("missing rename")?;
    assert_eq!(edits.get(&book).ok_or("missing book edit")?.len(), 1);
    let chapter_edits = edits.get(&chapter).ok_or("missing chapter edits")?;
    assert_eq!(chapter_edits.len(), 2);
    assert!(chapter_edits.iter().any(|edit| edit.range.start.line == 2));
    assert!(chapter_edits.iter().any(|edit| edit.range.start.line == 5));
    assert_eq!(
        apply_edits(CHAPTER, chapter_edits)?,
        CHAPTER.replace("chapter", "renamed")
    );
    assert_eq!(
        apply_edits(BOOK, edits.get(&book).ok_or("missing book edit")?)?,
        BOOK.replacen("<<chapter>>", "<<renamed>>", 1)
    );
    let refs = references::find_references(&doc, &book, &workspace, Position::new(0, 8), true)
        .ok_or("missing references")?;
    assert_eq!(refs.len(), 3);
    assert_eq!(
        refs.iter()
            .filter(|reference| reference.uri == chapter)
            .count(),
        2
    );
    Ok(())
}

#[test]
fn rename_uses_original_columns_after_tag_selection_and_indentation() -> Result<(), Box<dyn Error>>
{
    let workspace = Workspace::new();
    let book = file_uri("book.adoc")?;
    let chapter = file_uri("chapter.adoc")?;
    let text =
        "// tag::body[]\n    [[chapter]]\n    == Chapter\n\n    See <<chapter>>.\n// end::body[]\n";
    let options = options_with_sources(&[("chapter.adoc", text)])?;
    workspace.update_document_with_options(
        book.clone(),
        "See <<chapter>>.\n\ninclude::chapter.adoc[tag=body,indent=0]\n".into(),
        1,
        &options,
    );
    workspace.update_document(chapter.clone(), text.into(), 1);
    let doc = workspace.get_document(&book).ok_or("missing book")?;
    let edits = rename::compute_rename(&doc, &book, &workspace, Position::new(0, 8), "renamed")
        .and_then(|edit| edit.changes)
        .ok_or("missing edits")?;
    let edits = edits.get(&chapter).ok_or("missing chapter edits")?;
    assert_eq!(edits.len(), 2);
    assert_eq!(
        apply_edits(text, edits)?,
        text.replace("chapter", "renamed")
    );
    Ok(())
}

fn apply_edits(text: &str, edits: &[TextEdit]) -> Result<String, Box<dyn Error>> {
    let mut result = text.to_string();
    for edit in edits.iter().rev() {
        let start = position_to_offset(text, edit.range.start).ok_or("invalid start")?;
        let end = position_to_offset(text, edit.range.end).ok_or("invalid end")?;
        result.replace_range(start..end, &edit.new_text);
    }
    Ok(result)
}

#[test]
fn included_source_locations_do_not_match_parent_cursor_offsets() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let book = file_uri("book.adoc")?;
    let options = options_with_sources(&[("chapter.adoc", "[[chapter]]\n\nSee <<chapter>>.\n")])?;
    workspace.update_document_with_options(
        book.clone(),
        "include::chapter.adoc[]\n\nPlain text.\n".into(),
        1,
        &options,
    );
    let doc = workspace.get_document(&book).ok_or("missing book")?;
    assert!(rename::prepare_rename(&doc, Position::new(0, 4)).is_none());
    assert!(
        definition::find_definition_at_position(&doc, &book, &workspace, Position::new(0, 20))
            .is_none()
    );
    Ok(())
}

#[test]
fn file_rename_uses_included_source_directory() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let book = file_uri("book.adoc")?;
    let chapter = file_uri("parts/chapter.adoc")?;
    let text = "xref:next.adoc#target[Next]\n";
    let options = options_with_sources(&[("parts/chapter.adoc", text)])?;
    workspace.update_document_with_options(
        book.clone(),
        "include::parts/chapter.adoc[]\n".into(),
        1,
        &options,
    );
    workspace.update_document(chapter.clone(), text.into(), 1);
    let renames = [FileRename {
        old_uri: file_uri("parts/next.adoc")?.as_str().into(),
        new_uri: file_uri("parts/renamed.adoc")?.as_str().into(),
    }];
    let edits = file_rename::compute_file_rename_edits(&workspace, &renames)
        .and_then(|edit| edit.changes)
        .ok_or("missing edits")?;
    assert!(!edits.contains_key(&book));
    let edits = edits.get(&chapter).ok_or("missing chapter edit")?;
    assert_eq!(edits.len(), 1);
    assert_eq!(
        apply_edits(text, edits)?,
        "xref:renamed.adoc#target[Next]\n"
    );
    Ok(())
}

#[test]
fn diagnostics_follow_source_locations_and_clear_after_last_parent() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let first = file_uri("first.adoc")?;
    let second = file_uri("second.adoc")?;
    let chapter = file_uri("chapter.adoc")?;
    let options = options_with_sources(&[(
        "chapter.adoc",
        "// omitted\n// omitted\n=== Skipped\n\nSee <<missing>>.\n\nimage::missing.png[]\n",
    )])?;
    let text = "= Book\n\ninclude::chapter.adoc[lines=3..7]\n";
    for uri in [&first, &second] {
        let affected =
            workspace.update_document_with_options(uri.clone(), text.into(), 1, &options);
        assert!(affected.contains(&chapter));
        let (diagnostics, _) = workspace.diagnostics_for(uri);
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("Unresolved cross-reference"))
        );
    }
    let (diagnostics, version) = workspace.diagnostics_for(&chapter);
    assert_eq!(version, None);
    let missing: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.contains("Unresolved cross-reference"))
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(
        missing.first().ok_or("missing warning")?.range.start.line,
        4
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Section level")
                && diagnostic.range.start.line == 2)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("missing.png")
                && diagnostic.range.start.line == 6)
    );
    let affected = workspace.update_document(first, "= Book\n".into(), 2);
    assert!(affected.contains(&chapter));
    assert_ne!(workspace.diagnostics_for(&chapter).0, []);
    let affected = workspace.remove_document(&second);
    assert!(affected.contains(&chapter));
    assert_eq!(workspace.diagnostics_for(&chapter).0, []);
    Ok(())
}

#[test]
fn source_location_index_tracks_each_parent() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let first = file_uri("first.adoc")?;
    let second = file_uri("second.adoc")?;
    for uri in [&first, &second] {
        workspace.update_document_with_options(uri.clone(), BOOK.into(), 1, &options()?);
    }
    let anchors = workspace.find_anchor_globally("chapter");
    assert_eq!(anchors.len(), 1);
    assert_eq!(
        anchors.first().ok_or("missing anchor")?.0,
        file_uri("chapter.adoc")?
    );
    workspace.update_document(first, "No includes.\n".into(), 2);
    assert_eq!(workspace.find_anchor_globally("chapter").len(), 1);
    workspace.remove_document(&second);
    assert_eq!(workspace.find_anchor_globally("chapter"), []);
    Ok(())
}

#[test]
fn rename_preserves_anchor_syntax_labels_and_unicode() -> Result<(), Box<dyn Error>> {
    for anchor in ["[[old.name]]", "[[old.name,Label]]", "[#old]"] {
        let id = if anchor == "[#old]" {
            "old"
        } else {
            "old.name"
        };
        let content =
            format!("{anchor}\r\n== Section\r\n\r\n😀 <<{id},label>> and xref:#{id}[Label].\r\n");
        let workspace = Workspace::new();
        let uri = file_uri("syntax.adoc")?;
        workspace.update_document(uri.clone(), content.clone(), 1);
        let doc = workspace.get_document(&uri).ok_or("missing document")?;
        let edits = rename::compute_rename(&doc, &uri, &workspace, Position::new(3, 6), "new")
            .and_then(|edit| edit.changes)
            .ok_or("missing edits")?;
        let edits = edits.get(&uri).ok_or("missing file edits")?;
        assert_eq!(edits.len(), 3);
        assert_eq!(apply_edits(&content, edits)?, content.replace(id, "new"));
    }
    Ok(())
}

#[test]
fn rename_preserves_cross_file_target_prefix() -> Result<(), Box<dyn Error>> {
    let workspace = Workspace::new();
    let book = file_uri("book.adoc")?;
    let chapter = file_uri("chapter.adoc")?;
    let text = "xref:chapter.adoc#chapter[Chapter]\n\ninclude::chapter.adoc[]\n";
    workspace.update_document_with_options(book.clone(), text.into(), 1, &options()?);
    workspace.update_document(chapter, CHAPTER.into(), 1);
    let doc = workspace.get_document(&book).ok_or("missing book")?;
    let position = Position::new(0, 20);
    let prepared = rename::prepare_rename(&doc, position).ok_or("missing prepare rename")?;
    assert!(
        matches!(prepared, tower_lsp_server::ls_types::PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } if placeholder == "chapter")
    );
    let edits = rename::compute_rename(&doc, &book, &workspace, position, "renamed")
        .and_then(|edit| edit.changes)
        .ok_or("missing edits")?;
    assert_eq!(
        apply_edits(text, edits.get(&book).ok_or("missing book edit")?)?,
        "xref:chapter.adoc#renamed[Chapter]\n\ninclude::chapter.adoc[]\n"
    );
    Ok(())
}

#[test]
fn rename_does_not_change_references_without_an_editable_definition() -> Result<(), Box<dyn Error>>
{
    for text in [
        "== Generated\n\nSee <<_generated>>.\n",
        ":id: old\n\n[[{id}]]\n== Section\n\nSee <<old>>.\n",
    ] {
        let workspace = Workspace::new();
        let uri = file_uri("generated.adoc")?;
        workspace.update_document(uri.clone(), text.into(), 1);
        let doc = workspace.get_document(&uri).ok_or("missing document")?;
        let line = u32::try_from(text.lines().count() - 1)?;
        assert!(
            rename::compute_rename(&doc, &uri, &workspace, Position::new(line, 8), "new").is_none()
        );
    }
    Ok(())
}
