use crate::{AttributeValue, DocumentAttributes};

/// Check if the document has doctype=manpage.
pub fn is_manpage_doctype(attrs: &DocumentAttributes) -> bool {
    attrs
        .get("doctype")
        .is_some_and(|v| matches!(v, AttributeValue::String(s) if s == "manpage"))
}

/// Check if the document has doctype=book.
pub fn is_book_doctype(attrs: &DocumentAttributes) -> bool {
    attrs
        .get("doctype")
        .is_some_and(|v| matches!(v, AttributeValue::String(s) if s == "book"))
}
