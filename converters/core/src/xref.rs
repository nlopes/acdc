//! Automatic cross-reference display-text resolution.

use acdc_parser::{InlineNode, Reference};

/// Display content for an automatic cross-reference.
#[derive(Clone, Debug, PartialEq)]
pub enum XrefDisplay<'a> {
    /// Render the target title through the converter's inline pipeline.
    Inlines(Vec<InlineNode<'a>>),
    /// Render a plain reference label or `[id]` fallback.
    Text(String),
}

/// Resolve an empty cross-reference's display content.
///
/// Explicit reference labels take precedence over formatted target titles.
/// Unknown and untitled targets fall back to `[id]`, matching Asciidoctor.
#[must_use]
pub fn resolve_xref<'a>(reference: Option<&Reference<'a>>, target: &str) -> XrefDisplay<'a> {
    match reference {
        Some(reference) => {
            if let Some(label) = reference.xreflabel {
                XrefDisplay::Text(label.to_string())
            } else if let Some(title) = &reference.title {
                XrefDisplay::Inlines(title.as_ref().to_vec())
            } else {
                XrefDisplay::Text(format!("[{target}]"))
            }
        }
        None => XrefDisplay::Text(format!("[{target}]")),
    }
}
