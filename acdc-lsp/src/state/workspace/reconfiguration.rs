use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use tower_lsp_server::ls_types::{FileRename, Uri};

use super::Workspace;
use crate::{
    capabilities::file_rename,
    config::{AnalysisBackend, RootConfiguration},
    convert::file_path_to_uri,
};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn closing_a_document_after_root_removal_does_not_restore_disk_symbols() -> TestResult {
    let project = Project::new()?;
    fs::write(project.0.join("doc.adoc"), "== Disk\n")?;
    let workspace = Workspace::new();
    let uri = project.uri("doc.adoc")?;
    workspace.initialize_analysis(AnalysisBackend::Html5, vec![project.uri("")?]);
    workspace.update_document(uri.clone(), "== Buffer\n".into(), 3);
    let mut configuration = workspace.analysis_configuration();
    configuration.replace_roots(Vec::new());
    workspace.apply_analysis_configuration(&configuration);
    assert!(workspace.has_document(&uri));
    assert!(!workspace.query_workspace_symbols("Buffer").is_empty());
    workspace.remove_document(&uri);
    assert_eq!(workspace.symbol_index_len(), 0);
    assert!(workspace.query_workspace_symbols("").is_empty());
    Ok(())
}

#[test]
fn closing_a_document_reindexes_when_either_overlapping_root_remains() -> TestResult {
    for keep_nested in [false, true] {
        let project = Project::new()?;
        fs::create_dir(project.0.join("nested"))?;
        fs::write(project.0.join("nested/doc.adoc"), "== Disk\n")?;
        let root = project.uri("")?;
        let nested_root = project.uri("nested")?;
        let uri = project.uri("nested/doc.adoc")?;
        let workspace = Workspace::new();
        workspace.initialize_analysis(
            AnalysisBackend::Html5,
            vec![root.clone(), nested_root.clone()],
        );
        workspace.update_document(uri.clone(), "== Buffer\n".into(), 4);
        let mut configuration = workspace.analysis_configuration();
        configuration.replace_roots(vec![RootConfiguration {
            uri: if keep_nested { nested_root } else { root },
            backend: None,
        }]);
        workspace.apply_analysis_configuration(&configuration);
        workspace.remove_document(&uri);
        assert_eq!(workspace.symbol_index_len(), 1);
        let symbols: Vec<_> = workspace
            .query_workspace_symbols("Disk")
            .into_iter()
            .filter(|(_, symbol)| symbol.name == "Disk")
            .collect();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols.first().ok_or("missing symbol")?.0, uri);
        assert!(workspace.query_workspace_symbols("Buffer").is_empty());
    }
    Ok(())
}

#[test]
fn closed_file_moves_reindex_only_destinations_in_active_roots() -> TestResult {
    let project = Project::new()?;
    fs::create_dir(project.0.join("active"))?;
    fs::create_dir(project.0.join("outside"))?;
    let inside_path = project.0.join("active/doc.adoc");
    let outside_path = project.0.join("outside/doc.adoc");
    fs::write(&inside_path, "== Moved\n")?;
    let inside = project.uri("active/doc.adoc")?;
    let outside = project.uri("outside/doc.adoc")?;
    let workspace = Workspace::new();
    workspace.initialize_analysis(AnalysisBackend::Html5, vec![project.uri("active")?]);
    workspace.scan_workspace_files();
    assert_eq!(workspace.symbol_index_len(), 1);
    fs::rename(&inside_path, &outside_path)?;
    file_rename::update_workspace_after_rename(
        &workspace,
        &[FileRename {
            old_uri: inside.to_string(),
            new_uri: outside.to_string(),
        }],
    );
    assert!(workspace.query_workspace_symbols("Moved").is_empty());
    assert_eq!(workspace.symbol_index_len(), 0);
    fs::rename(outside_path, inside_path)?;
    file_rename::update_workspace_after_rename(
        &workspace,
        &[FileRename {
            old_uri: outside.to_string(),
            new_uri: inside.to_string(),
        }],
    );
    assert_eq!(workspace.symbol_index_len(), 1);
    let symbols: Vec<_> = workspace
        .query_workspace_symbols("Moved")
        .into_iter()
        .filter(|(_, symbol)| symbol.name == "Moved")
        .collect();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols.first().ok_or("missing symbol")?.0, inside);
    Ok(())
}

#[test]
fn closing_an_included_buffer_after_root_removal_still_refreshes_its_parent() -> TestResult {
    let project = Project::new()?;
    fs::write(project.0.join("child.adoc"), "== Disk\n")?;
    let workspace = Workspace::new();
    workspace.initialize_analysis(AnalysisBackend::Html5, vec![project.uri("")?]);
    let child = project.uri("child.adoc")?;
    let book = project.uri("book.adoc")?;
    workspace.update_document(child.clone(), "== Buffer\n".into(), 3);
    workspace.update_document(book.clone(), "include::child.adoc[]\n".into(), 7);
    let mut configuration = workspace.analysis_configuration();
    configuration.replace_roots(Vec::new());
    workspace.apply_analysis_configuration(&configuration);
    assert!(
        workspace
            .get_document(&book)
            .is_some_and(|state| state.anchors.contains_key("_buffer"))
    );
    let affected = workspace.remove_document(&child);
    assert!(affected.contains(&book));
    assert!(workspace.get_document(&book).is_some_and(
        |state| state.anchors.contains_key("_disk") && !state.anchors.contains_key("_buffer")
    ));
    assert_eq!(workspace.diagnostics_for(&book).1, Some(7));
    assert_eq!(workspace.symbol_index_len(), 0);
    Ok(())
}

#[test]
fn closing_non_file_uris_does_not_index_a_matching_disk_path() -> TestResult {
    let project = Project::new()?;
    fs::write(project.0.join("doc.adoc"), "== Disk\n")?;
    let root = project.uri("")?;
    let file = project.uri("doc.adoc")?;
    let virtual_root: Uri = format!(
        "memfs:{}",
        root.as_str()
            .strip_prefix("file:")
            .ok_or("missing file scheme")?
    )
    .parse()?;
    let virtual_file: Uri = format!(
        "memfs:{}",
        file.as_str()
            .strip_prefix("file:")
            .ok_or("missing file scheme")?
    )
    .parse()?;
    let workspace = Workspace::new();
    workspace.initialize_analysis(AnalysisBackend::Html5, vec![virtual_root]);
    for uri in [virtual_file, "untitled:notes".parse()?] {
        workspace.update_document(uri.clone(), "== Buffer\n".into(), 1);
        workspace.remove_document(&uri);
        assert_eq!(workspace.symbol_index_len(), 0);
    }
    Ok(())
}

struct Project(PathBuf);

impl Project {
    fn new() -> Result<Self, Box<dyn Error>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "acdc-lsp-reconfiguration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path)?;
        Ok(Self(path.canonicalize()?))
    }

    fn uri(&self, name: &str) -> Result<Uri, Box<dyn Error>> {
        file_path_to_uri(&self.0.join(name)).ok_or_else(|| "invalid file URI".into())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cross_file_diagnostics(workspace: &Workspace, uri: &Uri) -> usize {
    workspace
        .diagnostics_for(uri)
        .0
        .iter()
        .filter(|diagnostic| diagnostic.message.starts_with("Cross-file reference:"))
        .count()
}

#[test]
fn scoped_backend_change_refreshes_unchanged_reference_sources() -> TestResult {
    let project = Project::new()?;
    let workspace = Workspace::new();
    let source = project.uri("source/index.adoc")?;
    let target = project.uri("target/index.adoc")?;
    let root = project.uri("target")?;
    // Disk has an anchor that the unsaved buffer must hide under HTML analysis.
    fs::create_dir(project.0.join("target"))?;
    fs::write(
        project.0.join("target/index.adoc"),
        "[[pdf-only]]\n== Disk\n",
    )?;
    workspace.update_document(
        target.clone(),
        "ifdef::backend-pdf[]\n[[pdf-only]]\n== PDF\nendif::[]\n".into(),
        8,
    );
    workspace.update_document(
        source.clone(),
        "See xref:../target/index.adoc#pdf-only[PDF].\n".into(),
        5,
    );
    let source_text = workspace
        .get_document(&source)
        .ok_or("missing source")?
        .parsed
        .text_snapshot();
    assert_eq!(cross_file_diagnostics(&workspace, &source), 1);

    let mut configuration = workspace.analysis_configuration();
    configuration.replace_roots(vec![RootConfiguration {
        uri: root,
        backend: Some(AnalysisBackend::Pdf),
    }]);
    let result = workspace.apply_analysis_configuration(&configuration);
    assert_eq!(result.diagnostic_uris, [source.clone(), target.clone()]);
    assert_eq!(cross_file_diagnostics(&workspace, &source), 0);
    assert_eq!(workspace.diagnostics_for(&source).1, Some(5));
    assert_eq!(workspace.diagnostics_for(&target).1, Some(8));
    assert!(Arc::ptr_eq(
        &source_text,
        &workspace
            .get_document(&source)
            .ok_or("missing source")?
            .parsed
            .text_snapshot()
    ));

    configuration.replace_roots(Vec::new());
    workspace.apply_analysis_configuration(&configuration);
    assert_eq!(cross_file_diagnostics(&workspace, &source), 1);
    Ok(())
}

#[test]
fn global_backend_change_checks_mutual_references_after_all_anchors_change() -> TestResult {
    for reverse in [false, true] {
        let project = Project::new()?;
        let workspace = Workspace::new();
        let first = project.uri("first.adoc")?;
        let second = project.uri("second.adoc")?;
        let mut documents = vec![
            (
                first.clone(),
                "ifdef::backend-pdf[]\n[[first-pdf]]\n== First\nendif::[]\n\nSee xref:second.adoc#second-pdf[].\n",
            ),
            (
                second.clone(),
                "ifdef::backend-pdf[]\n[[second-pdf]]\n== Second\nendif::[]\n\nSee xref:first.adoc#first-pdf[].\n",
            ),
        ];
        if reverse {
            documents.reverse();
        }
        for (uri, text) in documents {
            workspace.update_document(uri, text.into(), 4);
        }
        let mut configuration = workspace.analysis_configuration();
        for backend in [
            AnalysisBackend::Pdf,
            AnalysisBackend::Html5,
            AnalysisBackend::Pdf,
        ] {
            configuration.set_unscoped(Some(backend));
            let result = workspace.apply_analysis_configuration(&configuration);
            assert_eq!(result.diagnostic_uris, [first.clone(), second.clone()]);
            let expected = usize::from(backend == AnalysisBackend::Html5);
            assert_eq!(cross_file_diagnostics(&workspace, &first), expected);
            assert_eq!(cross_file_diagnostics(&workspace, &second), expected);
        }
    }
    Ok(())
}

#[test]
fn configuration_refreshes_included_buffer_diagnostics_and_clears_old_sources() -> TestResult {
    let project = Project::new()?;
    let workspace = Workspace::new();
    let child = project.uri("target/fragments/child.adoc")?;
    let book = project.uri("target/book.adoc")?;
    let source = project.uri("source.adoc")?;
    workspace.update_document(child.clone(),
        "ifdef::backend-pdf[]\n[[included]]\n== Included\n\nSee xref:absent.adoc#missing[].\nendif::[]\n".into(), 3);
    workspace.update_document(book.clone(), "include::fragments/child.adoc[]\n".into(), 7);
    workspace.update_document(
        source.clone(),
        "See xref:target/book.adoc#included[].\n".into(),
        2,
    );
    assert_eq!(cross_file_diagnostics(&workspace, &source), 1);
    assert_eq!(cross_file_diagnostics(&workspace, &child), 0);
    let child_diagnostics = workspace.diagnostics_for(&child);

    let mut configuration = workspace.analysis_configuration();
    configuration.replace_roots(vec![
        RootConfiguration {
            uri: project.uri("target")?,
            backend: Some(AnalysisBackend::Pdf),
        },
        RootConfiguration {
            uri: project.uri("target/fragments")?,
            backend: Some(AnalysisBackend::Html5),
        },
    ]);
    let result = workspace.apply_analysis_configuration(&configuration);
    assert_eq!(result.diagnostic_uris.len(), 3);
    for uri in [&book, &source, &child] {
        assert!(result.diagnostic_uris.contains(uri));
    }
    assert_eq!(cross_file_diagnostics(&workspace, &source), 0);
    assert_eq!(cross_file_diagnostics(&workspace, &child), 1);
    assert_eq!(workspace.diagnostics_for(&child).1, Some(3));
    assert_eq!(workspace.diagnostics_for(&book).1, Some(7));
    assert!(
        !workspace
            .get_document(&child)
            .ok_or("missing child")?
            .anchors
            .contains_key("included")
    );
    assert!(
        workspace
            .get_document(&book)
            .ok_or("missing book")?
            .anchors
            .contains_key("included")
    );
    assert!(
        workspace
            .include_dependencies
            .get(&book)
            .is_some_and(|paths| paths.contains(&project.0.join("target/fragments/child.adoc")))
    );

    configuration.replace_roots(Vec::new());
    let result = workspace.apply_analysis_configuration(&configuration);
    assert!(result.diagnostic_uris.contains(&child));
    assert_eq!(cross_file_diagnostics(&workspace, &source), 1);
    assert_eq!(workspace.diagnostics_for(&child), child_diagnostics);
    Ok(())
}

#[test]
fn configuration_refresh_retains_parser_warnings_without_duplicates() -> TestResult {
    let project = Project::new()?;
    let workspace = Workspace::new();
    let source = project.uri("source.adoc")?;
    workspace.update_document(source.clone(), "include::missing.adoc[]\n".into(), 9);
    let before = workspace.diagnostics_for(&source);
    assert_eq!(before.0.len(), 1);
    let mut configuration = workspace.analysis_configuration();
    for name in ["first", "second"] {
        configuration.replace_roots(vec![RootConfiguration {
            uri: project.uri(name)?,
            backend: Some(AnalysisBackend::Pdf),
        }]);
        workspace.apply_analysis_configuration(&configuration);
        assert_eq!(workspace.diagnostics_for(&source), before);
    }
    Ok(())
}

#[test]
fn adding_a_scoped_root_refreshes_closed_targets_before_diagnostics() -> TestResult {
    let project = Project::new()?;
    fs::create_dir(project.0.join("target"))?;
    fs::write(
        project.0.join("target/closed.adoc"),
        "ifdef::backend-pdf[]\n[[closed-pdf]]\n== Closed PDF\nendif::[]\n",
    )?;
    let workspace = Workspace::new();
    let source = project.uri("source.adoc")?;
    workspace.update_document(
        source.clone(),
        "See xref:target/closed.adoc#closed-pdf[].\n".into(),
        6,
    );
    assert_eq!(cross_file_diagnostics(&workspace, &source), 1);

    let mut configuration = workspace.analysis_configuration();
    configuration.replace_roots(vec![RootConfiguration {
        uri: project.uri("target")?,
        backend: Some(AnalysisBackend::Pdf),
    }]);
    workspace.apply_analysis_configuration(&configuration);
    assert_eq!(cross_file_diagnostics(&workspace, &source), 0);
    assert!(
        workspace
            .query_workspace_symbols("Closed PDF")
            .iter()
            .any(|(_, symbol)| symbol.name == "Closed PDF")
    );

    configuration.replace_roots(Vec::new());
    workspace.apply_analysis_configuration(&configuration);
    assert_eq!(cross_file_diagnostics(&workspace, &source), 1);
    assert!(workspace.query_workspace_symbols("Closed PDF").is_empty());
    Ok(())
}
