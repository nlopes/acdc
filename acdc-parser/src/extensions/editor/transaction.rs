use super::super::{
    BlockBuilder, SourceContext, TargetId,
    finalization::repair_document,
    query::{TargetIndex, output_id},
    traversal::{BlockPath, highest_footnote},
};
use super::DocumentEditor;
use crate::{
    Block, BlockMetadata, Caption, Document, Error, SourceLocation, Title, Warning,
    grammar::GeneratedContext, parsed::OwnedInput,
};

pub(super) enum Edit {
    Insert(BlockBuilder),
    Replace(BlockBuilder),
    Remove,
}

impl DocumentEditor<'_> {
    pub(super) fn apply(&mut self, target: &TargetId, edit: &Edit) -> Result<TargetId, Error> {
        let path = self.path(target)?;
        let origin = self.location(target);
        let mode = self.result.mode;
        let identity = self.result.session.identity().clone();
        let index = self
            .result
            .targets
            .get_mut()
            .ok_or_else(|| failure(&origin, "query index is absent"))?;
        let (new_target, reference_warnings, construction_warnings) =
            self.result.parsed.with_document_mut(|owner, document| {
                let prepared = prepare(owner, document, &path, edit, mode, &origin, &identity)?;
                let mut candidate = snapshot(document);
                let mut candidate_index = index.clone();
                candidate_index.prepare_edit(&mut candidate);
                let outcome = transact(
                    &mut candidate,
                    &mut candidate_index,
                    target,
                    &path,
                    edit,
                    prepared,
                    &origin,
                )?;
                *document = candidate;
                *index = candidate_index;
                Ok::<_, Error>(outcome)
            })?;
        self.result.session.deferred.retain(|request| {
            index
                .entry(request.target())
                .and_then(|entry| {
                    entry
                        .path
                        .get(&self.result.parsed.document_storage().blocks)
                })
                .is_some_and(|block| matches!(block, Block::Extension(_)))
        });
        self.result
            .parsed
            .replace_reference_warnings(reference_warnings);
        self.result
            .parsed
            .append_extension_warnings(construction_warnings);
        index.reserve_warnings(&self.result.parsed);
        Ok(new_target)
    }
}

fn snapshot<'a>(document: &Document<'a>) -> Document<'a> {
    Document {
        generated_displays: document.generated_displays.clone(),
        header: document.header.clone(),
        untitled_header_location: document.untitled_header_location.clone(),
        attributes: document.attributes.clone(),
        blocks: document.blocks.clone(),
        footnotes: document.footnotes.clone(),
        toc_entries: document.toc_entries.clone(),
        references: document.references.clone(),
        location: document.location.clone(),
    }
}

struct Prepared<'a> {
    block: Block<'a>,
    kind: Option<crate::CaptionKind>,
    warnings: Vec<Warning>,
    recaption: bool,
    displays: bool,
}

fn prepare<'a>(
    owner: &'a OwnedInput,
    document: &Document<'a>,
    path: &BlockPath,
    edit: &Edit,
    mode: crate::CompatibilityMode,
    origin: &SourceLocation,
    identity: &super::super::target::DocumentIdentity,
) -> Result<Option<Prepared<'a>>, Error> {
    let (builder, replacing) = match edit {
        Edit::Insert(builder) => (builder, false),
        Edit::Replace(builder) => (builder, true),
        Edit::Remove => return Ok(None),
    };
    let original = path
        .get(&document.blocks)
        .ok_or_else(|| failure(origin, "target is absent"))?;
    if original.metadata().is_none() {
        return Err(failure(origin, "target has no block construction context"));
    }
    if replacing && matches!(original, Block::Section(_)) {
        return Err(failure(
            origin,
            "target is not a replaceable ordinary block",
        ));
    }
    if builder.id.as_deref() == Some("") {
        return Err(failure(origin, "output ID must not be empty"));
    }
    let context = original
        .metadata()
        .and_then(|metadata| metadata.source_context.as_deref())
        .cloned()
        .unwrap_or_else(|| SourceContext {
            attributes: document.attributes.clone(),
            mode,
        });
    let inline_context = GeneratedContext {
        source: &context,
        footnotes: path.footnotes(document),
        next_footnote: highest_footnote(document)
            .checked_add(1)
            .ok_or_else(|| failure(origin, "too many footnotes"))?,
    };
    if builder.source_body {
        return prepare_source(owner, original, builder, &inline_context, origin).map(Some);
    }
    let mut inherited = if replacing {
        original.metadata().cloned().unwrap_or_default()
    } else {
        BlockMetadata::default()
    };
    inherited.attribute_positions = None;
    inherited.positional_attributes.clear();
    inherited.reftext_source = None;
    inherited.prefix_events = None;
    let title = if replacing {
        original.title().cloned().unwrap_or_default()
    } else {
        Title::default()
    };
    let mut attributes = crate::grammar::prepare_generated(
        &owner.arena,
        builder,
        inherited,
        title,
        &inline_context,
        origin,
    )?;
    attributes.metadata.source_context = Some(std::sync::Arc::new(context));
    let kind = builder.caption_kind(attributes.metadata.style);
    let id = builder
        .id
        .as_deref()
        .or_else(|| attributes.metadata.id_anchor().map(|anchor| anchor.id));
    check_id(document, original, id, replacing, origin)?;
    let mut block = builder
        .clone()
        .build(
            &owner.arena,
            origin.location.clone(),
            attributes.metadata,
            attributes.title,
            |inline| inline.build(&owner.arena, document, identity, origin),
        )
        .map_err(|error| failure(origin, &error.to_string()))?;
    if let Block::Image(image) = &mut block {
        let inherited_alt = if replacing {
            if let Block::Image(image) = original {
                image.alt.clone()
            } else {
                None
            }
        } else {
            None
        };
        image.alt = if attributes.explicit_alt {
            attributes.alt
        } else {
            inherited_alt.or(attributes.alt)
        };
    }
    Ok(Some(Prepared {
        block,
        kind,
        warnings: attributes.warnings,
        recaption: true,
        displays: builder.has_displays(),
    }))
}

fn check_id(
    document: &Document<'_>,
    original: &Block<'_>,
    id: Option<&str>,
    replacing: bool,
    origin: &SourceLocation,
) -> Result<(), Error> {
    let Some(id) = id else {
        return Ok(());
    };
    if id.is_empty() {
        return Err(failure(origin, "output ID must not be empty"));
    }
    if document.references.contains_key(id) && !(replacing && output_id(original) == Some(id)) {
        return Err(failure(
            origin,
            &format!("output ID is already in use: {id}"),
        ));
    }
    Ok(())
}

fn prepare_source<'a>(
    owner: &'a OwnedInput,
    original: &Block<'a>,
    builder: &BlockBuilder,
    context: &GeneratedContext<'_, 'a>,
    origin: &SourceLocation,
) -> Result<Prepared<'a>, Error> {
    let source = match &builder.content {
        super::super::builder::Content::Paragraph(source)
        | super::super::builder::Content::Literal(source)
        | super::super::builder::Content::Listing(source) => owner.arena.alloc_str(source),
        super::super::builder::Content::Image(_)
        | super::super::builder::Content::UnorderedList(_)
        | super::super::builder::Content::ParagraphInlines(_) => {
            return Err(failure(origin, "source body is unsupported"));
        }
    };
    let mut block = original.clone();
    if let Some(metadata) = block.metadata_mut() {
        metadata.source_context = Some(std::sync::Arc::new(context.source.clone()));
    }
    let warnings =
        crate::grammar::replace_generated_source(&owner.arena, &mut block, source, context, origin)
            .map_err(|error| failure(origin, &error.to_string()))?;
    Ok(Prepared {
        block,
        kind: None,
        warnings,
        recaption: false,
        displays: false,
    })
}

type Outcome = (TargetId, Vec<Warning>, Vec<Warning>);

fn transact<'a>(
    document: &mut Document<'a>,
    index: &mut TargetIndex,
    target: &TargetId,
    path: &BlockPath,
    edit: &Edit,
    mut prepared: Option<Prepared<'a>>,
    origin: &SourceLocation,
) -> Result<Outcome, Error> {
    if prepared.as_ref().is_some_and(|prepared| prepared.displays) {
        document.generated_displays.activate();
    }
    if let Some(prepared) = &mut prepared
        && prepared.recaption
    {
        caption(&mut prepared.block, prepared.kind, index, origin)?;
    }
    let (blocks, position) = path
        .container_mut(&mut document.blocks)
        .ok_or_else(|| failure(origin, "target container is absent"))?;
    let warnings = prepared
        .as_mut()
        .map_or_else(Vec::new, |prepared| std::mem::take(&mut prepared.warnings));
    let handle = match edit {
        Edit::Insert(_) => {
            blocks.insert(
                position,
                prepared
                    .ok_or_else(|| failure(origin, "construction is absent"))?
                    .block,
            );
            index
                .insert_before(target)
                .ok_or_else(|| failure(origin, "target is absent"))?
        }
        Edit::Replace(_) => {
            *blocks
                .get_mut(position)
                .ok_or_else(|| failure(origin, "target is absent"))? = prepared
                .ok_or_else(|| failure(origin, "construction is absent"))?
                .block;
            index.replace(target);
            target.clone()
        }
        Edit::Remove => {
            blocks.remove(position);
            index.remove(target);
            target.clone()
        }
    };
    Ok((handle, repair_document(document, Some(index))?, warnings))
}

fn caption(
    block: &mut Block<'_>,
    kind: Option<crate::CaptionKind>,
    index: &mut TargetIndex,
    origin: &SourceLocation,
) -> Result<(), Error> {
    let titled = block.title().is_some();
    let Some(metadata) = block.metadata_mut() else {
        return Ok(());
    };
    let Some(kind) = kind else {
        return Ok(());
    };
    if matches!(metadata.caption, Some(Caption::Numbered { kind: previous, number, .. }) if previous == kind && (number.is_some() || !titled))
    {
        return Ok(());
    }
    let Some(context) = &metadata.source_context else {
        return Ok(());
    };
    metadata.caption = Some(Caption::resolve(metadata, &context.attributes, kind));
    if let Some(Caption::Numbered { number, .. }) = &mut metadata.caption
        && titled
    {
        *number = Some(
            index
                .next_caption(kind, kind.source_counter(&context.attributes))
                .ok_or_else(|| failure(origin, "caption counter overflow"))?,
        );
    }
    Ok(())
}

fn failure(origin: &SourceLocation, message: &str) -> Error {
    Error::Extension(Box::new(origin.clone()), message.into())
}
