use crate::DocumentAttributes;

/// Check if the document has doctype=manpage.
pub(super) fn is_manpage_doctype(attrs: &DocumentAttributes<'_>) -> bool {
    matches!(
        attrs.get("doctype"),
        Some(value) if value.as_str() == Some("manpage")
    )
}

/// Check if the document has doctype=book.
pub(super) fn is_book_doctype(attrs: &DocumentAttributes<'_>) -> bool {
    matches!(
        attrs.get("doctype"),
        Some(value) if value.as_str() == Some("book")
    )
}
