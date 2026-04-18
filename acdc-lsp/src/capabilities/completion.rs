//! Completion: suggest xref targets, attributes, and include paths

use std::path::Path;

use tower_lsp_server::ls_types::{
    Command, CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTextEdit,
    InsertTextFormat, Position, Range, TextEdit, Uri,
};

use crate::convert::uri_filename;
use crate::state::{DocumentState, Workspace};

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

/// Definition of a macro snippet completion
struct MacroSnippetDef {
    /// The macro name (e.g., "image")
    name: &'static str,
    /// Snippet for inline form (single colon), or None if no inline form
    inline_snippet: Option<&'static str>,
    /// Snippet for block form (double colon), or None if no block form
    block_snippet: Option<&'static str>,
    /// Short description shown in completion detail
    description: &'static str,
}

/// Macro snippets with tab stops for all supported `AsciiDoc` macros
const MACRO_SNIPPETS: &[MacroSnippetDef] = &[
    MacroSnippetDef {
        name: "image",
        inline_snippet: Some("image:${1:path}[${2:alt text}]"),
        block_snippet: Some("image::${1:path}[${2:alt text}]"),
        description: "Image",
    },
    MacroSnippetDef {
        name: "link",
        inline_snippet: Some("link:${1:url}[${2:link text}]"),
        block_snippet: None,
        description: "Explicit link",
    },
    MacroSnippetDef {
        name: "mailto",
        inline_snippet: Some("mailto:${1:address}[${2:text}]"),
        block_snippet: None,
        description: "Email link",
    },
    MacroSnippetDef {
        name: "icon",
        inline_snippet: Some("icon:${1:name}[${2:size}]"),
        block_snippet: None,
        description: "Icon reference",
    },
    MacroSnippetDef {
        name: "kbd",
        inline_snippet: Some("kbd:[${1:keys}]"),
        block_snippet: None,
        description: "Keyboard shortcut",
    },
    MacroSnippetDef {
        name: "btn",
        inline_snippet: Some("btn:[${1:label}]"),
        block_snippet: None,
        description: "UI button",
    },
    MacroSnippetDef {
        name: "menu",
        inline_snippet: Some("menu:${1:TopMenu}[${2:SubItem > SubItem}]"),
        block_snippet: None,
        description: "Menu navigation",
    },
    MacroSnippetDef {
        name: "footnote",
        inline_snippet: Some("footnote:[${1:content}]"),
        block_snippet: None,
        description: "Footnote",
    },
    MacroSnippetDef {
        name: "pass",
        inline_snippet: Some("pass:[${1:content}]"),
        block_snippet: None,
        description: "Passthrough",
    },
    MacroSnippetDef {
        name: "stem",
        inline_snippet: Some("stem:[${1:formula}]"),
        block_snippet: None,
        description: "Math formula",
    },
    MacroSnippetDef {
        name: "latexmath",
        inline_snippet: Some("latexmath:[${1:formula}]"),
        block_snippet: None,
        description: "LaTeX math formula",
    },
    MacroSnippetDef {
        name: "asciimath",
        inline_snippet: Some("asciimath:[${1:formula}]"),
        block_snippet: None,
        description: "AsciiMath formula",
    },
    MacroSnippetDef {
        name: "xref",
        inline_snippet: Some("xref:${1:target}[${2:text}]"),
        block_snippet: None,
        description: "Cross-reference",
    },
    MacroSnippetDef {
        name: "audio",
        inline_snippet: None,
        block_snippet: Some("audio::${1:path}[]"),
        description: "Audio block",
    },
    MacroSnippetDef {
        name: "video",
        inline_snippet: None,
        block_snippet: Some("video::${1:path}[]"),
        description: "Video block",
    },
    MacroSnippetDef {
        name: "toc",
        inline_snippet: None,
        block_snippet: Some("toc::[]"),
        description: "Table of contents",
    },
    MacroSnippetDef {
        name: "include",
        inline_snippet: None,
        block_snippet: Some("include::${1:path}[${2:leveloffset=${3:+1}}]"),
        description: "Include directive",
    },
];

/// Detect completion context from cursor position and text
#[derive(Debug, Clone, PartialEq)]
enum CompletionContext {
    /// After `:` at the start of a line (attribute definition)
    AttributeDefinition { prefix: String },
    /// After `{` (attribute reference)
    AttributeReference { prefix: String },
    /// After `<<` or `xref:` (cross-reference target)
    CrossReference { prefix: String },
    /// After `include::` (include path)
    IncludePath { prefix: String },
    /// Typing a macro name prefix (e.g., "ima" for image)
    MacroSnippet { prefix: String, at_line_start: bool },
    /// No completion context detected
    None,
}

/// Compute completion items for a position
#[must_use]
pub(crate) fn compute_completions(
    doc: &DocumentState,
    doc_uri: &Uri,
    workspace: &Workspace,
    position: Position,
) -> Option<Vec<CompletionItem>> {
    let context = detect_context(doc.text(), position)?;

    match context {
        CompletionContext::CrossReference { prefix } => {
            Some(complete_cross_references(doc, doc_uri, workspace, &prefix))
        }
        CompletionContext::AttributeReference { prefix } => {
            Some(complete_attribute_references(doc, &prefix))
        }
        CompletionContext::AttributeDefinition { prefix } => {
            Some(complete_attribute_definitions(&prefix))
        }
        CompletionContext::IncludePath { prefix } => {
            Some(complete_include_paths(doc_uri, &prefix, position))
        }
        CompletionContext::MacroSnippet {
            prefix,
            at_line_start,
        } => Some(complete_macro_snippets(&prefix, at_line_start, position)),
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

    // Check for macro snippet context: user is typing a word that could be a macro name
    if let Some(word_start) = find_word_start(&before_cursor) {
        let word = &before_cursor[word_start..];
        // Need at least 2 chars and word must not contain : or [ (already inside a macro)
        if word.len() >= 2
            && !word.contains(':')
            && !word.contains('[')
            && word_start
                .checked_sub(1)
                .is_none_or(|i| before_cursor.as_bytes().get(i) != Some(&b':'))
            && MACRO_SNIPPETS.iter().any(|m| m.name.starts_with(word))
        {
            let at_line_start = before_cursor[..word_start].chars().all(char::is_whitespace);
            return Some(CompletionContext::MacroSnippet {
                prefix: word.to_string(),
                at_line_start,
            });
        }
    }

    Some(CompletionContext::None)
}

/// Find the byte index where the current word starts (looking backwards from end).
/// Returns `None` if there's no alphabetic word at the cursor.
fn find_word_start(before_cursor: &str) -> Option<usize> {
    let bytes = before_cursor.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut i = bytes.len();
    while i > 0 && bytes.get(i - 1).is_some_and(u8::is_ascii_alphabetic) {
        i -= 1;
    }
    if i == bytes.len() { None } else { Some(i) }
}

/// Complete macro names with snippet expansions
fn complete_macro_snippets(
    prefix: &str,
    at_line_start: bool,
    position: Position,
) -> Vec<CompletionItem> {
    let prefix_len = u32::try_from(prefix.len()).unwrap_or(0);
    let edit_range = Range {
        start: Position {
            line: position.line,
            character: position.character - prefix_len,
        },
        end: position,
    };

    let mut items = Vec::new();

    for def in MACRO_SNIPPETS {
        if !def.name.starts_with(prefix) {
            continue;
        }

        if let Some(snippet) = def.inline_snippet {
            items.push(CompletionItem {
                label: format!("{}:", def.name),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(format!("{} (inline)", def.description)),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: edit_range,
                    new_text: snippet.to_string(),
                })),
                filter_text: Some(def.name.to_string()),
                sort_text: Some(format!("0{}", def.name)),
                ..Default::default()
            });
        }

        if at_line_start && let Some(snippet) = def.block_snippet {
            items.push(CompletionItem {
                label: format!("{}::", def.name),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(format!("{} (block)", def.description)),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: edit_range,
                    new_text: snippet.to_string(),
                })),
                filter_text: Some(def.name.to_string()),
                sort_text: Some(format!("1{}", def.name)),
                ..Default::default()
            });
        }
    }

    items
}

/// Complete cross-reference targets from document anchors (local + cross-file)
fn complete_cross_references(
    doc: &DocumentState,
    doc_uri: &Uri,
    workspace: &Workspace,
    prefix: &str,
) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = doc
        .anchors
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
        .collect();

    // Add anchors from other open documents
    let mut seen: std::collections::HashSet<String> = doc.anchors.keys().cloned().collect();

    for (anchor_id, uri) in workspace.all_anchors() {
        if uri == *doc_uri || !anchor_id.starts_with(prefix) || seen.contains(&anchor_id) {
            continue;
        }
        seen.insert(anchor_id.clone());

        let file_name = uri_filename(&uri);

        items.push(CompletionItem {
            label: anchor_id.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!(" {file_name}")),
                description: None,
            }),
            insert_text: Some(format!("{file_name}#{anchor_id}")),
            ..Default::default()
        });
    }

    items
}

/// Complete attribute references from document and built-in attributes
fn complete_attribute_references(doc: &DocumentState, prefix: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Add document-defined attributes
    if let Some(ast) = doc.ast() {
        let ast = ast.document();
        for (name, _value) in ast.attributes.iter() {
            if name.as_ref().starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.to_string(),
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

// TODO(nlopes): add support for skipping anything in .gitignore files, which would be
// more robust than a hardcoded list of skip dirs
/// Directories to skip during include path completion
const SKIP_DIRS: &[&str] = &[".git", ".svn", ".hg", "target", "node_modules", ".build"];

/// `AsciiDoc` file extensions (prioritized in sort order)
const ADOC_EXTENSIONS: &[&str] = &["adoc", "asciidoc", "ad", "asc"];

/// Complete include paths by listing files and directories on the filesystem
fn complete_include_paths(doc_uri: &Uri, prefix: &str, position: Position) -> Vec<CompletionItem> {
    let Some(doc_path) = doc_uri.to_file_path() else {
        return vec![];
    };
    let Some(doc_dir) = doc_path.parent() else {
        return vec![];
    };

    // Split prefix into directory part and name filter
    // e.g. "chapters/ch" → dir_part="chapters/", filter="ch"
    // e.g. "ch" → dir_part="", filter="ch"
    // e.g. "chapters/" → dir_part="chapters/", filter=""
    let (dir_part, filter) = match prefix.rfind('/') {
        Some(pos) => (&prefix[..=pos], &prefix[pos + 1..]),
        None => ("", prefix),
    };

    let search_dir = if dir_part.is_empty() {
        doc_dir.to_path_buf()
    } else {
        doc_dir.join(dir_part)
    };

    let Ok(entries) = std::fs::read_dir(&search_dir) else {
        return vec![];
    };

    // The text edit range covers the entire prefix (everything after `include::`)
    let prefix_len = u32::try_from(prefix.len()).unwrap_or(0);
    let edit_range = Range {
        start: Position {
            line: position.line,
            character: position.character - prefix_len,
        },
        end: position,
    };

    let filter_lower = filter.to_lowercase();
    let mut items = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden entries
        if name_str.starts_with('.') {
            continue;
        }

        let is_dir = entry.file_type().is_ok_and(|ft| ft.is_dir());

        // Skip known non-useful directories
        if is_dir && SKIP_DIRS.contains(&name_str.as_ref()) {
            continue;
        }

        // Apply filter
        if !name_str.to_lowercase().starts_with(&filter_lower) {
            continue;
        }

        if is_dir {
            let new_text = format!("{dir_part}{name_str}/");
            items.push(CompletionItem {
                label: format!("{name_str}/"),
                kind: Some(CompletionItemKind::FOLDER),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: edit_range,
                    new_text,
                })),
                sort_text: Some(format!("0{name_str}")),
                command: Some(Command {
                    title: "Trigger Suggest".to_string(),
                    command: "editor.action.triggerSuggest".to_string(),
                    arguments: None,
                }),
                ..Default::default()
            });
        } else {
            let is_adoc = Path::new(&*name_str)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ADOC_EXTENSIONS.contains(&ext));
            let new_text = format!("{dir_part}{name_str}[]");
            items.push(CompletionItem {
                label: name_str.to_string(),
                kind: Some(CompletionItemKind::FILE),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: edit_range,
                    new_text,
                })),
                sort_text: Some(format!("{}{name_str}", if is_adoc { "1" } else { "2" })),
                label_details: if is_adoc {
                    Some(CompletionItemLabelDetails {
                        detail: Some(" AsciiDoc".to_string()),
                        description: None,
                    })
                } else {
                    None
                },
                ..Default::default()
            });
        }
    }

    items.sort_by(|a, b| a.sort_text.cmp(&b.sort_text));
    items
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
    fn test_complete_anchors() -> Result<(), Box<dyn std::error::Error>> {
        let content = r"[[first-section]]
== First Section

[[second-section]]
== Second Section
";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>()?;
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).ok_or("document not found")?;

        let items = complete_cross_references(&doc, &uri, &workspace, "first");
        assert_eq!(items.len(), 1);
        let item = items.first();
        assert!(item.is_some(), "expected at least one item");
        assert_eq!(item.map(|i| &i.label), Some(&"first-section".to_string()));

        // Test with empty prefix gets all anchors (at least local ones)
        let items = complete_cross_references(&doc, &uri, &workspace, "");
        assert!(items.len() >= 2);
        Ok(())
    }

    fn setup_include_test_dir(
        suffix: &str,
    ) -> Result<(std::path::PathBuf, Uri), Box<dyn std::error::Error>> {
        let tmp = std::env::temp_dir().join(format!("acdc_lsp_test_include_completion_{suffix}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("chapters"))?;
        std::fs::create_dir_all(tmp.join(".git"))?;
        std::fs::create_dir_all(tmp.join("target"))?;

        std::fs::write(tmp.join("intro.adoc"), "= Intro\n")?;
        std::fs::write(tmp.join("appendix.asciidoc"), "= Appendix\n")?;
        std::fs::write(tmp.join("data.csv"), "a,b,c\n")?;
        std::fs::write(tmp.join("chapters/chapter-01.adoc"), "= Ch 1\n")?;
        std::fs::write(tmp.join("chapters/chapter-02.adoc"), "= Ch 2\n")?;

        let doc_uri = Uri::from_file_path(tmp.join("main.adoc")).ok_or("bad path")?;
        Ok((tmp, doc_uri))
    }

    /// Helper: position simulating cursor right after `include::{prefix}`
    fn pos_for_prefix(prefix: &str) -> Position {
        let col = u32::try_from("include::".len() + prefix.len()).unwrap_or(0);
        Position {
            line: 0,
            character: col,
        }
    }

    /// Helper: extract `new_text` from a `CompletionItem`'s `text_edit`
    fn edit_text(item: &CompletionItem) -> Option<&str> {
        match &item.text_edit {
            Some(CompletionTextEdit::Edit(te)) => Some(&te.new_text),
            _ => None,
        }
    }

    #[test]
    fn test_complete_include_paths_lists_files_and_dirs() -> Result<(), Box<dyn std::error::Error>>
    {
        let (tmp, doc_uri) = setup_include_test_dir("list")?;

        let items = complete_include_paths(&doc_uri, "", pos_for_prefix(""));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(labels.contains(&"intro.adoc"), "missing intro.adoc");
        assert!(
            labels.contains(&"appendix.asciidoc"),
            "missing appendix.asciidoc"
        );
        assert!(labels.contains(&"data.csv"), "missing data.csv");
        assert!(labels.contains(&"chapters/"), "missing chapters/");

        // Hidden dirs and skip dirs should be excluded
        assert!(!labels.contains(&".git/"), ".git should be hidden");
        assert!(!labels.contains(&"target/"), "target should be skipped");

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_complete_include_paths_subdirectory() -> Result<(), Box<dyn std::error::Error>> {
        let (tmp, doc_uri) = setup_include_test_dir("subdir")?;

        let prefix = "chapters/";
        let items = complete_include_paths(&doc_uri, prefix, pos_for_prefix(prefix));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(labels.contains(&"chapter-01.adoc"));
        assert!(labels.contains(&"chapter-02.adoc"));
        assert_eq!(items.len(), 2);

        // Verify text_edit replaces the full prefix with the complete path
        let ch1 = items
            .iter()
            .find(|i| i.label == "chapter-01.adoc")
            .ok_or("chapter-01.adoc not found")?;
        assert_eq!(edit_text(ch1), Some("chapters/chapter-01.adoc[]"));

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_complete_include_paths_filter() -> Result<(), Box<dyn std::error::Error>> {
        let (tmp, doc_uri) = setup_include_test_dir("filter")?;

        let prefix = "int";
        let items = complete_include_paths(&doc_uri, prefix, pos_for_prefix(prefix));
        assert_eq!(items.len(), 1);
        let first = items.first().ok_or("expected at least one item")?;
        assert_eq!(first.label, "intro.adoc");
        assert_eq!(edit_text(first), Some("intro.adoc[]"));

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_complete_include_paths_nonexistent_dir() -> Result<(), Box<dyn std::error::Error>> {
        let doc_uri = "file:///nonexistent/dir/doc.adoc".parse::<Uri>()?;
        let items = complete_include_paths(&doc_uri, "", pos_for_prefix(""));
        assert!(items.is_empty());
        Ok(())
    }

    #[test]
    fn test_complete_include_paths_adoc_sorted_first() -> Result<(), Box<dyn std::error::Error>> {
        let (tmp, doc_uri) = setup_include_test_dir("sort")?;

        let items = complete_include_paths(&doc_uri, "", pos_for_prefix(""));
        // Directories (sort "0...") come first, then adoc files ("1..."), then others ("2...")
        let first_file = items
            .iter()
            .find(|i| i.kind == Some(CompletionItemKind::FILE))
            .ok_or("expected at least one file")?;
        assert!(
            first_file
                .sort_text
                .as_ref()
                .is_some_and(|s| s.starts_with('1')),
            "first file should be an adoc file (sort prefix '1')"
        );

        // data.csv should have sort prefix '2'
        let csv = items
            .iter()
            .find(|i| i.label == "data.csv")
            .ok_or("data.csv not found")?;
        assert!(csv.sort_text.as_ref().is_some_and(|s| s.starts_with('2')));

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_complete_include_paths_dir_retriggers() -> Result<(), Box<dyn std::error::Error>> {
        let (tmp, doc_uri) = setup_include_test_dir("retrigger")?;

        let items = complete_include_paths(&doc_uri, "", pos_for_prefix(""));
        let dir_item = items
            .iter()
            .find(|i| i.label == "chapters/")
            .ok_or("chapters/ not found")?;
        let command = dir_item
            .command
            .as_ref()
            .ok_or("directory items should have a retrigger command")?;
        assert_eq!(command.command, "editor.action.triggerSuggest");
        assert_eq!(
            edit_text(dir_item),
            Some("chapters/"),
            "directory edit text should include trailing slash"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn test_detect_macro_snippet_at_line_start() {
        let context = detect_context(
            "ima",
            Position {
                line: 0,
                character: 3,
            },
        );
        assert_eq!(
            context,
            Some(CompletionContext::MacroSnippet {
                prefix: "ima".to_string(),
                at_line_start: true,
            })
        );
    }

    #[test]
    fn test_detect_macro_snippet_mid_line() {
        let context = detect_context(
            "See ima",
            Position {
                line: 0,
                character: 7,
            },
        );
        assert_eq!(
            context,
            Some(CompletionContext::MacroSnippet {
                prefix: "ima".to_string(),
                at_line_start: false,
            })
        );
    }

    #[test]
    fn test_no_macro_snippet_single_char() {
        let context = detect_context(
            "i",
            Position {
                line: 0,
                character: 1,
            },
        );
        assert_eq!(context, Some(CompletionContext::None));
    }

    #[test]
    fn test_no_macro_snippet_inside_macro() {
        // After "image:" the user is typing a target
        let context = detect_context(
            "image:foo",
            Position {
                line: 0,
                character: 9,
            },
        );
        assert_eq!(context, Some(CompletionContext::None));
    }

    #[test]
    fn test_xref_context_beats_macro_snippet() {
        let context = detect_context(
            "<<xref",
            Position {
                line: 0,
                character: 6,
            },
        );
        assert_eq!(
            context,
            Some(CompletionContext::CrossReference {
                prefix: "xref".to_string()
            })
        );
    }

    #[test]
    fn test_complete_macro_snippets_image_at_line_start() -> Result<(), Box<dyn std::error::Error>>
    {
        let items = complete_macro_snippets(
            "ima",
            true,
            Position {
                line: 0,
                character: 3,
            },
        );
        assert_eq!(items.len(), 2);

        let inline = items
            .iter()
            .find(|i| i.label == "image:")
            .ok_or("expected image: item")?;
        assert_eq!(inline.insert_text_format, Some(InsertTextFormat::SNIPPET));
        assert_eq!(edit_text(inline), Some("image:${1:path}[${2:alt text}]"));

        let block = items
            .iter()
            .find(|i| i.label == "image::")
            .ok_or("expected image:: item")?;
        assert_eq!(block.insert_text_format, Some(InsertTextFormat::SNIPPET));
        assert_eq!(edit_text(block), Some("image::${1:path}[${2:alt text}]"));
        Ok(())
    }

    #[test]
    fn test_complete_macro_snippets_mid_line_no_block() -> Result<(), Box<dyn std::error::Error>> {
        let items = complete_macro_snippets(
            "ima",
            false,
            Position {
                line: 0,
                character: 7,
            },
        );
        assert_eq!(items.len(), 1);
        let first = items.first().ok_or("expected one item")?;
        assert_eq!(first.label, "image:");
        Ok(())
    }

    #[test]
    fn test_complete_macro_snippets_no_match() {
        let items = complete_macro_snippets(
            "xyz",
            true,
            Position {
                line: 0,
                character: 3,
            },
        );
        assert!(items.is_empty());
    }

    #[test]
    fn test_complete_macro_snippets_kbd() -> Result<(), Box<dyn std::error::Error>> {
        let items = complete_macro_snippets(
            "kb",
            false,
            Position {
                line: 0,
                character: 2,
            },
        );
        assert_eq!(items.len(), 1);
        let first = items.first().ok_or("expected one item")?;
        assert_eq!(first.label, "kbd:");
        assert_eq!(edit_text(first), Some("kbd:[${1:keys}]"));
        Ok(())
    }
}
