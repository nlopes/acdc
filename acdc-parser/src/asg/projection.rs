use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::{
    Block, BlockMetadata, ContentAttribute, Document, DocumentAttributeAssignment, Form,
    InlineNode, Section, SectionKind, Substitution, Title,
};

use super::{Error, metadata, pointer, unsupported};

type Node = Map<String, Value>;

fn node(name: &str, kind: &str) -> Node {
    Map::from_iter([("name".into(), name.into()), ("type".into(), kind.into())])
}

pub(super) fn inlines(values: &[InlineNode<'_>], path: &str) -> Result<Value, Error> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| inline(value, &pointer(path, &index.to_string())))
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn inline(value: &InlineNode<'_>, path: &str) -> Result<Value, Error> {
    let mut result = match value {
        InlineNode::PlainText(text) => {
            if text.content.contains('&') {
                return Err(unsupported(
                    path,
                    "ordinary text with ampersands needs character-reference classification before strict export",
                ));
            }
            literal("text", text.content)
        }
        InlineNode::VerbatimText(text) => literal("text", text.content),
        InlineNode::Symbol(symbol) => literal("text", symbol.kind.text()),
        InlineNode::CharacterReference(reference) => {
            if let crate::CharacterReferenceKind::Numeric(value) = reference.kind()
                && !matches!(value, 0x9 | 0xa | 0xd | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x0010_ffff)
            {
                return Err(unsupported(
                    path,
                    "the numeric reference does not identify an XML character",
                ));
            }
            let mut result = node("charref", "string");
            result.insert("value".into(), Value::String(reference.kind().to_string()));
            result
        }
        InlineNode::StandaloneCurvedApostrophe(_) => literal("text", "\u{2019}"),
        InlineNode::RawText(raw) => match raw.subs.as_slice() {
            [] => literal("raw", raw.content),
            [Substitution::SpecialChars] => literal("text", raw.content),
            _ => {
                return Err(unsupported(
                    &pointer(path, "substitutions"),
                    "remaining passthrough operations are not resolved ASG content",
                ));
            }
        },
        InlineNode::BoldText(span) => span_node(
            "strong",
            &span.form,
            span.role,
            span.id.is_some(),
            &span.content,
            path,
        )?,
        InlineNode::ItalicText(span) => span_node(
            "emphasis",
            &span.form,
            span.role,
            span.id.is_some(),
            &span.content,
            path,
        )?,
        InlineNode::MonospaceText(span) => span_node(
            "code",
            &span.form,
            span.role,
            span.id.is_some(),
            &span.content,
            path,
        )?,
        InlineNode::HighlightText(span) => span_node(
            "mark",
            &span.form,
            span.role,
            span.id.is_some(),
            &span.content,
            path,
        )?,
        InlineNode::SubscriptText(_)
        | InlineNode::SuperscriptText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_) => {
            return Err(unsupported(
                path,
                "this inline form has no supported upstream mapping",
            ));
        }
    };
    metadata::location(&mut result, value.location(), path)?;
    Ok(Value::Object(result))
}

fn literal(name: &str, value: &str) -> Node {
    let mut result = node(name, "string");
    result.insert("value".into(), value.into());
    result
}

fn span_node(
    name: &str,
    form: &Form,
    role: Option<&str>,
    has_id: bool,
    content: &[InlineNode<'_>],
    path: &str,
) -> Result<Node, Error> {
    if role.is_some() || has_id {
        return Err(unsupported(
            &pointer(path, "metadata"),
            "upstream inline spans have no identity or role fields",
        ));
    }
    let mut result = node("span", "inline");
    result.insert("variant".into(), name.into());
    result.insert(
        "form".into(),
        match form {
            Form::Constrained => "constrained",
            Form::Unconstrained => "unconstrained",
        }
        .into(),
    );
    result.insert(
        "inlines".into(),
        inlines(content, &pointer(path, "inlines"))?,
    );
    Ok(result)
}

struct Target<'a, 'doc> {
    title: Option<&'a Title<'doc>>,
    label: Option<&'a ContentAttribute<'doc>>,
    section: Option<&'a Section<'doc>>,
}

struct Export<'a, 'doc> {
    targets: BTreeMap<String, Target<'a, 'doc>>,
}

pub(super) fn document(value: &Document<'_>) -> Result<Value, Error> {
    if !value.footnotes.is_empty() {
        return Err(unsupported(
            "/footnotes",
            "footnote definitions have no supported upstream mapping",
        ));
    }
    let mut result = node("document", "block");
    let mut attributes = Map::new();
    let mut assignments = value.attributes.assignments().collect::<Vec<_>>();
    assignments.sort_by_key(|(name, _)| *name);
    for (name, assignment) in assignments {
        let projected = match assignment {
            DocumentAttributeAssignment::Unset => Value::Null,
            DocumentAttributeAssignment::Set(attribute) => {
                if !attribute.inline_fragments().is_empty()
                    || !attribute.passthrough_ranges().is_empty()
                {
                    return Err(unsupported(
                        &pointer("/attributes", name),
                        "attribute content protection cannot be flattened into a scalar string",
                    ));
                }
                let mut text = String::new();
                attribute.write_text(&mut text).map_err(|error| {
                    unsupported(&pointer("/attributes", name), error.to_string())
                })?;
                Value::String(text)
            }
        };
        attributes.insert(name.into(), projected);
    }
    if value.header.is_some() || !attributes.is_empty() {
        result.insert("attributes".into(), Value::Object(attributes));
    }
    if let Some(header) = &value.header {
        metadata::header(&header.metadata)?;
        if header.subtitle.is_some() {
            return Err(unsupported(
                "/header/subtitle",
                "the upstream header has no subtitle field",
            ));
        }
        let mut projected = Map::new();
        metadata::title(&mut projected, &header.title, "/header")?;
        if !header.authors.is_empty() {
            projected.insert(
                "authors".into(),
                Value::Array(
                    header
                        .authors
                        .iter()
                        .map(|author| {
                            let mut author_node = Map::from_iter([
                                ("firstname".into(), author.first_name.into()),
                                ("lastname".into(), author.last_name.into()),
                                ("initials".into(), author.initials.into()),
                            ]);
                            if let Some(middle) = author.middle_name {
                                author_node.insert("middlename".into(), middle.into());
                            }
                            if let Some(email) = author.email {
                                author_node.insert("address".into(), email.into());
                            }
                            Value::Object(author_node)
                        })
                        .collect(),
                ),
            );
        }
        metadata::location(&mut projected, &header.location, "/header")?;
        result.insert("header".into(), Value::Object(projected));
    }
    let mut export = Export {
        targets: BTreeMap::new(),
    };
    result.insert("blocks".into(), export.blocks(&value.blocks, "/blocks")?);
    export.catalogs(value)?;
    metadata::location(&mut result, &value.location, "")?;
    Ok(Value::Object(result))
}

impl<'a, 'doc> Export<'a, 'doc> {
    fn blocks(&mut self, values: &'a [Block<'doc>], path: &str) -> Result<Value, Error> {
        let mut result = Vec::new();
        for (index, value) in values.iter().enumerate() {
            let at = pointer(path, &index.to_string());
            let projected = match value {
                Block::Paragraph(paragraph) => {
                    let mut result = node("paragraph", "block");
                    metadata::apply(&mut result, &paragraph.metadata, &at)?;
                    metadata::title(&mut result, &paragraph.title, &at)?;
                    result.insert(
                        "inlines".into(),
                        inlines(&paragraph.content, &pointer(&at, "inlines"))?,
                    );
                    self.target(&paragraph.metadata, &paragraph.title, None, &at)?;
                    result
                }
                Block::Section(section) => {
                    if section.kind != SectionKind::Normal || !section.numbering_is_disabled() {
                        return Err(unsupported(
                            &at,
                            "section category or numbering requires a defined semantic mapping",
                        ));
                    }
                    let mut result = node("section", "block");
                    metadata::apply(&mut result, &section.metadata, &at)?;
                    result.insert("id".into(), Value::String(section.id().into_owned()));
                    result.insert(
                        "title".into(),
                        inlines(section.title.as_ref(), &pointer(&at, "title"))?,
                    );
                    result.insert("level".into(), section.level.into());
                    self.target(&section.metadata, &section.title, Some(section), &at)?;
                    result.insert(
                        "blocks".into(),
                        self.blocks(&section.content, &pointer(&at, "blocks"))?,
                    );
                    result
                }
                Block::Comment(_) => continue,
                Block::TableOfContents(_)
                | Block::Admonition(_)
                | Block::DiscreteHeader(_)
                | Block::DocumentAttribute(_)
                | Block::ThematicBreak(_)
                | Block::PageBreak(_)
                | Block::UnorderedList(_)
                | Block::OrderedList(_)
                | Block::CalloutList(_)
                | Block::DescriptionList(_)
                | Block::DelimitedBlock(_)
                | Block::Image(_)
                | Block::Audio(_)
                | Block::Video(_) => {
                    return Err(unsupported(
                        &at,
                        "this block form has no supported upstream mapping",
                    ));
                }
            };
            let mut projected = projected;
            metadata::location(&mut projected, value.location(), &at)?;
            result.push(Value::Object(projected));
        }
        Ok(Value::Array(result))
    }

    fn target(
        &mut self,
        metadata: &'a BlockMetadata<'doc>,
        title: &'a Title<'doc>,
        section: Option<&'a Section<'doc>>,
        path: &str,
    ) -> Result<(), Error> {
        let id = section
            .map(|section| section.id().into_owned())
            .or_else(|| metadata::anchor(metadata).map(|anchor| anchor.id.to_string()));
        if let Some(id) = id {
            let target = Target {
                title: title.is_present().then_some(title),
                label: metadata::label(metadata),
                section,
            };
            if self.targets.insert(id, target).is_some() {
                return Err(unsupported(
                    &pointer(path, "id"),
                    "duplicate target identity cannot preserve first-registration semantics",
                ));
            }
        }
        Ok(())
    }

    fn catalogs(&self, value: &Document<'_>) -> Result<(), Error> {
        let mut references = value.references.iter().collect::<Vec<_>>();
        references.sort_by_key(|(id, _)| *id);
        for (id, reference) in references {
            let at = pointer("/references", id);
            let target = self.targets.get(*id).ok_or_else(|| {
                unsupported(&at, "reference data has no supported exported owner")
            })?;
            let label = target.label.filter(|content| {
                content.iter().any(|inline| match inline {
                    InlineNode::PlainText(text) => !text.content.trim().is_empty(),
                    InlineNode::RawText(text) => !text.content.trim().is_empty(),
                    InlineNode::CharacterReference(_)
                    | InlineNode::Symbol(_)
                    | InlineNode::VerbatimText(_)
                    | InlineNode::BoldText(_)
                    | InlineNode::ItalicText(_)
                    | InlineNode::MonospaceText(_)
                    | InlineNode::HighlightText(_)
                    | InlineNode::SubscriptText(_)
                    | InlineNode::SuperscriptText(_)
                    | InlineNode::CurvedQuotationText(_)
                    | InlineNode::CurvedApostropheText(_)
                    | InlineNode::StandaloneCurvedApostrophe(_)
                    | InlineNode::LineBreak(_)
                    | InlineNode::InlineAnchor(_)
                    | InlineNode::Macro(_)
                    | InlineNode::CalloutRef(_) => true,
                })
            });
            if reference.title.as_ref() != target.title
                || reference.xreflabel.as_ref() != label
                || reference.caption.is_some()
                || reference.is_bibliography()
                || reference.has_automatic_citation()
                || reference.section_number().is_some()
                || reference.section_name()
                    != target.section.map(|section| {
                        crate::model::section::reference_name(
                            section.kind,
                            section.level,
                            value
                                .attributes
                                .get("doctype")
                                .and_then(crate::DocumentAttributeValue::as_str)
                                == Some("book"),
                        )
                    })
            {
                return Err(unsupported(
                    &at,
                    "independent reference meaning cannot be reconstructed from exported owners",
                ));
            }
        }
        for (index, entry) in value.toc_entries.iter().enumerate() {
            let at = pointer("/toc_entries", &index.to_string());
            let target = self
                .targets
                .get(entry.id)
                .and_then(|target| target.section)
                .ok_or_else(|| {
                    unsupported(
                        &at,
                        "table-of-contents data has no supported exported section",
                    )
                })?;
            if entry.title != target.title
                || entry.level != target.level
                || entry.kind != target.kind
                || entry.number().is_some()
                || entry.xreflabel.as_ref() != metadata::label(&target.metadata)
            {
                return Err(unsupported(
                    &at,
                    "independent table-of-contents meaning cannot be reconstructed from exported sections",
                ));
            }
        }
        Ok(())
    }
}
