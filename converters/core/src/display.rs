//! Resolve symbolic labels into ordinary parser nodes for one conversion.

use acdc_parser::{
    Error,
    ast::{CaptionRef, DocumentRef},
    extensions::{DisplayIntent, DisplayLabel, GeneratedDisplayRef, PreparedDocument},
};

/// Create a fresh display context without changing the shared parsed document.
/// The direct path borrows the original document when no requests are active.
/// Keep the returned context alive while traversing its `document()`.
/// Captions omit trailing whitespace and retain their final period; a requested
/// caption that has no prefix uses `[id]`. Select title intent for uncaptioned
/// targets. Present-empty captions remain explicit empty labels, overriding reftext.
///
/// # Errors
/// Returns a parser error for an unfinalized request or invalid retained content.
pub fn prepare(document: DocumentRef<'_>) -> Result<PreparedDocument<'_>, Error> {
    PreparedDocument::new(document, resolve)
}

fn resolve(request: GeneratedDisplayRef<'_>) -> DisplayLabel {
    if request.intent() == DisplayIntent::Title {
        return DisplayLabel::Title;
    }
    match request.caption() {
        Some(CaptionRef::Numbered {
            label,
            number: Some(number),
            ..
        }) => DisplayLabel::Text(if label.is_empty() {
            format!("{number}.")
        } else {
            format!("{label} {number}.")
        }),
        Some(CaptionRef::Custom(text)) => DisplayLabel::Text(text.trim_end().to_string()),
        Some(CaptionRef::CustomContent(_)) => DisplayLabel::Caption { trim_end: true },
        Some(CaptionRef::Unnumbered | CaptionRef::Numbered { number: None, .. }) | None => {
            DisplayLabel::Text(format!("[{}]", request.target()))
        }
        Some(_) => DisplayLabel::Text(format!("[{}]", request.target())),
    }
}
