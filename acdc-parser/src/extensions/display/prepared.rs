use std::fmt;

use bumpalo::Bump;

use crate::{
    Caption, Document, Error, InlineMacro, InlineNode, ast::DocumentRef,
    grammar::try_walk_document_inline_nodes_mut,
};

use super::{DisplayState, GeneratedDisplay, GeneratedDisplayRef, failure, plain};

/// A converter's label decision, lowered by parser-owned constructors.
#[derive(Clone, Debug)]
pub enum DisplayLabel {
    /// Use the captured title's ordinary inline nodes.
    Title,
    /// Copy the caption's inline content, optionally trimming final whitespace.
    Caption { trim_end: bool },
    /// Literal text with ordinary output escaping.
    Text(String),
}

struct Preparation<'doc> {
    source: DocumentRef<'doc>,
    arena: Bump,
}

self_cell::self_cell! {
    struct PreparedCell<'doc> {
        owner: Preparation<'doc>,
        #[covariant]
        dependent: Document,
    }
}

/// One conversion's ordinary document. The source document is never mutated.
pub struct PreparedDocument<'doc> {
    source: DocumentRef<'doc>,
    prepared: Option<PreparedCell<'doc>>,
}

impl fmt::Debug for PreparedDocument<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedDocument")
            .field("resolved", &self.prepared.is_some())
            .finish_non_exhaustive()
    }
}

impl<'doc> PreparedDocument<'doc> {
    /// Borrow ordinary nodes for conversion or checked serialization.
    #[must_use]
    pub fn document(&self) -> DocumentRef<'_> {
        self.prepared.as_ref().map_or(self.source, |cell| {
            DocumentRef::from(cell.borrow_dependent())
        })
    }

    /// Prepare symbolic references without rerunning selection or processors.
    /// No allocation or tree copy occurs when there are no display requests.
    ///
    /// # Errors
    /// Rejects unfinalized requests or content that cannot be repaired.
    pub fn new(
        source: DocumentRef<'doc>,
        mut resolve: impl FnMut(GeneratedDisplayRef<'_>) -> DisplayLabel,
    ) -> Result<Self, Error> {
        if !source.has_deferred_displays() {
            return Ok(Self {
                source,
                prepared: None,
            });
        }
        let prepared = PreparedCell::try_new(
            Preparation {
                source,
                arena: Bump::new(),
            },
            |owner| {
                let mut document = Document::from(owner.source);
                try_walk_document_inline_nodes_mut(&mut document, &mut |node| {
                    if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = node
                        && let Some(display) = xref.generated_display.take()
                    {
                        let target = display.resolved.as_ref().ok_or_else(|| {
                            failure(&display.origin, "display target has not been finalized")
                        })?;
                        let label = if target.missing {
                            DisplayLabel::Text(format!("[{}]", display.selection.id))
                        } else {
                            resolve(GeneratedDisplayRef(&display))
                        };
                        xref.text = build_label(&owner.arena, &display, label);
                        // A present-empty label must not fall back to generic reftext.
                        if xref.text.is_empty() {
                            xref.text.push(plain("", &display.origin.location));
                        }
                    }
                    Ok::<_, Error>(())
                })?;
                document.generated_displays = DisplayState::default();
                repair(&mut document)?;
                Ok::<_, Error>(document)
            },
        )?;
        Ok(Self {
            source,
            prepared: Some(prepared),
        })
    }
}

fn build_label<'a>(
    arena: &'a Bump,
    display: &GeneratedDisplay<'a>,
    label: DisplayLabel,
) -> Vec<InlineNode<'a>> {
    match label {
        DisplayLabel::Title => {
            if display.selection.title.is_present() {
                display.selection.title.as_inlines().to_vec()
            } else {
                vec![plain(
                    arena.alloc_str(&format!("[{}]", GeneratedDisplayRef(display).target())),
                    &display.origin.location,
                )]
            }
        }
        DisplayLabel::Text(text) => vec![plain(arena.alloc_str(&text), &display.origin.location)],
        DisplayLabel::Caption { trim_end } => {
            let mut nodes = display
                .resolved
                .as_ref()
                .and_then(|target| target.caption.as_ref())
                .and_then(|caption| {
                    if let Caption::CustomContent(content) = caption {
                        Some(content.as_inlines().to_vec())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if trim_end {
                trim_caption(&mut nodes);
            }
            nodes
        }
    }
}

fn trim_caption(nodes: &mut Vec<InlineNode<'_>>) {
    while let Some(node) = nodes.last_mut() {
        let text = if let InlineNode::PlainText(text) = node {
            let trimmed = text.content.trim_end();
            text.replace_text(trimmed);
            trimmed
        } else if let InlineNode::RawText(text) = node {
            let trimmed = text.content.trim_end();
            text.replace_text(trimmed);
            trimmed
        } else {
            break;
        };
        if !text.is_empty() {
            break;
        }
        nodes.pop();
    }
}

fn repair(document: &mut Document<'_>) -> Result<(), Error> {
    let sections: std::collections::HashMap<_, _> = document
        .references
        .iter()
        .map(|(id, reference)| {
            (
                *id,
                (reference.occurrence.clone(), reference.section.clone()),
            )
        })
        .collect();
    let origin = crate::SourceLocation::at_location(None, document.location.clone());
    document
        .rebuild_footnotes()
        .map_err(|error| failure(&origin, &error.to_string()))?;
    document
        .rebuild_extension_references()
        .map_err(|error| failure(&origin, &error.to_string()))?;
    for (id, reference) in &mut document.references {
        if let Some((occurrence, section)) = sections.get(id)
            && reference.occurrence == *occurrence
        {
            reference.section.clone_from(section);
        }
    }
    Ok(())
}
