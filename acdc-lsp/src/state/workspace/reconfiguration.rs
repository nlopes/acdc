use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use tower_lsp_server::ls_types::Uri;

use super::Workspace;
use crate::{
    config::{AnalysisBackend, RootConfiguration},
    convert::file_path_to_uri,
};

type TestResult = Result<(), Box<dyn Error>>;

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
