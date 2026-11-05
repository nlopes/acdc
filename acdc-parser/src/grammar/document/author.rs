//! Author names and header attributes.

use crate::{
    Author, DocumentAttributes, Header, Options,
    author_attributes::{build_author_full_name, set_author_attrs},
    grammar::{ParserState, document_parser},
};
use bumpalo::Bump;
use std::rc::Rc;

/// Keep header authors and document attributes consistent.
/// An explicit `:author:` overrides the author line. A changed author refreshes
/// derived name fields but preserves fields assigned independently.
pub(super) fn derive_author_attrs<'a>(
    arena: &'a Bump,
    header: &mut Header<'a>,
    attrs: &mut DocumentAttributes<'a>,
    options: &Rc<Options<'a>>,
) {
    let author_changed = ingest_author_attribute(arena, header, attrs, options);
    set_author_attrs(&header.authors, attrs, author_changed);
}

/// Make implicit author metadata available to subsequent header entries.
pub(super) fn register_author_attrs<'a>(
    authors: &[Author<'a>],
    attrs: &mut DocumentAttributes<'a>,
) {
    set_author_attrs(authors, attrs, false);
}

/// Apply the effective author and email attributes to `header.authors`.
///
/// Return whether the author names were rebuilt. An unchanged first author
/// retains the complete author list.
fn ingest_author_attribute<'a>(
    arena: &'a Bump,
    header: &mut Header<'a>,
    attrs: &DocumentAttributes<'a>,
    options: &Rc<Options<'a>>,
) -> bool {
    let Some(author) = attrs.text("author") else {
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
            let email = attrs.text("email");
            if first.email != email {
                first.email = email.map(|value| &*arena.alloc_str(value));
            }
        }
        return false;
    }
    // Parsed names borrow from the temporary arena. Copy them to the document
    // arena before the temporary arena is dropped.
    let scratch = Bump::new();
    let mut temp_state = ParserState::new(author, &scratch, Rc::clone(options));
    let parsed = document_parser::authors(author, &mut temp_state).unwrap_or_else(|_| {
        // Attribute values can contain punctuation outside the author-line grammar.
        let mut names = author.split_whitespace();
        let first = names.next().unwrap_or_default();
        let second = names.next();
        let rest = names.collect::<Vec<_>>().join(" ");
        let (middle, last) = if rest.is_empty() {
            (None, second)
        } else {
            (second, Some(&*scratch.alloc_str(&rest)))
        };
        vec![Author::new(&scratch, first, middle, last)]
    });
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
        && let Some(email) = attrs.text("email")
    {
        first.email = Some(arena.alloc_str(email));
    }
    header.authors = authors;
    true
}
