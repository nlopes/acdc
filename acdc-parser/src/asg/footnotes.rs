use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use crate::{
    Document, Footnote, FootnoteDefinition, InlineMacro, InlineNode,
    ast::{DocumentRef, InlineRef},
};

use super::{Error, Profile, metadata, pointer, projection::inlines, unsupported};

pub(super) fn occurrence(note: &Footnote<'_>, path: &str) -> Result<Map<String, Value>, Error> {
    if note.registration_substitutions.is_some() {
        return Err(unsupported(
            &pointer(path, "registration_substitutions"),
            "footnote substitutions are not resolved",
        ));
    }
    let mut result = Map::from_iter([
        ("name".into(), "footnote".into()),
        ("type".into(), "inline".into()),
        ("number".into(), json!(note.number)),
        (
            "form".into(),
            if note.content.is_some() {
                "definition"
            } else {
                "reference"
            }
            .into(),
        ),
    ]);
    if let Some(id) = note.id {
        result.insert("id".into(), id.into());
    }
    Ok(result)
}

pub(super) fn definitions(document: &Document<'_>, profile: Profile) -> Result<Vec<Value>, Error> {
    if profile == Profile::Upstream {
        if let Some(note) = document.footnotes.first() {
            return Err(unsupported(
                "/footnotes/0",
                "footnote definitions need the named acdc profile",
            )
            .with_context("footnote", &note.location));
        }
        return Ok(Vec::new());
    }
    let mut index = BTreeMap::new();
    let mut names = BTreeSet::new();
    for (ordinal, note) in document.footnotes.iter().enumerate() {
        let at = pointer("/footnotes", &ordinal.to_string());
        if note.number == 0 || index.insert(note.number, note).is_some() {
            return Err(unsupported(
                &pointer(&at, "number"),
                "registered footnote numbers must be positive and unique",
            )
            .with_context("footnote", &note.location));
        }
        if note.id.is_some_and(|id| !names.insert(id)) {
            return Err(unsupported(
                &pointer(&at, "id"),
                "registered footnote IDs must be unique",
            )
            .with_context("footnote", &note.location));
        }
        if note.registration_substitutions.is_some() {
            return Err(unsupported(
                &pointer(&at, "registration_substitutions"),
                "footnote definition substitutions are not resolved",
            )
            .with_context("footnote", &note.location));
        }
    }
    for field in DocumentRef::from(document).content_occurrences() {
        for inline in field.content().as_inlines() {
            if let InlineNode::Macro(InlineMacro::Footnote(note)) = inline {
                check_note(note, &index, "/footnotes")?;
            }
        }
    }
    check_cycles(&index)?;
    let mut result = Vec::new();
    for (ordinal, note) in document.footnotes.iter().enumerate() {
        let at = pointer("/footnotes", &ordinal.to_string());
        visit(&note.content, &mut |child| check_note(child, &index, &at))?;
        let mut node = Map::from_iter([
            ("number".into(), json!(note.number)),
            (
                "inlines".into(),
                inlines(&note.content, &pointer(&at, "inlines"), profile)?,
            ),
        ]);
        if let Some(id) = note.id {
            node.insert("id".into(), id.into());
        }
        metadata::location(&mut node, &note.location, &at)
            .map_err(|error| error.with_context("footnote", &note.location))?;
        result.push(Value::Object(node));
    }
    Ok(result)
}

fn check_note(
    note: &Footnote<'_>,
    index: &BTreeMap<u32, &FootnoteDefinition<'_>>,
    path: &str,
) -> Result<(), Error> {
    if note.number == 0 && note.content.is_none() {
        return Ok(());
    }
    let definition = index.get(&note.number).ok_or_else(|| {
        unsupported(
            path,
            "footnote occurrence has no matching registered body; repair the document explicitly",
        )
        .with_context("footnote", &note.location)
    })?;
    if note.id != definition.id
        || note
            .content
            .as_ref()
            .is_some_and(|content| content != &definition.content)
    {
        return Err(unsupported(path, "footnote occurrence disagrees with its registered body; repair the document explicitly")
            .with_context("footnote", &note.location));
    }
    Ok(())
}

fn check_cycles(index: &BTreeMap<u32, &FootnoteDefinition<'_>>) -> Result<(), Error> {
    let mut complete = BTreeSet::new();
    for &root in index.keys() {
        let mut active = BTreeSet::new();
        let mut pending = vec![(root, false)];
        while let Some((number, leaving)) = pending.pop() {
            if leaving {
                active.remove(&number);
                complete.insert(number);
            } else if !complete.contains(&number) {
                let Some(definition) = index.get(&number) else {
                    continue;
                };
                if !active.insert(number) {
                    return Err(unsupported(
                        "/footnotes",
                        "registered footnote bodies contain a reference cycle",
                    )
                    .with_context("footnote", &definition.location));
                }
                pending.push((number, true));
                visit(&definition.content, &mut |note| {
                    if note.number != 0 {
                        pending.push((note.number, false));
                    }
                    Ok(())
                })?;
            }
        }
    }
    Ok(())
}

pub(super) fn require_document(values: &[InlineNode<'_>]) -> Result<(), Error> {
    visit(values, &mut |note| {
        Err(unsupported(
            "",
            "footnote export requires its document registration context",
        )
        .with_context("footnote", &note.location))
    })
}

fn visit<'a>(
    values: &'a [InlineNode<'a>],
    visitor: &mut impl FnMut(&Footnote<'a>) -> Result<(), Error>,
) -> Result<(), Error> {
    for inline in values {
        if let InlineNode::Macro(InlineMacro::Footnote(note)) = inline {
            visitor(note)?;
        }
        for field in InlineRef::from(inline).content_occurrences() {
            for child in field.content().as_inlines() {
                if let InlineNode::Macro(InlineMacro::Footnote(note)) = child {
                    visitor(note)?;
                }
            }
        }
    }
    Ok(())
}
