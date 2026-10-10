//! Author names and header attributes.

use crate::{
    AttributeName, Author, DocumentAttributes, Header,
    grammar::{ParserState, document_parser},
};
use bumpalo::Bump;
use std::borrow::Cow;

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

/// Keep header authors and document attributes consistent.
/// An explicit `:author:` overrides the author line. A changed author refreshes
/// derived name fields but preserves fields assigned independently.
pub(super) fn derive_author_attrs<'a>(
    arena: &'a Bump,
    header: &mut Header<'a>,
    attrs: &mut DocumentAttributes<'a>,
) {
    let author_changed = ingest_author_attribute(arena, header, attrs);
    set_author_attrs(&header.authors, attrs, author_changed);
}

/// Make implicit author metadata available to subsequent header entries.
pub(super) fn register_author_attrs<'a>(
    authors: &[Author<'a>],
    attrs: &mut DocumentAttributes<'a>,
) {
    set_author_attrs(authors, attrs, false);
}

fn set_author_attrs<'a>(authors: &[Author<'a>], attrs: &mut DocumentAttributes<'a>, refresh: bool) {
    if authors.is_empty() {
        return;
    }
    let all_names: Vec<String> = authors.iter().map(build_author_full_name).collect();
    attrs.set_text("authorcount".into(), authors.len().to_string().into());
    // First registration retains existing defaults. Rebuilding an overridden
    // author refreshes derived name fields while preserving explicit fields.
    let mut assign = |name: AttributeName<'a>, value: Cow<'a, str>| {
        if refresh {
            attrs.set_derived_text(name, value);
        } else {
            attrs.insert_text(name, value);
        }
    };
    assign("authors".into(), all_names.join(", ").into());
    for (i, author) in authors.iter().enumerate() {
        let suffix = if i == 0 {
            String::new()
        } else {
            format!("_{}", i + 1)
        };
        assign(
            format!("author{suffix}").into(),
            build_author_full_name(author).into(),
        );
        assign(
            format!("firstname{suffix}").into(),
            Cow::Borrowed(author.first_name),
        );
        if let Some(middle) = author.middle_name {
            assign(format!("middlename{suffix}").into(), Cow::Borrowed(middle));
        }
        assign(
            format!("lastname{suffix}").into(),
            Cow::Borrowed(author.last_name),
        );
        assign(
            format!("authorinitials{suffix}").into(),
            Cow::Borrowed(author.initials),
        );
        if let Some(email) = author.email {
            assign(format!("email{suffix}").into(), Cow::Borrowed(email));
        }
    }
}

/// Apply the effective author and email attributes to `header.authors`.
///
/// Return whether the author names were rebuilt. An unchanged first author
/// retains the complete author list.
fn ingest_author_attribute<'a>(
    arena: &'a Bump,
    header: &mut Header<'a>,
    attrs: &DocumentAttributes<'a>,
) -> bool {
    let Some(author) = attrs.text("author").map(crate::strip_quotes) else {
        return false;
    };
    if author.is_empty() {
        return false;
    }
    // An unchanged implicit first author must not replace the complete list.
    if header
        .authors
        .first()
        .is_some_and(|first| build_author_full_name(first) == author)
    {
        if let Some(first) = header.authors.first_mut() {
            let email = attrs.text("email").map(crate::strip_quotes);
            if first.email != email {
                first.email = email.map(|value| &*arena.alloc_str(value));
            }
        }
        return false;
    }
    // Parsed names borrow from the temporary arena. Copy them to the document
    // arena before the temporary arena is dropped.
    let scratch = Bump::new();
    let mut temp_state = ParserState::new(author, &scratch);
    let Ok(parsed) = document_parser::authors(author, &mut temp_state) else {
        return false;
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

    if let Some(first) = authors.first_mut()
        && first.email.is_none()
        && let Some(email) = attrs.text("email").map(crate::strip_quotes)
    {
        first.email = Some(arena.alloc_str(email));
    }
    header.authors = authors;
    true
}
