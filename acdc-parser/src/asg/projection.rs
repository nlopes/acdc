mod blocks;
mod lists;

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::{
    Block, BlockMetadata, Caption, ContentAttribute, DelimitedBlockType, Document,
    DocumentAttributeAssignment, Form, Header, InlineNode, Section, SectionKind, Substitution,
    Title,
};

use super::{
    ACDC_SCHEMA_ID, Error, Profile, diagnostics, images, metadata, pointer, references, unsupported,
};

type Node = Map<String, Value>;

fn node(name: &str, kind: &str) -> Node {
    Map::from_iter([("name".into(), name.into()), ("type".into(), kind.into())])
}

pub(super) fn inlines(
    values: &[InlineNode<'_>],
    path: &str,
    profile: Profile,
) -> Result<Value, Error> {
    project_inlines(values, path, false, profile)
}

fn literal_inlines(
    values: &[InlineNode<'_>],
    path: &str,
    profile: Profile,
) -> Result<Value, Error> {
    project_inlines(values, path, true, profile)
}

fn project_inlines(
    values: &[InlineNode<'_>],
    path: &str,
    literal_content: bool,
    profile: Profile,
) -> Result<Value, Error> {
    let mut result = Vec::new();
    for value in values {
        let bracketed_email = matches!(value, InlineNode::Macro(crate::InlineMacro::Autolink(link))
            if link.bracketed && references::mailto_address(&link.url.to_string()).is_some());
        if bracketed_email {
            result.push(Value::Object(literal("text", "<")));
        }
        let at = pointer(path, &result.len().to_string());
        let projected = if let InlineNode::VerbatimText(text) = value
            && literal_content
        {
            let mut result = literal("text", text.content);
            metadata::location(&mut result, &text.location, &at)
                .map_err(|error| error.with_context("verbatim text", &text.location))?;
            Value::Object(result)
        } else {
            inline(value, &at, profile)?
        };
        result.push(projected);
        if bracketed_email {
            result.push(Value::Object(literal("text", ">")));
        }
    }
    Ok(Value::Array(result))
}

fn inline(value: &InlineNode<'_>, path: &str, profile: Profile) -> Result<Value, Error> {
    inline_value(value, path, profile)
        .map_err(|error| error.with_context(diagnostics::inline_construct(value), value.location()))
}

fn inline_value(value: &InlineNode<'_>, path: &str, profile: Profile) -> Result<Value, Error> {
    let mut result = match value {
        InlineNode::PlainText(text) => literal("text", text.content),
        InlineNode::VerbatimText(_) => {
            return Err(unsupported(
                path,
                "verbatim presentation requires a supported literal or listing block mapping",
            ));
        }
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
        InlineNode::RawText(raw) => raw_literal(raw, path)?,
        InlineNode::BoldText(span) => span_node(
            "strong",
            &span.form,
            span.role,
            span.id.is_some(),
            &span.content,
            path,
            profile,
        )?,
        InlineNode::ItalicText(span) => span_node(
            "emphasis",
            &span.form,
            span.role,
            span.id.is_some(),
            &span.content,
            path,
            profile,
        )?,
        InlineNode::MonospaceText(span) => span_node(
            "code",
            &span.form,
            span.role,
            span.id.is_some(),
            &span.content,
            path,
            profile,
        )?,
        InlineNode::HighlightText(span) => span_node(
            "mark",
            &span.form,
            span.role,
            span.id.is_some(),
            &span.content,
            path,
            profile,
        )?,
        InlineNode::Macro(crate::InlineMacro::Image(image)) if profile == Profile::Acdc => {
            images::inline(image, path, profile)?
        }
        InlineNode::Macro(value) => references::project(value, path, profile)?,
        InlineNode::SubscriptText(_)
        | InlineNode::SuperscriptText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
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
    profile: Profile,
) -> Result<Node, Error> {
    for (present, property) in [(has_id, "id"), (role.is_some(), "role")] {
        if present {
            return Err(unsupported(
                &pointer(path, property),
                "upstream inline spans have no identity or role fields",
            ));
        }
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
        inlines(content, &pointer(path, "inlines"), profile)?,
    );
    Ok(result)
}

struct Target<'a, 'doc> {
    title: Option<&'a Title<'doc>>,
    label: Option<&'a ContentAttribute<'doc>>,
    section: Option<&'a Section<'doc>>,
    caption: Option<&'a Caption<'doc>>,
}

struct Export<'a, 'doc> {
    profile: Profile,
    targets: BTreeMap<String, Target<'a, 'doc>>,
}

fn project_header(header: &Header<'_>, profile: Profile) -> Result<Value, Error> {
    if !header.title.is_present() {
        return Err(unsupported(
            "/header/title",
            "the upstream header requires a present title",
        ));
    }
    metadata::header(&header.metadata)?;
    if header.subtitle.is_some() {
        return Err(unsupported(
            "/header/subtitle",
            "the upstream header has no subtitle field",
        ));
    }
    let mut projected = Map::new();
    metadata::title(&mut projected, &header.title, "/header", profile)?;
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
    Ok(Value::Object(projected))
}

pub(super) fn document(value: &Document<'_>, profile: Profile) -> Result<Value, Error> {
    if let Some(note) = value.footnotes.first() {
        return Err(unsupported(
            "/footnotes/0",
            "footnote definitions have no supported upstream mapping",
        )
        .with_context("footnote", &note.location));
    }
    let mut result = node("document", "block");
    if profile == Profile::Acdc {
        result.insert("profile".into(), ACDC_SCHEMA_ID.into());
    }
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
                    )
                    .with_context("document attribute", &crate::Location::default()));
                }
                let mut text = String::new();
                attribute.write_text(&mut text).map_err(|error| {
                    unsupported(&pointer("/attributes", name), error.to_string())
                        .with_context("document attribute", &crate::Location::default())
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
        let projected = project_header(header, profile)
            .map_err(|error| error.with_context("header", &header.location))?;
        result.insert("header".into(), projected);
    }
    let mut export = Export {
        profile,
        targets: BTreeMap::new(),
    };
    result.insert("blocks".into(), export.blocks(&value.blocks, "/blocks")?);
    export.catalogs(value)?;
    metadata::location(&mut result, &value.location, "")?;
    Ok(Value::Object(result))
}

impl<'a, 'doc> Export<'a, 'doc> {
    fn blocks(&mut self, values: &'a [Block<'doc>], path: &str) -> Result<Value, Error> {
        self.block_body(values, path, true)
    }

    fn nonsection_blocks(&mut self, values: &'a [Block<'doc>], path: &str) -> Result<Value, Error> {
        self.block_body(values, path, false)
    }

    fn block_body(
        &mut self,
        values: &'a [Block<'doc>],
        path: &str,
        allow_sections: bool,
    ) -> Result<Value, Error> {
        let mut result = Vec::new();
        for value in values {
            let at = pointer(path, &result.len().to_string());
            if !allow_sections && matches!(value, Block::Section(_)) {
                return Err(unsupported(&at, "this block body cannot contain a section")
                    .with_context("section", value.location()));
            }
            if let Some(projected) = self.block(value, &at).map_err(|error| {
                error.with_context(diagnostics::block_construct(value), value.location())
            })? {
                result.push(projected);
            }
        }
        Ok(Value::Array(result))
    }

    fn block(&mut self, value: &'a Block<'doc>, at: &str) -> Result<Option<Value>, Error> {
        let projected = match value {
            Block::Paragraph(paragraph) => self.paragraph(paragraph, at)?,
            Block::Section(section) => self.section(section, at)?,
            Block::UnorderedList(list) => {
                let mut result = self.list(&list.items, list.marker, "unordered", at)?;
                self.block_metadata(&mut result, &list.metadata, Some(&list.title), at)?;
                result
            }
            Block::OrderedList(list) => {
                let mut result = self.list(&list.items, list.marker, "ordered", at)?;
                self.block_metadata(&mut result, &list.metadata, Some(&list.title), at)?;
                result
            }
            Block::CalloutList(list) => self.callout_list(list, at)?,
            Block::DescriptionList(list) => self.description_list(list, at)?,
            Block::Comment(comment) => return metadata::check_comment(comment, at).map(|()| None),
            Block::DelimitedBlock(block)
                if matches!(&block.inner, DelimitedBlockType::DelimitedComment(_)) =>
            {
                if block.title.is_present() {
                    return Err(unsupported(
                        &pointer(at, "title"),
                        "a comment title cannot be omitted",
                    ));
                }
                metadata::empty(&block.metadata, at)?;
                return Ok(None);
            }
            Block::DelimitedBlock(block) => self.delimited(block, at)?,
            Block::Image(image) => {
                if image.alt.is_some() && self.profile == Profile::Upstream {
                    return Err(unsupported(
                        &pointer(at, "alt"),
                        "parsed alternative content needs an explicit content-attribute mapping",
                    ));
                }
                let mut result = self.block_macro(
                    "image",
                    Some(&image.source),
                    Some(&image.title),
                    &image.metadata,
                    at,
                )?;
                images::block(&mut result, image, at, self.profile)?;
                result
            }
            Block::Audio(audio) => self.block_macro(
                "audio",
                Some(&audio.source),
                Some(&audio.title),
                &audio.metadata,
                at,
            )?,
            Block::Video(video) => self.block_macro(
                "video",
                Some(&video.source),
                Some(&video.title),
                &video.metadata,
                at,
            )?,
            Block::TableOfContents(toc) => {
                self.block_macro("toc", None, None, &toc.metadata, at)?
            }
            Block::DiscreteHeader(heading) => self.heading(heading, at)?,
            Block::PageBreak(page) => {
                let mut result = node("break", "block");
                result.insert("variant".into(), "page".into());
                self.block_metadata(&mut result, &page.metadata, Some(&page.title), at)?;
                result
            }
            Block::ThematicBreak(thematic) => {
                let mut result = node("break", "block");
                result.insert("variant".into(), "thematic".into());
                metadata::title(&mut result, &thematic.title, at, self.profile)?;
                self.identity(&mut result, &thematic.anchors, Some(&thematic.title), at)?;
                result
            }
            Block::Admonition(value) => {
                let property = if value.paragraph_form {
                    "form"
                } else {
                    "delimiter"
                };
                return Err(unsupported(
                    &pointer(at, property),
                    "the actual admonition source form needs a supported mapping",
                ));
            }
            Block::DocumentAttribute(_) => {
                return Err(unsupported(
                    at,
                    "ordered body assignments have no supported upstream mapping",
                ));
            }
        };
        let mut projected = projected;
        metadata::location(&mut projected, value.location(), at)?;
        Ok(Some(Value::Object(projected)))
    }

    fn section(&mut self, section: &'a Section<'doc>, at: &str) -> Result<Node, Error> {
        for (present, property) in [
            (section.kind != SectionKind::Normal, "kind"),
            (!section.numbering_is_disabled(), "numbering"),
        ] {
            if present {
                return Err(unsupported(
                    &pointer(at, property),
                    "section category or numbering requires a defined semantic mapping",
                ));
            }
        }
        if !section.title.is_present() {
            return Err(unsupported(
                &pointer(at, "title"),
                "the upstream section requires a present title",
            ));
        }
        let mut result = node("section", "block");
        metadata::apply(self.profile, &mut result, &section.metadata, at)?;
        result.insert("id".into(), Value::String(section.id().into_owned()));
        result.insert(
            "title".into(),
            inlines(
                section.title.as_inlines(),
                &pointer(at, "title"),
                self.profile,
            )?,
        );
        result.insert("level".into(), section.level.into());
        self.target(&section.metadata, Some(&section.title), Some(section), at)?;
        result.insert(
            "blocks".into(),
            self.blocks(&section.content, &pointer(at, "blocks"))?,
        );
        Ok(result)
    }

    fn target(
        &mut self,
        metadata: &'a BlockMetadata<'doc>,
        title: Option<&'a Title<'doc>>,
        section: Option<&'a Section<'doc>>,
        path: &str,
    ) -> Result<(), Error> {
        let id = section
            .map(|section| section.id().into_owned())
            .or_else(|| metadata::anchor(metadata).map(|anchor| anchor.id.to_string()));
        if let Some(id) = id {
            let target = Target {
                title: title.filter(|title| title.is_present()),
                label: metadata::label(metadata),
                section,
                caption: metadata.caption.as_ref(),
            };
            self.insert_target(id, target, path)?;
        }
        Ok(())
    }

    fn insert_target(
        &mut self,
        id: String,
        target: Target<'a, 'doc>,
        path: &str,
    ) -> Result<(), Error> {
        if self.targets.insert(id, target).is_some() {
            return Err(unsupported(
                &pointer(path, "id"),
                "duplicate target identity cannot preserve first-registration semantics",
            ));
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
                    .with_context("reference", &reference.location)
            })?;
            let label = target.label.filter(|content| {
                content.as_inlines().iter().any(|inline| match inline {
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
            if reference.target != *id
                || reference.title.as_ref().map(ContentAttribute::as_inlines)
                    != target.title.map(Title::as_inlines)
                || reference.xreflabel.as_ref() != label
                || reference.caption.as_ref() != target.caption
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
                )
                .with_context("reference", &reference.location));
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
                    .with_context("table of contents entry", &entry.location)
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
                ).with_context("table of contents entry", &entry.location));
            }
        }
        Ok(())
    }
}

fn raw_literal(raw: &crate::Raw<'_>, path: &str) -> Result<Node, Error> {
    if let Some(metadata) = &raw.metadata {
        if metadata.id.is_some() {
            return Err(unsupported(
                &pointer(&pointer(path, "metadata"), "id"),
                "passthrough identity is not representable in this ASG profile",
            ));
        }
        if !metadata.roles.is_empty() {
            return Err(unsupported(
                &pointer(&pointer(path, "metadata"), "roles"),
                "passthrough roles are not representable in this ASG profile",
            ));
        }
    }
    Ok(match raw.subs.as_slice() {
        [] => literal("raw", raw.content),
        [Substitution::SpecialChars] => literal("text", raw.content),
        _ => {
            return Err(unsupported(
                &pointer(path, "substitutions"),
                "remaining passthrough operations are not resolved ASG content",
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{Error, Profile, inlines};
    use crate::{CalloutRef, InlineMacro, InlineNode, Location, Pass, PassthroughKind};

    #[test]
    fn pending_comment_events_are_rejected_instead_of_discarded() {
        let mut metadata = crate::BlockMetadata::default();
        metadata.push_prefix_event(crate::Block::DocumentAttribute(
            crate::DocumentAttribute::accepted(
                "later".into(),
                crate::DocumentAttributeAssignment::Set("value".into()),
                Location::default(),
            ),
        ));
        let mut comment =
            crate::Comment::new(crate::CommentKind::Paragraph, "hidden", Location::default());
        comment.prefix_events = metadata.prefix_events;
        let mut document = crate::Document::default();
        document.set_blocks([crate::Block::Comment(comment)]);
        assert!(
            matches!(crate::asg::document((&document).into()), Err(Error::Unsupported { path, .. }) if path == "/blocks/0/prefix_events")
        );
    }

    #[test]
    fn unresolved_macro_and_callout_owners_are_rejected() {
        let pass = InlineNode::Macro(InlineMacro::Pass(Pass {
            attribute_fragments: Box::default(),
            text: Some("literal"),
            substitutions: Vec::new(),
            location: Location::default(),
            kind: PassthroughKind::Macro,
        }));
        for (node, owner) in [
            (pass, "passthrough"),
            (
                InlineNode::CalloutRef(CalloutRef::explicit(1, Location::default())),
                "callout",
            ),
        ] {
            assert!(
                matches!(inlines(&[node], "", Profile::Upstream), Err(Error::Unsupported {
                path, construct: Some(actual), ..
            }) if path == "/0" && actual == owner)
            );
        }
    }
}
