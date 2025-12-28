//! Completion: suggest xref targets, attributes, and include paths

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Position,
};

use crate::state::DocumentState;

/// Built-in `AsciiDoc` attributes that are commonly used
const BUILTIN_ATTRIBUTES: &[(&str, &str)] = &[
    ("author", "Document author name"),
    ("email", "Author email address"),
    ("revdate", "Document revision date"),
    ("revnumber", "Document revision number"),
    ("revremark", "Document revision remark"),
    ("doctitle", "Document title"),
    ("doctype", "Document type (article, book, manpage, inline)"),
    ("description", "Document description for metadata"),
    ("keywords", "Document keywords for metadata"),
    ("icons", "Icon mode (font, image, or unset for text)"),
    ("iconsdir", "Directory for custom icons"),
    ("imagesdir", "Base directory for images"),
    ("toc", "Table of contents placement"),
    ("toclevels", "Number of section levels in TOC"),
    ("sectnums", "Enable section numbering"),
    ("sectnumlevels", "Depth of section numbering"),
    ("sectanchors", "Add anchors to section titles"),
    ("sectlinks", "Make section titles into links"),
    ("source-highlighter", "Source code highlighter"),
    ("stem", "STEM notation interpreter (asciimath, latexmath)"),
    (
        "experimental",
        "Enable experimental features like kbd macro",
    ),
    ("nofooter", "Suppress footer"),
    ("noheader", "Suppress header"),
    ("notitle", "Suppress document title"),
    ("showtitle", "Show document title in body"),
    ("hide-uri-scheme", "Hide URI scheme in autolinks"),
    ("linkattrs", "Parse attributes in link macros"),
    ("hardbreaks", "Preserve hard line breaks"),
    ("compat-mode", "Enable compatibility mode"),
];

/// Detect completion context from cursor position and text
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    /// After `:` at the start of a line (attribute definition)
    AttributeDefinition { prefix: String },
    /// After `{` (attribute reference)
    AttributeReference { prefix: String },
    /// After `<<` or `xref:` (cross-reference target)
    CrossReference { prefix: String },
    /// After `include::` (include path)
    IncludePath { prefix: String },
    /// No completion context detected
    None,
}

/// Compute completion items for a position
#[must_use]
pub fn compute_completions(doc: &DocumentState, position: Position) -> Option<Vec<CompletionItem>> {
    let context = detect_context(&doc.text, position)?;

    match context {
        CompletionContext::CrossReference { prefix } => {
            Some(complete_cross_references(doc, &prefix))
        }
        CompletionContext::AttributeReference { prefix } => {
            Some(complete_attribute_references(doc, &prefix))
        }
        CompletionContext::AttributeDefinition { prefix } => {
            Some(complete_attribute_definitions(&prefix))
        }
        CompletionContext::IncludePath { prefix: _ } => {
            // Include path completion requires filesystem access - skip for MVP
            Some(vec![])
        }
        CompletionContext::None => None,
    }
}

/// Detect the completion context from cursor position
fn detect_context(text: &str, position: Position) -> Option<CompletionContext> {
    let line_num = position.line as usize;
    let char_num = position.character as usize;

    // Get the line at cursor
    let line = text.lines().nth(line_num)?;

    // Get text before cursor on this line
    let before_cursor: String = line.chars().take(char_num).collect();

    // Check for cross-reference patterns: << or xref:
    if let Some(xref_start) = before_cursor.rfind("<<") {
        let prefix = &before_cursor[xref_start + 2..];
        // Make sure we're not past a closing >>
        if !prefix.contains(">>") {
            return Some(CompletionContext::CrossReference {
                prefix: prefix.to_string(),
            });
        }
    }
    if let Some(xref_start) = before_cursor.rfind("xref:") {
        let prefix = &before_cursor[xref_start + 5..];
        // Make sure we're not past a closing ]
        if !prefix.contains('[') {
            return Some(CompletionContext::CrossReference {
                prefix: prefix.to_string(),
            });
        }
    }

    // Check for attribute reference: {
    if let Some(attr_start) = before_cursor.rfind('{') {
        let prefix = &before_cursor[attr_start + 1..];
        // Make sure we're not past a closing }
        if !prefix.contains('}') {
            return Some(CompletionContext::AttributeReference {
                prefix: prefix.to_string(),
            });
        }
    }

    // Check for include path: include::
    if let Some(include_start) = before_cursor.rfind("include::") {
        let prefix = &before_cursor[include_start + 9..];
        // Make sure we're not past a closing ]
        if !prefix.contains('[') {
            return Some(CompletionContext::IncludePath {
                prefix: prefix.to_string(),
            });
        }
    }

    // Check for attribute definition: : at start of line
    if before_cursor.starts_with(':') && !before_cursor.contains("::") {
        let prefix = &before_cursor[1..];
        // Make sure we're not past the closing :
        if !prefix.contains(':') {
            return Some(CompletionContext::AttributeDefinition {
                prefix: prefix.to_string(),
            });
        }
    }

    Some(CompletionContext::None)
}

/// Complete cross-reference targets from document anchors
fn complete_cross_references(doc: &DocumentState, prefix: &str) -> Vec<CompletionItem> {
    doc.anchors
        .keys()
        .filter(|id| id.starts_with(prefix))
        .map(|id| CompletionItem {
            label: id.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" anchor".to_string()),
                description: None,
            }),
            ..Default::default()
        })
        .collect()
}

/// Complete attribute references from document and built-in attributes
fn complete_attribute_references(doc: &DocumentState, prefix: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Add document-defined attributes
    if let Some(ast) = &doc.ast {
        for (name, _value) in ast.attributes.iter() {
            if name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: Some(" document".to_string()),
                        description: None,
                    }),
                    ..Default::default()
                });
            }
        }
    }

    // Add built-in attributes
    for (name, desc) in BUILTIN_ATTRIBUTES {
        if name.starts_with(prefix) {
            items.push(CompletionItem {
                label: (*name).to_string(),
                kind: Some(CompletionItemKind::CONSTANT),
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(" built-in".to_string()),
                    description: None,
                }),
                detail: Some((*desc).to_string()),
                ..Default::default()
            });
        }
    }

    items
}

/// Complete attribute definitions (names for defining new attributes)
fn complete_attribute_definitions(prefix: &str) -> Vec<CompletionItem> {
    BUILTIN_ATTRIBUTES
        .iter()
        .filter(|(name, _)| name.starts_with(prefix))
        .map(|(name, desc)| CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some((*desc).to_string()),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_xref_context() {
        // Test << syntax
        let context = detect_context(
            "See <<my-sec",
            Position {
                line: 0,
                character: 12,
            },
        );
        assert_eq!(
            context,
            Some(CompletionContext::CrossReference {
                prefix: "my-sec".to_string()
            })
        );

        // Test xref: syntax
        let context = detect_context(
            "See xref:target",
            Position {
                line: 0,
                character: 15,
            },
        );
        assert_eq!(
            context,
            Some(CompletionContext::CrossReference {
                prefix: "target".to_string()
            })
        );
    }

    #[test]
    fn test_detect_attribute_reference_context() {
        let context = detect_context(
            "The {doc",
            Position {
                line: 0,
                character: 8,
            },
        );
        assert_eq!(
            context,
            Some(CompletionContext::AttributeReference {
                prefix: "doc".to_string()
            })
        );
    }

    #[test]
    fn test_detect_attribute_definition_context() {
        let context = detect_context(
            ":toc",
            Position {
                line: 0,
                character: 4,
            },
        );
        assert_eq!(
            context,
            Some(CompletionContext::AttributeDefinition {
                prefix: "toc".to_string()
            })
        );
    }

    #[test]
    fn test_detect_include_context() {
        let context = detect_context(
            "include::path/to",
            Position {
                line: 0,
                character: 17,
            },
        );
        assert_eq!(
            context,
            Some(CompletionContext::IncludePath {
                prefix: "path/to".to_string()
            })
        );
    }

    #[test]
    fn test_no_context_after_closed() {
        // After >> is closed
        let context = detect_context(
            "See <<section>> more",
            Position {
                line: 0,
                character: 20,
            },
        );
        assert_eq!(context, Some(CompletionContext::None));

        // After } is closed
        let context = detect_context(
            "Value: {attr} more",
            Position {
                line: 0,
                character: 18,
            },
        );
        assert_eq!(context, Some(CompletionContext::None));
    }

    #[test]
    fn test_complete_anchors() -> Result<(), acdc_parser::Error> {
        use crate::capabilities::definition::{collect_anchors, collect_xrefs};
        use acdc_parser::Options;

        let content = r"[[first-section]]
== First Section

[[second-section]]
== Second Section
";
        let options = Options::default();
        let ast = acdc_parser::parse(content, &options)?;
        let anchors = collect_anchors(&ast);
        let xrefs = collect_xrefs(&ast);
        let doc = DocumentState::new_success(content.to_string(), 1, ast, anchors, xrefs);

        let items = complete_cross_references(&doc, "first");
        assert_eq!(items.len(), 1);
        let item = items.first();
        assert!(item.is_some(), "expected at least one item");
        assert_eq!(item.map(|i| &i.label), Some(&"first-section".to_string()));

        // Test with empty prefix gets all anchors
        let items = complete_cross_references(&doc, "");
        assert_eq!(items.len(), 2);
        Ok(())
    }
}
