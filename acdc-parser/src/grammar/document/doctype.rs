//! Document type checks used by the block grammar.

use crate::DocumentAttributes;

pub(super) fn is_manpage_doctype(attrs: &DocumentAttributes<'_>) -> bool {
    matches!(
        attrs.get("doctype"),
        Some(value) if value.as_str() == Some("manpage")
    )
}

pub(super) fn is_book_doctype(attrs: &DocumentAttributes<'_>) -> bool {
    matches!(
        attrs.get("doctype"),
        Some(value) if value.as_str() == Some("book")
    )
}
