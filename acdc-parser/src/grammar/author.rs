use crate::{AttributeValue, Author, DocumentAttributes, Header};

use super::{ParserState, document::document_parser};

/// Build a full name string from an `Author`.
fn build_author_full_name(author: &Author) -> String {
    let mut name = author.first_name.clone();
    if let Some(middle) = &author.middle_name {
        name.push(' ');
        name.push_str(middle);
    }
    if !author.last_name.is_empty() {
        name.push(' ');
        name.push_str(&author.last_name);
    }
    name
}

/// Bidirectional sync between `Header.authors` and document attributes.
///
/// When `:author:` is explicitly set as a document attribute, it overrides any author line.
/// When no author line is present, populates `header.authors` from `:author:` and `:email:`
/// document attributes.
///
/// Always sets the derived document attributes (`author`, `authors`, `firstname`,
/// `lastname`, `authorinitials`, `email`, `authorcount`, etc.) from whatever authors are
/// present, so `{author}` references work in the document body.
pub(crate) fn derive_author_attrs(header: &mut Header, attrs: &mut DocumentAttributes) {
    // :author: attribute always overrides the author line (asciidoctor behavior)
    if let Some(author) = attrs.get_string("author")
        && !author.is_empty()
    {
        let mut temp_state = ParserState::new(&author);
        if let Ok(mut authors) = document_parser::authors(&author, &mut temp_state) {
            // Apply :email: if present and first author has no email
            if let Some(first) = authors.first_mut()
                && first.email.is_none()
                && let Some(email) = attrs.get_string("email")
            {
                first.email = Some(email);
            }
            header.authors = authors;
        }
    }

    // Set document attributes from authors (bidirectional sync)
    if !header.authors.is_empty() {
        let all_names: Vec<String> = header.authors.iter().map(build_author_full_name).collect();
        attrs.insert(
            "authors".into(),
            AttributeValue::String(all_names.join(", ")),
        );
        attrs.insert(
            "authorcount".into(),
            AttributeValue::String(header.authors.len().to_string()),
        );

        for (i, author) in header.authors.iter().enumerate() {
            let suffix = if i == 0 {
                String::new()
            } else {
                format!("_{}", i + 1)
            };
            attrs.insert(
                format!("author{suffix}"),
                AttributeValue::String(build_author_full_name(author)),
            );
            attrs.insert(
                format!("firstname{suffix}"),
                AttributeValue::String(author.first_name.clone()),
            );
            if let Some(middle) = &author.middle_name {
                attrs.insert(
                    format!("middlename{suffix}"),
                    AttributeValue::String(middle.clone()),
                );
            }
            attrs.insert(
                format!("lastname{suffix}"),
                AttributeValue::String(author.last_name.clone()),
            );
            attrs.insert(
                format!("authorinitials{suffix}"),
                AttributeValue::String(author.initials.clone()),
            );
            if let Some(email) = &author.email {
                attrs.insert(
                    format!("email{suffix}"),
                    AttributeValue::String(email.clone()),
                );
            }
        }
    }
}
