//! Attribute and inline preparation for declarative extension construction.

use std::rc::Rc;

use bumpalo::Bump;

use crate::{
    BlockMetadata, ContentAttribute, Error, FootnoteDefinition, Location, Options, SourceLocation,
    Title, Warning,
    extensions::{BlockBuilder, ExtensionBlock, SourceContext},
    grammar::{BlockAttributeMode, ParserState},
};

use super::{
    delimited::DelimitedKind,
    metadata::{self, AttributeBlockForm, ParsedAttributeList},
};

pub(crate) struct Context<'doc, 'a> {
    pub(crate) source: &'doc SourceContext<'a>,
    pub(crate) footnotes: &'doc [FootnoteDefinition<'a>],
    pub(crate) next_footnote: u32,
}

impl<'a> Context<'_, 'a> {
    pub(crate) fn state(&self, input: &'a str, arena: &'a Bump) -> Result<ParserState<'a>, Error> {
        let mut options = Options::builder()
            .with_compatibility_mode(self.source.mode)
            .build()?;
        options.document_attributes = self.source.attributes.clone();
        // Retain titles supplied by generated attribute lists, without callbacks.
        options.extensions = Some(crate::extensions::Registry::default());
        let state = ParserState::new(input, arena, Rc::new(options));
        state
            .footnote_tracker
            .borrow_mut()
            .seed(self.footnotes, self.next_footnote);
        Ok(state)
    }
}

pub(crate) struct Prepared<'a> {
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) title: Title<'a>,
    pub(crate) alt: Option<ContentAttribute<'a>>,
    pub(crate) explicit_alt: bool,
    pub(crate) warnings: Vec<Warning>,
}

impl<'a> ParserState<'a> {
    pub(crate) fn prepare_block_output(
        &self,
        builder: &BlockBuilder,
        extension: &mut ExtensionBlock<'a>,
    ) -> Result<Prepared<'a>, Error> {
        let metadata = std::mem::take(&mut extension.metadata);
        let title = std::mem::take(&mut extension.title);
        // The invocation's metadata has already been interpreted in source order.
        if builder.defaults.is_none()
            && builder.attributes.is_empty()
            && builder.unsets.is_empty()
            && !(builder.is_image() && metadata.alt_source.is_some())
        {
            return Ok(Prepared {
                metadata,
                title,
                alt: None,
                explicit_alt: false,
                warnings: Vec::new(),
            });
        }
        prepare(
            self.arena,
            builder,
            metadata,
            title,
            extension.invocation.location(),
            &extension.location,
            |input| Ok(Self::for_inline_parsing(input, self, self.inline_ctx)),
        )
    }
}

pub(crate) fn prepare<'a>(
    arena: &'a Bump,
    builder: &BlockBuilder,
    mut inherited: BlockMetadata<'a>,
    title: Title<'a>,
    origin: &SourceLocation,
    location: &Location,
    make_state: impl FnOnce(&'a str) -> Result<ParserState<'a>, Error>,
) -> Result<Prepared<'a>, Error> {
    let sources: Vec<_> = builder.defaults.iter().chain(&builder.attributes).collect();
    if sources.iter().any(|source| source.contains(['\n', '\r'])) {
        return Err(Error::Extension(
            Box::new(origin.clone()),
            "generated attribute lists must fit on one line".into(),
        ));
    }
    let input = arena.alloc_str(
        &sources
            .iter()
            .map(|source| source.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let mut state = make_state(input)?;
    let mode = if builder.is_image() {
        BlockAttributeMode::Macro
    } else {
        BlockAttributeMode::Block
    };
    let mut attributes = ParsedAttributeList::default();
    let mut offset = 0;
    if let Some(defaults) = &builder.defaults {
        let source = input.get(..defaults.len()).unwrap_or_default();
        attributes = metadata::parse_block_attribute_list(&mut state, source, 0, mode, 0)?;
        offset = defaults.len() + 1;
    }
    if title.is_present() {
        attributes.title = None;
    }
    if builder.is_image() && inherited.alt_source.is_some() {
        // An authored alt also overrides the first positional image default.
        attributes.metadata.first_source = None;
    }
    // Attribute positions and pending reftext belong to the parsed source.
    // Generated lists have their own input and positions.
    inherited.attribute_positions = None;
    inherited.positional_attributes.clear();
    inherited.reftext_source = None;
    let previous = inherited.clone();
    attributes.overlay_prepared(inherited, state.options.compatibility_mode);
    let mut explicit_alt = builder.unsets.iter().any(|name| name == "alt");
    for source in &builder.attributes {
        let source = input.get(offset..offset + source.len()).unwrap_or_default();
        let parsed = metadata::parse_block_attribute_list(&mut state, source, offset, mode, 0)?;
        explicit_alt |=
            parsed.metadata.first_source.is_some() || parsed.metadata.alt_source.is_some();
        attributes.merge(parsed, state.options.compatibility_mode);
        offset += source.len() + 1;
    }
    let explicit_title = attributes.title.is_some();
    let mut prepared = finish(&mut state, builder, attributes, title)?;
    prepared.explicit_alt = explicit_alt;
    for name in &builder.unsets {
        unset(&mut prepared, name);
    }
    prepared.warnings = state.take_inline_warnings();
    for warning in &mut prepared.warnings {
        warning.location = Some(origin.clone());
    }
    if explicit_title && let Some(source) = prepared.title.source() {
        let mut source = source.clone();
        source.location = origin.clone();
        prepared.title =
            std::mem::take(&mut prepared.title).with_source(Some(std::sync::Arc::new(source)));
    }
    remap(&mut prepared, &previous, explicit_title, location);
    Ok(prepared)
}

fn finish<'a>(
    state: &mut ParserState<'a>,
    builder: &BlockBuilder,
    mut attributes: ParsedAttributeList<'a>,
    inherited_title: Title<'a>,
) -> Result<Prepared<'a>, Error> {
    if !builder.is_image() {
        let form = if builder.is_verbatim() {
            AttributeBlockForm::Delimited(
                if builder.is_listing() {
                    DelimitedKind::Listing
                } else {
                    DelimitedKind::Literal
                },
                None,
            )
        } else {
            AttributeBlockForm::Paragraph
        };
        metadata::route_block_positions(state, &mut attributes, form);
    }
    let media = builder.is_image().then(|| attributes.metadata.clone());
    let alt = metadata::resolve_block_content_attributes(
        state,
        &mut attributes,
        media.as_ref(),
        builder.is_image(),
    )?;
    let title = attributes
        .title
        .take()
        .map_or(Ok(inherited_title), |source| {
            metadata::resolve_title(state, source, 0)
        })?;
    let mut parsed = metadata::finish_block_parsing_metadata(state, attributes, title, None)?;
    if let Some(media) = media {
        metadata::route_media_positions(
            &mut parsed.metadata,
            &media,
            state.options.compatibility_mode,
            false,
        );
        parsed.metadata.attributes.remove("alt");
    }
    parsed.metadata.first_source = None;
    parsed.metadata.alt_source = None;
    parsed.metadata.move_positional_attributes_to_attributes();
    Ok(Prepared {
        metadata: parsed.metadata,
        title: parsed.title,
        alt,
        explicit_alt: false,
        warnings: Vec::new(),
    })
}

fn unset(prepared: &mut Prepared<'_>, name: &str) {
    match name {
        "id" => {
            prepared.metadata.id = None;
            prepared.metadata.anchors.clear();
        }
        "role" | "roles" => prepared.metadata.roles = crate::TokenAssignment::default(),
        "opts" | "options" => prepared.metadata.options = crate::TokenAssignment::default(),
        "style" => prepared.metadata.style = None,
        "title" => prepared.title = Title::default(),
        "caption" => prepared.metadata.caption = None,
        "reftext" => prepared.metadata.reftext = None,
        "subs" => prepared.metadata.substitutions = None,
        "attribution" => prepared.metadata.attribution = None,
        "citetitle" => prepared.metadata.citetitle = None,
        "alt" => prepared.alt = None,
        name => {
            prepared.metadata.attributes.remove(name);
        }
    }
}

fn remap<'a>(
    prepared: &mut Prepared<'a>,
    previous: &BlockMetadata<'a>,
    explicit_title: bool,
    origin: &Location,
) {
    for anchor in prepared
        .metadata
        .id
        .iter_mut()
        .chain(&mut prepared.metadata.anchors)
    {
        if !previous
            .id
            .iter()
            .chain(&previous.anchors)
            .any(|old| old == anchor)
        {
            anchor.location = origin.clone();
        }
    }
    if explicit_title {
        for inline in prepared.title.inlines_mut() {
            crate::grammar::location_walk::walk_inline_locations_mut(inline, &mut |location| {
                *location = origin.clone();
            });
        }
    }
    for (current, old) in [
        (&mut prepared.alt, &None),
        (&mut prepared.metadata.reftext, &previous.reftext),
        (&mut prepared.metadata.attribution, &previous.attribution),
        (&mut prepared.metadata.citetitle, &previous.citetitle),
    ] {
        if current != old
            && let Some(content) = current
        {
            remap_content(content, origin);
        }
    }
    if prepared.metadata.caption != previous.caption
        && let Some(crate::Caption::CustomContent(content)) = &mut prepared.metadata.caption
    {
        remap_content(content, origin);
    }
}

fn remap_content(content: &mut ContentAttribute<'_>, origin: &Location) {
    for inline in content.inlines_mut() {
        crate::grammar::location_walk::walk_inline_locations_mut(inline, &mut |location| {
            *location = origin.clone();
        });
    }
}

pub(crate) fn replace_source<'a>(
    arena: &'a Bump,
    block: &mut crate::Block<'a>,
    source: &'a str,
    context: &Context<'_, 'a>,
    origin: &SourceLocation,
) -> Result<Vec<Warning>, Error> {
    let metadata = block.metadata().cloned().unwrap_or_default();
    let mut state = context.state(source, arena)?;
    let substitutions = metadata.substitutions.as_ref().map_or_else(
        || crate::model::substitution::SubstitutionPlan::for_mode(context.source.mode),
        |spec| {
            crate::model::substitution::SubstitutionPlan::for_block_spec(spec, context.source.mode)
        },
    );
    let parsing = crate::grammar::helpers::BlockParsingMetadata {
        metadata,
        substitutions,
        ..Default::default()
    };
    let kind = if matches!(block, crate::Block::DelimitedLiteral(_)) {
        DelimitedKind::Literal
    } else {
        DelimitedKind::Listing
    };
    let nodes = if let crate::Block::Paragraph(paragraph) = block {
        paragraph.content =
            crate::grammar::process_inlines(&mut state, &parsing, 0, source.len(), 0, source)?.0;
        paragraph.source_text = Some(source);
        &mut paragraph.content
    } else if let crate::Block::DelimitedListing(crate::Listing(delimited))
    | crate::Block::DelimitedLiteral(crate::Literal(delimited)) = block
    {
        let params = super::delimited::DelimitedParams {
            open_start: 0,
            source_text: source,
            start: 0,
            offset: 0,
            content_start: 0,
            content_end: source.len(),
            end: source.len(),
            open_delim: delimited.delimiter,
            close: None,
            content: source,
            kind,
        };
        delimited.body = super::verbatim::verbatim_inlines(
            &mut state,
            &parsing,
            &mut delimited.metadata,
            &params,
        )?;
        delimited.source_text = Some(source);
        &mut delimited.body
    } else {
        return Err(Error::Extension(
            Box::new(origin.clone()),
            "source body is unsupported".into(),
        ));
    };
    for node in nodes {
        crate::grammar::location_walk::walk_inline_locations_mut(node, &mut |location| {
            *location = origin.location.clone();
        });
    }
    let mut warnings = std::mem::take(&mut *state.warnings.borrow_mut()).into_vec();
    for warning in &mut warnings {
        warning.location = Some(origin.clone());
    }
    Ok(warnings)
}
