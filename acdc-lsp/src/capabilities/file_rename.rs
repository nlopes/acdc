//! File rename: update cross-file references when files are renamed or moved

use std::collections::HashMap;
use std::path::{Component, Path};

use tower_lsp::lsp_types::{FileRename, TextEdit, Url, WorkspaceEdit};

use crate::convert::location_to_range;
use crate::state::{Workspace, XrefTarget};

/// Compute workspace edits to update references when files are renamed.
///
/// Scans all open documents for xrefs, includes, and link macros that reference
/// any of the renamed files, and returns text edits to rewrite the paths.
#[must_use]
pub fn compute_file_rename_edits(
    workspace: &Workspace,
    renames: &[FileRename],
) -> Option<WorkspaceEdit> {
    let rename_map: HashMap<Url, Url> = renames
        .iter()
        .filter_map(|r| {
            let old = Url::parse(&r.old_uri).ok()?;
            let new = Url::parse(&r.new_uri).ok()?;
            Some((old, new))
        })
        .collect();

    if rename_map.is_empty() {
        return None;
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    workspace.for_each_document(|doc_uri, doc_state| {
        let Ok(doc_path) = doc_uri.to_file_path() else {
            return;
        };
        let Some(doc_dir) = doc_path.parent() else {
            return;
        };

        // Update xref targets
        for (target, location) in &doc_state.xrefs {
            let parsed = XrefTarget::parse(target);
            let Some(file_part) = &parsed.file else {
                continue;
            };

            let Some(resolved) = workspace.resolve_xref_file(doc_uri, file_part) else {
                continue;
            };

            let Some(new_uri) = rename_map.get(&resolved) else {
                continue;
            };

            let Ok(new_path) = new_uri.to_file_path() else {
                continue;
            };

            let new_relative = compute_relative_path(doc_dir, &new_path);
            let new_target = match &parsed.anchor {
                Some(anchor) => format!("{new_relative}#{anchor}"),
                None => new_relative,
            };

            changes.entry(doc_uri.clone()).or_default().push(TextEdit {
                range: location_to_range(location),
                new_text: new_target,
            });
        }

        // Update include targets
        for (target, location) in &doc_state.includes {
            let Some(resolved) = resolve_relative_uri(doc_uri, target) else {
                continue;
            };

            let Some(new_uri) = rename_map.get(&resolved) else {
                continue;
            };

            let Ok(new_path) = new_uri.to_file_path() else {
                continue;
            };

            let new_relative = compute_relative_path(doc_dir, &new_path);

            changes.entry(doc_uri.clone()).or_default().push(TextEdit {
                range: location_to_range(location),
                new_text: new_relative,
            });
        }
    });

    // Also scan non-open workspace files
    scan_workspace_files_for_renames(workspace, &rename_map, &mut changes);

    if changes.is_empty() {
        return None;
    }

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

/// Update workspace state after files have been renamed.
pub fn update_workspace_after_rename(workspace: &Workspace, renames: &[FileRename]) {
    for rename in renames {
        let Some(old_uri) = Url::parse(&rename.old_uri).ok() else {
            continue;
        };
        let Some(new_uri) = Url::parse(&rename.new_uri).ok() else {
            continue;
        };
        workspace.rename_document_uri(&old_uri, &new_uri);
    }
}

/// Scan non-open workspace files on disk for references to renamed files.
fn scan_workspace_files_for_renames(
    workspace: &Workspace,
    rename_map: &HashMap<Url, Url>,
    changes: &mut HashMap<Url, Vec<TextEdit>>,
) {
    let files = workspace.discover_workspace_files();

    for file_path in files {
        let Ok(file_uri) = Url::from_file_path(&file_path) else {
            continue;
        };

        // Skip files already open (handled by for_each_document)
        if workspace.has_document(&file_uri) {
            continue;
        }

        // Skip files that are themselves being renamed
        if rename_map.contains_key(&file_uri) {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(&file_path) else {
            continue;
        };

        let Some(doc_dir) = file_path.parent() else {
            continue;
        };

        // Parse and scan for references
        let options = acdc_parser::Options::default();
        let Ok(doc) = acdc_parser::parse(&text, &options) else {
            continue;
        };

        let xrefs = super::definition::collect_xrefs(&doc);
        for (target, location) in &xrefs {
            let parsed = XrefTarget::parse(target);
            let Some(file_part) = &parsed.file else {
                continue;
            };
            let Some(resolved) = workspace.resolve_xref_file(&file_uri, file_part) else {
                continue;
            };
            let Some(new_uri) = rename_map.get(&resolved) else {
                continue;
            };
            let Ok(new_path) = new_uri.to_file_path() else {
                continue;
            };
            let new_relative = compute_relative_path(doc_dir, &new_path);
            let new_target = match &parsed.anchor {
                Some(anchor) => format!("{new_relative}#{anchor}"),
                None => new_relative,
            };
            changes.entry(file_uri.clone()).or_default().push(TextEdit {
                range: location_to_range(location),
                new_text: new_target,
            });
        }

        // Scan includes from raw text
        let includes = extract_includes_for_scan(&text);
        for (target, location) in &includes {
            let Some(resolved) = resolve_relative_uri(&file_uri, target) else {
                continue;
            };
            let Some(new_uri) = rename_map.get(&resolved) else {
                continue;
            };
            let Ok(new_path) = new_uri.to_file_path() else {
                continue;
            };
            let new_relative = compute_relative_path(doc_dir, &new_path);
            changes.entry(file_uri.clone()).or_default().push(TextEdit {
                range: location_to_range(location),
                new_text: new_relative,
            });
        }
    }
}

/// Extract include directives from raw text (mirrors `DocumentState::extract_includes`).
fn extract_includes_for_scan(text: &str) -> Vec<(String, acdc_parser::Location)> {
    let mut includes = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("include::")
            && let Some(bracket_pos) = rest.find('[')
        {
            let target = &rest[..bracket_pos];
            if !target.is_empty() {
                let col_offset = line.find("include::").unwrap_or(0);
                let target_start = col_offset + "include::".len();
                let target_end = target_start + target.len();

                let mut location = acdc_parser::Location::default();
                location.start.line = line_idx + 1;
                location.start.column = target_start + 1;
                location.end.line = line_idx + 1;
                location.end.column = target_end;

                let line_start: usize = text.lines().take(line_idx).map(|l| l.len() + 1).sum();
                location.absolute_start = line_start + target_start;
                location.absolute_end = line_start + target_end;

                includes.push((target.to_string(), location));
            }
        }
    }

    includes
}

/// Resolve a relative path against a document URI's directory.
fn resolve_relative_uri(doc_uri: &Url, relative_path: &str) -> Option<Url> {
    let mut base = doc_uri.clone();
    base.path_segments_mut().ok()?.pop();
    let base_str = base.as_str();
    let base = if base_str.ends_with('/') {
        base
    } else {
        Url::parse(&format!("{base_str}/")).ok()?
    };
    base.join(relative_path).ok()
}

/// Compute the relative path from a directory to a target file.
///
/// Both paths must be absolute. Returns a forward-slash separated relative
/// path suitable for use in `AsciiDoc` references.
fn compute_relative_path(from_dir: &Path, to_path: &Path) -> String {
    // Find the common prefix length
    let from_components: Vec<Component<'_>> = from_dir.components().collect();
    let to_components: Vec<Component<'_>> = to_path.components().collect();

    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // Number of ".." needed to go up from from_dir to common ancestor
    let ups = from_components.len() - common_len;

    // Remaining components of to_path after common prefix
    let remaining: Vec<&str> = to_components
        .get(common_len..)
        .unwrap_or(&[])
        .iter()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => None,
        })
        .collect();

    let mut parts: Vec<&str> = std::iter::repeat_n("..", ups).collect();
    parts.extend(remaining);

    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;

    #[test]
    fn test_compute_relative_path_same_dir() {
        let from = Path::new("/docs");
        let to = Path::new("/docs/new.adoc");
        assert_eq!(compute_relative_path(from, to), "new.adoc");
    }

    #[test]
    fn test_compute_relative_path_subdirectory() {
        let from = Path::new("/docs");
        let to = Path::new("/docs/guides/setup.adoc");
        assert_eq!(compute_relative_path(from, to), "guides/setup.adoc");
    }

    #[test]
    fn test_compute_relative_path_parent_directory() {
        let from = Path::new("/docs/guides");
        let to = Path::new("/docs/setup.adoc");
        assert_eq!(compute_relative_path(from, to), "../setup.adoc");
    }

    #[test]
    fn test_compute_relative_path_sibling_directory() {
        let from = Path::new("/docs/guides");
        let to = Path::new("/docs/reference/api.adoc");
        assert_eq!(compute_relative_path(from, to), "../reference/api.adoc");
    }

    #[test]
    fn test_same_directory_rename_xref() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let doc_uri = Url::parse("file:///docs/index.adoc")?;
        let content = "= Index\n\nSee xref:old.adoc#intro[introduction].\n";
        workspace.update_document(doc_uri.clone(), content.to_string(), 1);

        let renames = vec![FileRename {
            old_uri: "file:///docs/old.adoc".to_string(),
            new_uri: "file:///docs/new.adoc".to_string(),
        }];

        let result = compute_file_rename_edits(&workspace, &renames);
        assert!(result.is_some(), "expected edits");

        let edit = result.ok_or("expected edit")?;
        let changes = edit.changes.ok_or("expected changes")?;
        let edits = changes.get(&doc_uri).ok_or("expected edits for doc")?;
        assert_eq!(edits.len(), 1);
        assert_eq!(edits.first().ok_or("no edit")?.new_text, "new.adoc#intro");

        Ok(())
    }

    #[test]
    fn test_cross_directory_move_xref() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let doc_uri = Url::parse("file:///docs/index.adoc")?;
        let content = "= Index\n\nSee xref:setup.adoc[setup guide].\n";
        workspace.update_document(doc_uri.clone(), content.to_string(), 1);

        let renames = vec![FileRename {
            old_uri: "file:///docs/setup.adoc".to_string(),
            new_uri: "file:///docs/guides/setup.adoc".to_string(),
        }];

        let result = compute_file_rename_edits(&workspace, &renames);
        assert!(result.is_some(), "expected edits");

        let edit = result.ok_or("expected edit")?;
        let changes = edit.changes.ok_or("expected changes")?;
        let edits = changes.get(&doc_uri).ok_or("expected edits for doc")?;
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits.first().ok_or("no edit")?.new_text,
            "guides/setup.adoc"
        );

        Ok(())
    }

    #[test]
    fn test_include_rewriting() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let doc_uri = Url::parse("file:///docs/main.adoc")?;
        let content = "= Main\n\ninclude::chapter.adoc[]\n";
        workspace.update_document(doc_uri.clone(), content.to_string(), 1);

        let renames = vec![FileRename {
            old_uri: "file:///docs/chapter.adoc".to_string(),
            new_uri: "file:///docs/parts/chapter.adoc".to_string(),
        }];

        let result = compute_file_rename_edits(&workspace, &renames);
        assert!(result.is_some(), "expected edits");

        let edit = result.ok_or("expected edit")?;
        let changes = edit.changes.ok_or("expected changes")?;
        let edits = changes.get(&doc_uri).ok_or("expected edits for doc")?;
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits.first().ok_or("no edit")?.new_text,
            "parts/chapter.adoc"
        );

        Ok(())
    }

    #[test]
    fn test_multiple_documents() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();

        let doc1_uri = Url::parse("file:///docs/index.adoc")?;
        let doc1_content = "= Index\n\nSee xref:shared.adoc#sec[link].\n";
        workspace.update_document(doc1_uri.clone(), doc1_content.to_string(), 1);

        let doc2_uri = Url::parse("file:///docs/other.adoc")?;
        let doc2_content = "= Other\n\ninclude::shared.adoc[]\n";
        workspace.update_document(doc2_uri.clone(), doc2_content.to_string(), 1);

        let renames = vec![FileRename {
            old_uri: "file:///docs/shared.adoc".to_string(),
            new_uri: "file:///docs/common/shared.adoc".to_string(),
        }];

        let result = compute_file_rename_edits(&workspace, &renames);
        assert!(result.is_some(), "expected edits");

        let edit = result.ok_or("expected edit")?;
        let changes = edit.changes.ok_or("expected changes")?;

        let edits1 = changes.get(&doc1_uri).ok_or("expected edits for doc1")?;
        assert_eq!(edits1.len(), 1);
        assert_eq!(
            edits1.first().ok_or("no edit")?.new_text,
            "common/shared.adoc#sec"
        );

        let edits2 = changes.get(&doc2_uri).ok_or("expected edits for doc2")?;
        assert_eq!(edits2.len(), 1);
        assert_eq!(
            edits2.first().ok_or("no edit")?.new_text,
            "common/shared.adoc"
        );

        Ok(())
    }

    #[test]
    fn test_anchor_preservation() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let doc_uri = Url::parse("file:///docs/index.adoc")?;
        let content = "= Index\n\nSee xref:guide.adoc#getting-started[start here].\n";
        workspace.update_document(doc_uri.clone(), content.to_string(), 1);

        let renames = vec![FileRename {
            old_uri: "file:///docs/guide.adoc".to_string(),
            new_uri: "file:///docs/tutorial.adoc".to_string(),
        }];

        let result = compute_file_rename_edits(&workspace, &renames);
        assert!(result.is_some());

        let edit = result.ok_or("expected edit")?;
        let changes = edit.changes.ok_or("expected changes")?;
        let edits = changes.get(&doc_uri).ok_or("expected edits")?;
        assert_eq!(
            edits.first().ok_or("no edit")?.new_text,
            "tutorial.adoc#getting-started"
        );

        Ok(())
    }

    #[test]
    fn test_same_file_xrefs_unaffected() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let doc_uri = Url::parse("file:///docs/index.adoc")?;
        let content = "= Index\n\n[[my-anchor]]\n== Section\n\nSee <<my-anchor>>.\n";
        workspace.update_document(doc_uri.clone(), content.to_string(), 1);

        let renames = vec![FileRename {
            old_uri: "file:///docs/other.adoc".to_string(),
            new_uri: "file:///docs/renamed.adoc".to_string(),
        }];

        let result = compute_file_rename_edits(&workspace, &renames);
        assert!(result.is_none(), "expected no edits for same-file xrefs");

        Ok(())
    }

    #[test]
    fn test_no_matching_references() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let doc_uri = Url::parse("file:///docs/index.adoc")?;
        let content = "= Index\n\nJust regular text.\n";
        workspace.update_document(doc_uri.clone(), content.to_string(), 1);

        let renames = vec![FileRename {
            old_uri: "file:///docs/old.adoc".to_string(),
            new_uri: "file:///docs/new.adoc".to_string(),
        }];

        let result = compute_file_rename_edits(&workspace, &renames);
        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn test_workspace_state_update() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let old_uri = Url::parse("file:///docs/old.adoc")?;
        let content = "[[my-anchor]]\n== Section\n\nContent.\n";
        workspace.update_document(old_uri.clone(), content.to_string(), 1);

        assert!(!workspace.find_anchor_globally("my-anchor").is_empty());

        let new_uri = Url::parse("file:///docs/new.adoc")?;
        let renames = vec![FileRename {
            old_uri: old_uri.to_string(),
            new_uri: new_uri.to_string(),
        }];

        update_workspace_after_rename(&workspace, &renames);

        // Old URI should no longer have the document
        assert!(workspace.get_document(&old_uri).is_none());
        // New URI should have it
        assert!(workspace.get_document(&new_uri).is_some());
        // Anchor should be findable and point to new URI
        let anchors = workspace.find_anchor_globally("my-anchor");
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors.first().ok_or("no anchor")?.0, new_uri);

        Ok(())
    }

    #[test]
    fn test_disk_scan_finds_references() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = std::env::temp_dir().join("acdc_lsp_test_rename_disk");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;

        // Create two files: architecture.adoc references readme.adoc
        std::fs::write(
            tmp.join("architecture.adoc"),
            "= Architecture\n\nSee xref:readme.adoc#intro[intro].\n\ninclude::readme.adoc[]\n",
        )?;
        std::fs::write(
            tmp.join("readme.adoc"),
            "[[intro]]\n== Introduction\n\nContent.\n",
        )?;

        let workspace = Workspace::new();
        let root_url = Url::from_file_path(&tmp).map_err(|()| "bad path")?;
        workspace.set_workspace_roots(vec![root_url]);

        // Do NOT open any documents — test the disk scan path
        let old_uri =
            Url::from_file_path(tmp.join("readme.adoc")).map_err(|()| "bad old path")?;
        let new_uri =
            Url::from_file_path(tmp.join("guide.adoc")).map_err(|()| "bad new path")?;

        let renames = vec![FileRename {
            old_uri: old_uri.to_string(),
            new_uri: new_uri.to_string(),
        }];

        let result = compute_file_rename_edits(&workspace, &renames);
        assert!(result.is_some(), "expected edits from disk scan");

        let edit = result.ok_or("expected edit")?;
        let changes = edit.changes.ok_or("expected changes")?;

        let arch_uri =
            Url::from_file_path(tmp.join("architecture.adoc")).map_err(|()| "bad arch path")?;
        let edits = changes.get(&arch_uri).ok_or("expected edits for architecture.adoc")?;
        assert_eq!(edits.len(), 2, "expected xref + include edits");

        let texts: Vec<&str> = edits.iter().map(|e| e.new_text.as_str()).collect();
        assert!(texts.contains(&"guide.adoc#intro"), "xref not updated: {texts:?}");
        assert!(texts.contains(&"guide.adoc"), "include not updated: {texts:?}");

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }
}
