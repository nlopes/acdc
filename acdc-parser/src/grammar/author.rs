use std::borrow::Cow;

use bumpalo::Bump;

use crate::{AttributeValue, Author, DocumentAttributes, Header};

use super::{ParserState, document::document_parser};

/// Build a full name string from an `Author`.
fn build_author_full_name(author: &Author) -> String {
    let mut name = author.first_name.to_string();
    if let Some(middle) = &author.middle_name {
        name.push(' ');
        name.push_str(middle);
    }
    if !author.last_name.is_empty() {
        name.push(' ');
        name.push_str(author.last_name);
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
pub(crate) fn derive_author_attrs<'a>(
    arena: &'a Bump,
    header: &mut Header<'a>,
    attrs: &mut DocumentAttributes<'a>,
) {
    ingest_author_attribute(arena, header, attrs);
    if header.authors.is_empty() {
        return;
    }
    set_aggregate_author_attrs(header, attrs);
    set_per_author_attrs(header, attrs);
}

/// If `:author:` is set as a document attribute, parse it and overwrite
/// `header.authors`. Matches asciidoctor: the attribute wins over any
/// author line already in the header.
fn ingest_author_attribute<'a>(
    arena: &'a Bump,
    header: &mut Header<'a>,
    attrs: &DocumentAttributes<'a>,
) {
    let Some(author) = attrs.get_string("author") else {
        return;
    };
    if author.is_empty() {
        return;
    }
    // Parse the `:author:` value in a scratch arena. The returned authors
    // borrow from that arena (which drops at end-of-scope), so re-intern
    // every string into the outer arena before keeping them alongside
    // `header`.
    let scratch = Bump::new();
    let mut temp_state = ParserState::new(&author, &scratch);
    let Ok(parsed) = document_parser::authors(&author, &mut temp_state) else {
        return;
    };
    // `arena.alloc_str` returns `&mut str`; reborrow to `&str` to match the
    // `Option<&'a str>` field type.
    let mut authors: Vec<Author<'a>> = parsed
        .into_iter()
        .map(|a| Author {
            first_name: arena.alloc_str(a.first_name),
            middle_name: a.middle_name.map(|m| &*arena.alloc_str(m)),
            last_name: arena.alloc_str(a.last_name),
            initials: arena.alloc_str(a.initials),
            email: a.email.map(|e| &*arena.alloc_str(e)),
        })
        .collect();
    // Apply :email: if present and the first author has no email yet.
    if let Some(first) = authors.first_mut()
        && first.email.is_none()
        && let Some(email) = attrs.get_string("email")
    {
        first.email = Some(arena.alloc_str(&email));
    }
    header.authors = authors;
}

/// Set `authors` (comma-joined full names) and `authorcount` from the
/// current header authors.
fn set_aggregate_author_attrs<'a>(header: &Header<'a>, attrs: &mut DocumentAttributes<'a>) {
    let all_names: Vec<String> = header.authors.iter().map(build_author_full_name).collect();
    attrs.insert(
        "authors".into(),
        AttributeValue::String(all_names.join(", ").into()),
    );
    attrs.insert(
        "authorcount".into(),
        AttributeValue::String(header.authors.len().to_string().into()),
    );
}

/// Set the per-author attribute family (`author`, `firstname`, `lastname`,
/// `authorinitials`, `email`, plus `_2`-suffixed variants for subsequent
/// authors) so `{firstname_2}` references work in the document body.
fn set_per_author_attrs<'a>(header: &Header<'a>, attrs: &mut DocumentAttributes<'a>) {
    for (i, author) in header.authors.iter().enumerate() {
        let suffix = if i == 0 {
            String::new()
        } else {
            format!("_{}", i + 1)
        };
        attrs.insert(
            format!("author{suffix}").into(),
            AttributeValue::String(build_author_full_name(author).into()),
        );
        attrs.insert(
            format!("firstname{suffix}").into(),
            AttributeValue::String(Cow::Borrowed(author.first_name)),
        );
        if let Some(middle) = author.middle_name {
            attrs.insert(
                format!("middlename{suffix}").into(),
                AttributeValue::String(Cow::Borrowed(middle)),
            );
        }
        attrs.insert(
            format!("lastname{suffix}").into(),
            AttributeValue::String(Cow::Borrowed(author.last_name)),
        );
        attrs.insert(
            format!("authorinitials{suffix}").into(),
            AttributeValue::String(Cow::Borrowed(author.initials)),
        );
        if let Some(email) = author.email {
            attrs.insert(
                format!("email{suffix}").into(),
                AttributeValue::String(Cow::Borrowed(email)),
            );
        }
    }
}
