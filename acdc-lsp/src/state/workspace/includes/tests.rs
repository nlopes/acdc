use std::{
    error::Error,
    fs,
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use tower_lsp_server::ls_types::{DiagnosticSeverity, Uri};

use super::super::Workspace;
use crate::{convert::file_path_to_uri, limits::MAX_INDEXABLE_FILE_BYTES};

type TestResult = Result<(), Box<dyn Error>>;

struct Project(PathBuf);

impl Project {
    fn new() -> Result<Self, Box<dyn Error>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "acdc-lsp-includes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path.canonicalize()?))
    }

    fn uri(&self, name: &str) -> Result<Uri, Box<dyn Error>> {
        file_path_to_uri(&self.0.join(name)).ok_or_else(|| "invalid file URI".into())
    }

    fn write(&self, name: &str, text: &str) -> TestResult {
        fs::write(self.0.join(name), text)?;
        Ok(())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn has_anchor(workspace: &Workspace, uri: &Uri, anchor: &str) -> bool {
    workspace
        .get_document(uri)
        .is_some_and(|document| document.anchors.contains_key(anchor))
}

#[cfg(windows)]
#[test]
fn canonical_windows_uri_paths_use_native_separators() -> TestResult {
    let path = std::path::Path::new(r"\\?\C:\docs\unsaved.adoc");
    let uri = Uri::from_file_path(path).ok_or("invalid canonical file URI")?;
    assert_eq!(super::file_path(&uri).as_deref(), Some(path));
    let normal = std::path::Path::new(r"C:\docs\unsaved.adoc");
    let uri = file_path_to_uri(path).ok_or("invalid drive URI")?;
    assert_eq!(super::file_path(&uri).as_deref(), Some(normal));
    Ok(())
}

#[cfg(windows)]
#[test]
fn canonical_windows_uris_load_disk_and_buffer_includes() -> TestResult {
    let project = Project::new()?;
    project.write("child.adoc", "== Disk\n")?;
    let workspace = Workspace::new();
    // Preserve the upstream conversion that caused the Windows release failure.
    let book =
        Uri::from_file_path(project.0.join("book.adoc")).ok_or("invalid canonical file URI")?;
    workspace.update_document(book.clone(), "include::child.adoc[]\n".into(), 1);
    assert!(has_anchor(&workspace, &book, "_disk"));
    assert_eq!(workspace.diagnostics_for(&book).0, []);

    let child = project.uri("child.adoc")?;
    let affected = workspace.update_document(child.clone(), "== Buffer\n".into(), 1);
    assert!(affected.contains(&book));
    assert!(has_anchor(&workspace, &book, "_buffer"));
    workspace.remove_document(&child);
    assert!(has_anchor(&workspace, &book, "_disk"));
    assert_eq!(workspace.diagnostics_for(&book).0, []);
    Ok(())
}

#[test]
fn include_paths_reject_non_file_relative_and_remote_uris() -> TestResult {
    for uri in [
        "untitled:book",
        "file:relative.adoc",
        "file://remote/share/book.adoc",
    ] {
        assert!(super::file_path(&uri.parse()?).is_none(), "{uri}");
    }
    Ok(())
}

#[test]
fn unsaved_includes_override_disk_and_refresh_parent_versions() -> TestResult {
    let project = Project::new()?;
    project.write("child.adoc", "== Disk\n")?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    workspace.update_document(child.clone(), "== Unsaved\n".into(), 3);
    workspace.update_document(book.clone(), "include::child.adoc[]\n".into(), 7);
    assert!(has_anchor(&workspace, &book, "_unsaved"));
    assert!(!has_anchor(&workspace, &book, "_disk"));

    let affected = workspace.update_document(child.clone(), "== Changed\n".into(), 4);
    assert!(affected.contains(&book));
    assert!(has_anchor(&workspace, &book, "_changed"));
    assert!(!has_anchor(&workspace, &book, "_unsaved"));
    assert_eq!(workspace.diagnostics_for(&book).1, Some(7));
    assert_eq!(workspace.diagnostics_for(&child).1, Some(4));
    Ok(())
}

#[test]
fn opening_and_closing_a_missing_include_refreshes_parents() -> TestResult {
    let project = Project::new()?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    workspace.update_document(book.clone(), "include::child.adoc[]\n".into(), 1);
    assert!(
        workspace
            .diagnostics_for(&book)
            .0
            .iter()
            .any(|d| d.message.contains("not found"))
    );
    workspace.update_document(child.clone(), "== New\n".into(), 1);
    assert!(has_anchor(&workspace, &book, "_new"));
    assert_eq!(workspace.diagnostics_for(&book).0, []);
    let affected = workspace.remove_document(&child);
    assert!(affected.contains(&book));
    assert!(!has_anchor(&workspace, &book, "_new"));
    assert_eq!(workspace.find_anchor_globally("_new"), []);
    assert!(
        workspace
            .diagnostics_for(&book)
            .0
            .iter()
            .any(|d| d.message.contains("not found"))
    );
    Ok(())
}

#[test]
fn closing_a_buffer_restores_disk_content_and_clears_old_diagnostics() -> TestResult {
    let project = Project::new()?;
    project.write("child.adoc", "== Disk\n")?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    workspace.update_document(child.clone(), "== Buffer\n\nSee <<missing>>.\n".into(), 1);
    workspace.update_document(book.clone(), "include::child.adoc[]\n".into(), 1);
    assert_ne!(workspace.diagnostics_for(&child).0, []);
    let affected = workspace.remove_document(&child);
    assert!(affected.contains(&child));
    assert!(has_anchor(&workspace, &book, "_disk"));
    assert!(!has_anchor(&workspace, &book, "_buffer"));
    assert_eq!(workspace.diagnostics_for(&child).0, []);
    Ok(())
}

#[test]
fn nested_includes_refresh_all_parents_and_drop_removed_dependencies() -> TestResult {
    let project = Project::new()?;
    project.write("middle.adoc", "include::leaf.adoc[]\n")?;
    let workspace = Workspace::new();
    let first = project.uri("first.adoc")?;
    let second = project.uri("second.adoc")?;
    let leaf = project.uri("leaf.adoc")?;
    workspace.update_document(first.clone(), "include::middle.adoc[]\n".into(), 1);
    workspace.update_document(second.clone(), "include::middle.adoc[]\n".into(), 1);
    let affected = workspace.update_document(leaf.clone(), "== Leaf\n".into(), 1);
    assert!(affected.contains(&first) && affected.contains(&second));
    assert!(has_anchor(&workspace, &first, "_leaf"));
    assert!(has_anchor(&workspace, &second, "_leaf"));
    workspace.update_document(first.clone(), "== Independent\n".into(), 2);
    let affected = workspace.update_document(leaf, "== Changed\n".into(), 2);
    assert!(!affected.contains(&first));
    assert!(affected.contains(&second));
    assert!(has_anchor(&workspace, &second, "_changed"));
    assert!(!has_anchor(&workspace, &second, "_leaf"));
    Ok(())
}

#[test]
fn disk_creation_changes_and_deletion_refresh_includes() -> TestResult {
    let project = Project::new()?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    workspace.update_document(
        book.clone(),
        "include::child.adoc[opts=optional]\n".into(),
        1,
    );
    assert_eq!(workspace.diagnostics_for(&book).0, []);
    project.write("child.adoc", "== Created\n")?;
    assert!(
        workspace
            .files_changed(std::slice::from_ref(&child))
            .contains(&book)
    );
    assert!(has_anchor(&workspace, &book, "_created"));
    project.write("child.adoc", "== Changed\n")?;
    workspace.files_changed(&[book.clone(), child.clone()]);
    assert!(has_anchor(&workspace, &book, "_changed"));
    assert!(!has_anchor(&workspace, &book, "_created"));
    fs::remove_file(project.0.join("child.adoc"))?;
    workspace.files_changed(&[child]);
    assert!(!has_anchor(&workspace, &book, "_changed"));
    assert!(workspace.query_workspace_symbols("Changed").is_empty());
    Ok(())
}

#[test]
fn watcher_events_do_not_replace_open_buffers() -> TestResult {
    let project = Project::new()?;
    project.write("child.adoc", "== Disk\n")?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    workspace.update_document(child.clone(), "== Buffer\n".into(), 2);
    workspace.update_document(book.clone(), "include::child.adoc[]\n".into(), 1);
    project.write("child.adoc", "== Updated disk\n")?;
    workspace.files_changed(&[child]);
    assert!(has_anchor(&workspace, &book, "_buffer"));
    assert!(!has_anchor(&workspace, &book, "_updated_disk"));
    Ok(())
}

#[test]
fn line_selection_accepts_large_files_and_size_failures_can_recover() -> TestResult {
    let project = Project::new()?;
    let mut file = fs::File::create(project.0.join("large.adoc"))?;
    file.write_all(b"== Selected\n")?;
    file.set_len(MAX_INDEXABLE_FILE_BYTES + 1024)?;
    drop(file);
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    workspace.update_document(book.clone(), "include::large.adoc[lines=1]\n".into(), 1);
    assert!(has_anchor(&workspace, &book, "_selected"));
    workspace.update_document(book.clone(), "include::large.adoc[]\n".into(), 2);
    assert!(
        workspace
            .get_document(&book)
            .ok_or("missing book")?
            .ast()
            .is_none()
    );
    assert!(
        workspace
            .diagnostics_for(&book)
            .0
            .iter()
            .any(|d| d.message.contains("limit"))
    );
    project.write("large.adoc", "== Recovered\n")?;
    workspace.files_changed(&[project.uri("large.adoc")?]);
    assert!(has_anchor(&workspace, &book, "_recovered"));
    Ok(())
}

#[test]
fn line_selection_uses_an_oversized_open_buffer() -> TestResult {
    let project = Project::new()?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    let child = project.uri("large.adoc")?;
    let text = format!(
        "== Buffer\n{}",
        "x".repeat(usize::try_from(MAX_INDEXABLE_FILE_BYTES)?)
    );
    workspace.update_document(child.clone(), text, 1);
    assert!(
        workspace
            .get_document(&child)
            .ok_or("missing child")?
            .ast()
            .is_none()
    );
    workspace.update_document(book.clone(), "include::large.adoc[lines=1]\n".into(), 1);
    assert!(has_anchor(&workspace, &book, "_buffer"));
    Ok(())
}

#[test]
fn renaming_an_entry_document_rebases_its_includes() -> TestResult {
    let project = Project::new()?;
    fs::create_dir(project.0.join("moved"))?;
    project.write("child.adoc", "== Before\n")?;
    project.write("moved/child.adoc", "== After\n")?;
    let workspace = Workspace::new();
    let old = project.uri("book.adoc")?;
    let new = project.uri("moved/book.adoc")?;
    workspace.update_document(old.clone(), "include::child.adoc[]\n".into(), 3);
    assert!(has_anchor(&workspace, &old, "_before"));
    let affected = workspace.rename_document_uri(&old, &new);
    assert!(affected.contains(&old) && affected.contains(&new));
    assert!(!workspace.has_document(&old));
    assert!(!workspace.include_dependencies.contains_key(&old));
    assert!(has_anchor(&workspace, &new, "_after"));
    assert!(!has_anchor(&workspace, &new, "_before"));
    assert_eq!(workspace.find_anchor_globally("_before"), []);
    Ok(())
}

#[test]
fn attributes_optional_targets_and_inactive_includes_use_parser_policy() -> TestResult {
    let project = Project::new()?;
    project.write("child.adoc", "== Selected\n")?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    let text = ":part: child.adoc\n\ninclude::{part}[]\n\ninclude::optional.adoc[opts=optional]\n\nifdef::not-set[]\ninclude::inactive.adoc[]\nendif::[]\n";
    workspace.update_document(book.clone(), text.into(), 1);
    assert!(has_anchor(&workspace, &book, "_selected"));
    assert!(
        workspace
            .diagnostics_for(&book)
            .0
            .iter()
            .all(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::HINT))
    );
    assert!(
        !workspace
            .files_changed(&[project.uri("inactive.adoc")?])
            .contains(&book)
    );
    Ok(())
}

#[test]
fn non_file_documents_keep_includes_disabled() -> TestResult {
    let workspace = Workspace::new();
    let uri: Uri = "untitled:book".parse()?;
    workspace.update_document(uri.clone(), "include::Cargo.toml[]\n".into(), 1);
    let document = workspace.get_document(&uri).ok_or("missing document")?;
    assert_eq!(document.text(), "include::Cargo.toml[]\n");
    assert_eq!(document.includes.len(), 1);
    assert!(!workspace.include_dependencies.contains_key(&uri));
    Ok(())
}

#[test]
fn remote_includes_stay_disabled_in_server_mode() -> TestResult {
    let project = Project::new()?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    workspace.update_document(
        book.clone(),
        ":allow-uri-read:\n\ninclude::https://example.invalid/chapter.adoc[]\n".into(),
        1,
    );
    assert!(
        workspace
            .diagnostics_for(&book)
            .0
            .iter()
            .any(|d| d.message.contains("URI access is disabled"))
    );
    assert!(
        workspace
            .include_dependencies
            .get(&book)
            .is_some_and(|paths| paths.is_empty())
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_include_outside_files_or_buffers() -> TestResult {
    let project = Project::new()?;
    let outside = Project::new()?;
    outside.write("private.adoc", "== Private\n")?;
    std::os::unix::fs::symlink(outside.0.join("private.adoc"), project.0.join("link.adoc"))?;
    let workspace = Workspace::new();
    workspace.update_document(
        outside.uri("private.adoc")?,
        "== Private buffer\n".into(),
        1,
    );
    let book = project.uri("book.adoc")?;
    workspace.update_document(book.clone(), "include::link.adoc[]\n".into(), 1);
    assert!(!has_anchor(&workspace, &book, "_private"));
    assert!(!has_anchor(&workspace, &book, "_private_buffer"));
    assert!(
        workspace
            .diagnostics_for(&book)
            .0
            .iter()
            .any(|d| d.message.contains("not readable"))
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unsaved_files_beneath_symlinked_folders_refresh_their_parents() -> TestResult {
    let project = Project::new()?;
    fs::create_dir(project.0.join("real"))?;
    std::os::unix::fs::symlink(project.0.join("real"), project.0.join("alias"))?;
    let workspace = Workspace::new();
    let book = project.uri("book.adoc")?;
    workspace.update_document(book.clone(), "include::alias/child.adoc[]\n".into(), 1);
    workspace.update_document(project.uri("real/child.adoc")?, "== New\n".into(), 1);
    assert!(has_anchor(&workspace, &book, "_new"));
    assert_eq!(workspace.diagnostics_for(&book).0, []);
    Ok(())
}
