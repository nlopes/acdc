//! Document Links: make URLs, file references, and includes clickable

use std::ops::ControlFlow;

use acdc_converters_core::InlineTextTransform;
use acdc_parser::{Location, ast::HyperlinkRef};
use tower_lsp_server::ls_types::{DocumentLink, Uri};

use super::link_occurrences::{LinkOccurrence, visit_links};

use crate::{
    convert::{is_primary_location, location_to_utf16_range, resolve_relative_uri},
    state::DocumentState,
};

/// Collected link information
struct LinkInfo {
    target: String,
    location: Location,
    tooltip: Option<String>,
}

/// Collect all document links (clickable URLs, file references, and includes)
#[must_use]
pub(crate) fn collect_document_links(doc: &DocumentState, doc_uri: &Uri) -> Vec<DocumentLink> {
    let mut links = Vec::new();

    if let Some(ast) = doc.ast() {
        let _ = visit_links(ast.document_ref(), |link| {
            let info = match link {
                LinkOccurrence::Hyperlink(link) => LinkInfo {
                    target: link.target().to_string(),
                    location: link.location().into(),
                    tooltip: match link {
                        HyperlinkRef::Link(explicit) if !explicit.text().is_empty() => {
                            Some(InlineTextTransform::default().content_to_string(explicit.text()))
                        }
                        HyperlinkRef::Link(_)
                        | HyperlinkRef::Url(_)
                        | HyperlinkRef::Mailto(_)
                        | HyperlinkRef::Autolink(_)
                        | _ => None,
                    },
                },
                LinkOccurrence::Image(image) => LinkInfo {
                    target: image.source().to_string(),
                    location: image.location().into(),
                    tooltip: Some("Open image".to_string()),
                },
            };
            links.push(info);
            ControlFlow::<()>::Continue(())
        });
    }

    let mut result: Vec<DocumentLink> = links
        .into_iter()
        .filter(|info| is_primary_location(&info.location))
        .filter_map(|info| {
            let range = location_to_utf16_range(doc.text(), &info.location)?;
            let target = if info.target.starts_with("http://")
                || info.target.starts_with("https://")
                || info.target.starts_with("mailto:")
                || info.target.starts_with("ftp://")
                || info.target.starts_with("file://")
            {
                info.target.parse().ok()
            } else {
                // Resolve relative paths against the document's directory
                resolve_relative_uri(doc_uri, &info.target)
            };

            target.map(|uri| DocumentLink {
                range,
                target: Some(uri),
                tooltip: info.tooltip,
                data: None,
            })
        })
        .collect();

    // Add include directives as clickable links
    for (include_target, include_loc) in &doc.includes {
        if let Some(target_uri) = resolve_relative_uri(doc_uri, include_target)
            && let Some(range) = location_to_utf16_range(doc.text(), include_loc)
        {
            result.push(DocumentLink {
                range,
                target: Some(target_uri),
                tooltip: Some(format!("Open included file: {include_target}")),
                data: None,
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;

    #[test]
    fn test_collect_urls() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"= Document

Visit https://example.com for more info.

Also see link:https://rust-lang.org[Rust].
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let links = collect_document_links(&doc, &uri);
        assert_eq!(links.len(), 2);
        Ok(())
    }

    #[test]
    fn test_collect_mailto() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"= Document

Contact mailto:test@example.com[us] for help.
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let links = collect_document_links(&doc, &uri);
        assert_eq!(links.len(), 1);

        let link = links.first();
        assert!(link.is_some(), "expected at least one link");
        assert!(link.is_some_and(|l| {
            l.target
                .as_ref()
                .is_some_and(|u| u.as_str().starts_with("mailto:"))
        }));
        Ok(())
    }

    #[test]
    fn test_include_directives_as_links() -> Result<(), Box<dyn std::error::Error>> {
        let content = "= Document\n\ninclude::chapter1.adoc[]\n\nSome text.\n";
        let workspace = Workspace::new();
        let uri = "file:///docs/main.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let links = collect_document_links(&doc, &uri);

        // Should have the include directive as a clickable link
        let include_links: Vec<_> = links
            .iter()
            .filter(|l| {
                l.tooltip
                    .as_deref()
                    .is_some_and(|t| t.contains("included file"))
            })
            .collect();
        assert_eq!(include_links.len(), 1, "Expected 1 include link");

        let include_link = include_links.first().ok_or("expected include link")?;
        assert!(
            include_link
                .target
                .as_ref()
                .is_some_and(|u| u.as_str().ends_with("chapter1.adoc"))
        );
        Ok(())
    }

    #[test]
    fn test_resolve_relative_uri() -> Result<(), Box<dyn std::error::Error>> {
        let doc_uri = "file:///docs/main.adoc".parse::<Uri>()?;
        let resolved = resolve_relative_uri(&doc_uri, "chapter.adoc");
        assert!(resolved.is_some());
        assert_eq!(
            resolved.map(|u| u.to_string()),
            Some("file:///docs/chapter.adoc".to_string())
        );
        Ok(())
    }
}
