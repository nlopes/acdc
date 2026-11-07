use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use crate::{
    Document, Footnote, FootnoteDefinition, InlineMacro, InlineNode,
    ast::{DocumentRef, FootnoteDefinitionRef, InlineRef, StructuralRef},
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
    validate_scope(
        StructuralRef::Document(DocumentRef::from(document)),
        &document.footnotes,
        "/footnotes",
    )?;
    project_definitions(&document.footnotes, "/footnotes", profile)
}

pub(super) fn validate_scope(
    owner: StructuralRef<'_>,
    definitions: &[FootnoteDefinition<'_>],
    path: &str,
) -> Result<(), Error> {
    let mut index = BTreeMap::new();
    let mut names = BTreeSet::new();
    for (ordinal, note) in definitions.iter().enumerate() {
        let at = pointer(path, &ordinal.to_string());
        if note.number == 0
            || index
                .insert(note.number, FootnoteDefinitionRef::from(note))
                .is_some()
        {
            return Err(unsupported(
                &pointer(&at, "number"),
                "registered footnote numbers must be positive and unique",
            )
            .with_context("footnote", &note.location));
        }
        if note.id.is_some_and(|id| !names.insert(id)) {
            return Err(unsupported(
                &pointer(&at, "id"),
                "registered footnote IDs must be unique within their owner",
            )
            .with_context("footnote", &note.location));
        }
    }
    let mut pending = vec![owner];
    while let Some(node) = pending.pop() {
        for field in node.content_fields() {
            visit(field.content().as_inlines(), &mut |note| {
                check_note(note, &index, path)
            })?;
        }
        pending.extend(node.children().filter(
            |child| !matches!(child, StructuralRef::TableCell(cell) if cell.owns_footnotes()),
        ));
    }
    for note in definitions {
        visit(&note.content, &mut |child| check_note(child, &index, path))?;
    }
    check_cycles(&index, path)
}

pub(super) fn project_definitions(
    definitions: &[FootnoteDefinition<'_>],
    path: &str,
    profile: Profile,
) -> Result<Vec<Value>, Error> {
    let mut result = Vec::new();
    for (ordinal, note) in definitions.iter().enumerate() {
        let at = pointer(path, &ordinal.to_string());
        if note.registration_substitutions.is_some() {
            return Err(unsupported(
                &pointer(&at, "registration_substitutions"),
                "footnote definition substitutions are not resolved",
            )
            .with_context("footnote", &note.location));
        }
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
    index: &BTreeMap<u32, FootnoteDefinitionRef<'_>>,
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
    if note.id != definition.id()
        || note
            .content
            .as_ref()
            .is_some_and(|content| content.as_slice() != definition.content().as_inlines())
    {
        return Err(unsupported(path, "footnote occurrence disagrees with its registered body; repair the document explicitly")
            .with_context("footnote", &note.location));
    }
    Ok(())
}

fn check_cycles(index: &BTreeMap<u32, FootnoteDefinitionRef<'_>>, path: &str) -> Result<(), Error> {
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
                        path,
                        "registered footnote bodies contain a reference cycle",
                    )
                    .with_context("footnote", &crate::Location::from(definition.location())));
                }
                pending.push((number, true));
                visit(definition.content().as_inlines(), &mut |note| {
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

// Captured labels link to canonical bodies but do not own historical body copies.
pub(super) fn check_projected(document: &Map<String, Value>) -> Result<(), Error> {
    let mut index = BTreeMap::new();
    super::walk::objects(document, "", &[], &mut |node, path| {
        if let Some(notes) = node.get("footnotes").and_then(Value::as_array) {
            for (ordinal, note) in notes.iter().enumerate() {
                if let Some(number) = note.get("number").and_then(Value::as_u64)
                    && index
                        .insert(
                            number,
                            note.get("id").and_then(Value::as_str).map(str::to_owned),
                        )
                        .is_some()
                {
                    return Err(link_error(
                        &pointer(
                            &pointer(&pointer(path, "footnotes"), &ordinal.to_string()),
                            "number",
                        ),
                        "registered footnote numbers must be unique across owners",
                    ));
                }
            }
        }
        Ok(())
    })?;
    super::walk::objects(document, "", &[], &mut |node, path| {
        if !is_occurrence(node) {
            return Ok(());
        }
        let number = node.get("number").and_then(Value::as_u64).ok_or_else(|| {
            link_error(
                &pointer(path, "number"),
                "footnote number must be a nonnegative integer",
            )
        })?;
        if number == 0 {
            return if node.get("form").and_then(Value::as_str) == Some("reference") {
                Ok(())
            } else {
                Err(link_error(
                    &pointer(path, "number"),
                    "an unregistered footnote cannot define a body",
                ))
            };
        }
        let id = index.get(&number).ok_or_else(|| link_error(&pointer(path, "number"), "footnote occurrence has no matching registered body; repair the document explicitly"))?;
        if id.as_deref() != node.get("id").and_then(Value::as_str) {
            return Err(link_error(
                &pointer(path, "id"),
                "footnote occurrence ID disagrees with its registered body; repair the document explicitly",
            ));
        }
        Ok(())
    })
}

pub(super) fn require_projected_document(document: &Map<String, Value>) -> Result<(), Error> {
    super::walk::objects(document, "", &[], &mut |node, path| {
        if is_occurrence(node) {
            Err(link_error(
                path,
                "footnote export requires its document registration context",
            ))
        } else {
            Ok(())
        }
    })
}

fn is_occurrence(node: &Map<String, Value>) -> bool {
    node.get("name").and_then(Value::as_str) == Some("footnote")
        && node.get("type").and_then(Value::as_str) == Some("inline")
}

fn link_error(path: &str, message: &str) -> Error {
    unsupported(path, message).with_context("footnote", &crate::Location::default())
}
