//! Walking a parsed document and replacing its diagram blocks.
//!
//! asciidoctor-diagram is a set of block, block-macro and inline-macro
//! extensions that Asciidoctor invokes while it parses. acdc's parser has no
//! extension registry, so the same work happens as a pass over the finished
//! AST: every `[graphviz]`-style block and every `graphviz::file[]` macro is
//! found, rendered, and replaced in place before any converter sees the
//! document.
//!
//! Two consequences follow from running after the parse rather than during it.
//! A block macro arrives as an ordinary paragraph and has to be recognised
//! from its text (see [`crate::attrlist`]), and the inline-macro form
//! (`graphviz:file[]` inside a sentence) is not supported at all, because the
//! parser has already turned it into plain text by then.

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
};

use acdc_converters_core::{Diagnostics, Warning, WarningSource};
use acdc_parser::{
    Anchor, AttributeValue, Block, BlockMetadata, Caption, CaptionKind, DelimitedBlock,
    DelimitedBlockType, Document, DocumentArena, DocumentAttributes, ElementAttributes, Image,
    InlineNode, Location, Source, SourceLocation, Table, Title, Verbatim,
};

use crate::{
    Options,
    attrlist::{self, MacroForm},
    converters::NAMES,
    error::{Error, Result},
    paths,
    render::{self, Rendered},
    source::{CommandCache, Origin, Request},
};

/// Runs the diagram pass over parsed documents.
///
/// One processor can handle several documents; it carries the cache of
/// resolved tool locations, so a second document does not re-scan `PATH`.
pub struct Processor {
    options: Options,
    commands: CommandCache,
    warning_source: WarningSource,
}

impl Processor {
    /// Build a processor.
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self {
            options,
            commands: RefCell::new(std::collections::HashMap::default()),
            warning_source: WarningSource::new("diagram"),
        }
    }

    /// The configuration this processor was built with.
    #[must_use]
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Replace every diagram block in `document` with its generated output.
    ///
    /// Blocks that fail to generate are left as they are — the diagram source
    /// stays visible in the output — and the failure is reported as a warning,
    /// which is what `:diagram-on-error: log` (the default) means. With
    /// `:diagram-on-error: abort` the first failure is returned instead.
    ///
    /// `arena` must be the one that belongs to the same parse as `document`;
    /// [`ParseResult::with_document_mut`](acdc_parser::ParseResult::with_document_mut)
    /// hands out both together.
    ///
    /// # Errors
    ///
    /// Returns the first generation failure when the document sets
    /// `:diagram-on-error: abort`.
    pub fn process<'arena>(
        &self,
        document: &mut Document<'arena>,
        arena: &'arena DocumentArena,
        warnings: &mut Vec<Warning>,
    ) -> Result<()> {
        // `attributes` and `blocks` are disjoint fields, so the attribute
        // lookups the pass needs can stay borrowed while the tree is rewritten.
        let attributes = &document.attributes;
        let abort_on_error = matches!(
            attributes
                .get("diagram-on-error")
                .and_then(|value| value.text()),
            Some("abort")
        );
        let context = Context {
            options: &self.options,
            commands: &self.commands,
            attributes,
            arena,
            abort_on_error,
            produced_image: Cell::new(false),
        };
        let mut diagnostics = Diagnostics::new(&self.warning_source, warnings);
        walk_blocks(&context, &mut document.blocks, &mut diagnostics)?;

        // Each generated image took a figure caption with no ordinal, because
        // the parser numbered the document before any of them existed. One
        // pass now settles the whole document, so a generated diagram is
        // numbered among the hand-written images in the order it appears.
        //
        // A document that produced no image is left exactly as parsed: the
        // pass should be invisible to one that holds no diagrams.
        if context.produced_image.get() {
            document.renumber_captions();
        }
        Ok(())
    }
}

/// Everything the walk needs, threaded down the tree.
struct Context<'ctx, 'arena> {
    options: &'ctx Options,
    commands: &'ctx CommandCache,
    attributes: &'ctx DocumentAttributes<'ctx>,
    arena: &'arena DocumentArena,
    abort_on_error: bool,
    /// Whether any diagram became an image, which is what makes renumbering
    /// the document's captions necessary.
    produced_image: Cell<bool>,
}

impl Context<'_, '_> {
    /// The label a figure caption carries, from `figure-caption`.
    ///
    /// Asciidoctor's own default is `Figure`; setting the attribute to nothing
    /// suppresses the label and leaves the bare ordinal.
    fn figure_label(&self) -> String {
        // A bare `:figure-caption:` carries no text; it is the "set to
        // nothing" case above and must not fall back to the default.
        self.attributes.get("figure-caption").map_or_else(
            || "Figure".to_string(),
            |value| value.text().unwrap_or_default().to_string(),
        )
    }

    /// Report a generation failure, or propagate it when the document asked
    /// for that.
    fn report(
        &self,
        error: Error,
        location: &Location,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<()> {
        if self.abort_on_error {
            return Err(error);
        }
        diagnostics.emit(
            Warning::new(diagnostics.source().clone(), error.to_string())
                .with_advice("The diagram source is left in the output unchanged.")
                .at(SourceLocation::at_location(None, location.clone())),
        );
        Ok(())
    }
}

/// Recurse through a block list, rendering any diagram it holds.
fn walk_blocks<'arena>(
    context: &Context<'_, 'arena>,
    blocks: &mut Vec<Block<'arena>>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<()> {
    for block in blocks.iter_mut() {
        walk_block(context, block, diagnostics)?;
    }
    Ok(())
}

fn walk_block<'arena>(
    context: &Context<'_, 'arena>,
    block: &mut Block<'arena>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<()> {
    match block {
        Block::Section(section) => walk_blocks(context, &mut section.content, diagnostics),
        Block::Admonition(admonition) => walk_blocks(context, &mut admonition.blocks, diagnostics),
        Block::UnorderedList(list) => {
            for item in &mut list.items {
                walk_blocks(context, &mut item.blocks, diagnostics)?;
            }
            Ok(())
        }
        Block::OrderedList(list) => {
            for item in &mut list.items {
                walk_blocks(context, &mut item.blocks, diagnostics)?;
            }
            Ok(())
        }
        Block::DescriptionList(list) => {
            for item in &mut list.items {
                walk_blocks(context, &mut item.description, diagnostics)?;
            }
            Ok(())
        }
        Block::DelimitedBlock(_) => walk_delimited(context, block, diagnostics),
        Block::Paragraph(_) => walk_paragraph(context, block, diagnostics),
        Block::CalloutList(_)
        | Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_) => Ok(()),
        // `Block` is non-exhaustive: a block type added later cannot hold a
        // diagram until this pass learns how to reach into it.
        other => {
            tracing::debug!(?other, "diagram pass does not descend into this block");
            Ok(())
        }
    }
}

fn walk_table<'arena>(
    context: &Context<'_, 'arena>,
    table: &mut Table<'arena>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<()> {
    let rows = table
        .header
        .iter_mut()
        .chain(table.rows.iter_mut())
        .chain(table.footer.iter_mut());
    for row in rows {
        for column in &mut row.columns {
            walk_blocks(context, &mut column.content, diagnostics)?;
        }
    }
    Ok(())
}

/// Handle a delimited block: render it if its style names a diagram type,
/// otherwise recurse into whatever it contains.
fn walk_delimited<'arena>(
    context: &Context<'_, 'arena>,
    block: &mut Block<'arena>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<()> {
    let Block::DelimitedBlock(delimited) = block else {
        return Ok(());
    };

    if let Some(name) = diagram_name(delimited.metadata.style) {
        return render_delimited(context, block, name, diagnostics);
    }

    match &mut delimited.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => walk_blocks(context, blocks, diagnostics),
        DelimitedBlockType::DelimitedTable(table) => walk_table(context, table, diagnostics),
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedStem(_) => Ok(()),
        other => {
            tracing::debug!(?other, "diagram pass does not descend into this block");
            Ok(())
        }
    }
}

/// Render a `[graphviz]`-style block.
fn render_delimited<'arena>(
    context: &Context<'_, 'arena>,
    block: &mut Block<'arena>,
    name: &'static str,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<()> {
    let Block::DelimitedBlock(delimited) = block else {
        return Ok(());
    };
    let location = delimited.location.clone();

    let Some(code) = verbatim_code(&delimited.inner) else {
        diagnostics.emit(
            Warning::new(
                diagnostics.source().clone(),
                format!("`{name}` needs a listing or literal block"),
            )
            .with_advice("Delimit the diagram source with `----` or `....`.")
            .at(SourceLocation::at_location(None, location)),
        );
        return Ok(());
    };

    let (attributes, options) = block_attributes(&delimited.metadata, &["target", "format"]);
    let request = Request {
        name,
        code,
        attributes,
        options,
        origin: Origin::Block,
        base_dir: context.options.base_dir().to_path_buf(),
        document: context.attributes,
        commands: context.commands,
    };

    match render::render(context.options, request) {
        Ok(rendered) => {
            apply(context, block, rendered);
            Ok(())
        }
        Err(error) => context.report(error, &location, diagnostics),
    }
}

/// Handle a paragraph that might be a `graphviz::file[]` block macro.
fn walk_paragraph<'arena>(
    context: &Context<'_, 'arena>,
    block: &mut Block<'arena>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<()> {
    // The macro text is copied out so the paragraph is no longer borrowed by
    // the time the block is replaced.
    let (line, location) = {
        let Block::Paragraph(paragraph) = block else {
            return Ok(());
        };
        let Some(line) = single_line(&paragraph.content) else {
            return Ok(());
        };
        (line.to_string(), paragraph.location.clone())
    };
    let Some(form) = attrlist::parse_macro(&line, &|name| registered(name).is_some()) else {
        return Ok(());
    };
    let Some(name) = registered(form.name) else {
        return Ok(());
    };

    let target = paths::resolve(
        std::path::Path::new(form.target),
        context.options.base_dir(),
    );
    let code = match std::fs::read_to_string(&target) {
        Ok(code) => code,
        Err(source) => {
            return context.report(
                Error::SourceFile {
                    path: target,
                    source,
                },
                &location,
                diagnostics,
            );
        }
    };

    let base_dir = target
        .parent()
        .map_or_else(|| context.options.base_dir().to_path_buf(), PathBuf::from);
    let (attributes, options) = macro_attributes(&form);

    let request = Request {
        name,
        code,
        attributes,
        options,
        origin: Origin::File(target),
        base_dir,
        document: context.attributes,
        commands: context.commands,
    };

    match render::render(context.options, request) {
        Ok(rendered) => {
            apply_to_paragraph(context, block, &form, rendered);
            Ok(())
        }
        Err(error) => context.report(error, &location, diagnostics),
    }
}

/// Replace a rendered `[graphviz]`-style block in place.
fn apply<'arena>(context: &Context<'_, 'arena>, block: &mut Block<'arena>, rendered: Rendered) {
    let Block::DelimitedBlock(delimited) = block else {
        return;
    };

    match rendered {
        Rendered::Text {
            content,
            attributes,
        } => {
            // Staying a delimited block keeps the title, id, roles and
            // delimiter the author wrote; only the body and the style change.
            delimited.metadata.style = None;
            delimited.metadata.attributes = element_attributes(attributes);
            delimited.inner = DelimitedBlockType::DelimitedLiteral(vec![verbatim(
                context.arena,
                &content,
                delimited.location.clone(),
            )]);
        }
        Rendered::Image {
            target,
            attributes,
            explicit_target,
            caption,
        } => {
            context.produced_image.set(true);
            let parts = ImageBlock {
                target,
                attributes,
                explicit_target,
                caption,
                figure_label: context.figure_label(),
                title: std::mem::take(&mut delimited.title),
                metadata: std::mem::take(&mut delimited.metadata),
                location: delimited.location.clone(),
            };
            *block = image_block(parts);
        }
    }
}

/// Replace a rendered block macro, which arrived as a paragraph.
fn apply_to_paragraph<'arena>(
    context: &Context<'_, 'arena>,
    block: &mut Block<'arena>,
    form: &MacroForm<'_>,
    rendered: Rendered,
) {
    let (title, mut metadata, location) = {
        let Block::Paragraph(paragraph) = block else {
            return;
        };
        (
            std::mem::take(&mut paragraph.title),
            std::mem::take(&mut paragraph.metadata),
            paragraph.location.clone(),
        )
    };
    apply_macro_metadata(&mut metadata, form, context.arena);

    *block = match rendered {
        Rendered::Text {
            content,
            attributes,
        } => {
            metadata.attributes = element_attributes(attributes);
            Block::DelimitedBlock(
                DelimitedBlock::new(
                    DelimitedBlockType::DelimitedLiteral(vec![verbatim(
                        context.arena,
                        &content,
                        location.clone(),
                    )]),
                    "....",
                    location,
                )
                .with_metadata(metadata)
                .with_title(title),
            )
        }
        Rendered::Image {
            target,
            attributes,
            explicit_target,
            caption,
        } => {
            context.produced_image.set(true);
            image_block(ImageBlock {
                target,
                attributes,
                explicit_target,
                caption,
                figure_label: context.figure_label(),
                title,
                metadata,
                location,
            })
        }
    };
}

/// The pieces an image block is assembled from.
struct ImageBlock<'a> {
    target: String,
    attributes: BTreeMap<String, String>,
    explicit_target: Option<String>,
    /// An explicit `caption=` prefix written on the diagram block.
    caption: Option<String>,
    /// The document's `figure-caption` label, for the caption the image takes
    /// when the block gave it none.
    figure_label: String,
    title: Title<'a>,
    metadata: BlockMetadata<'a>,
    location: Location,
}

fn image_block(parts: ImageBlock<'_>) -> Block<'_> {
    let ImageBlock {
        target,
        mut attributes,
        explicit_target,
        caption,
        figure_label,
        title,
        mut metadata,
        location,
    } = parts;

    if !attributes.contains_key("alt") {
        attributes.insert("alt".to_string(), alt_text(explicit_target.as_deref()));
    }

    // The block was a listing or a literal when the parser handed out caption
    // ordinals, so whatever caption it carries belongs to the wrong counter.
    // The node is an image now and takes a figure caption; the ordinal is left
    // for `Document::renumber_captions`, which assigns it in document order
    // once every diagram in the document has been replaced.
    metadata.caption = Some(match caption {
        Some(prefix) => Caption::Custom(Cow::Owned(prefix)),
        None => Caption::Numbered {
            kind: CaptionKind::Figure,
            label: Cow::Owned(figure_label),
            number: None,
        },
    });

    metadata.style = None;
    metadata.attributes = element_attributes(attributes);

    Block::Image(
        Image::new(Source::Path(PathBuf::from(target)), location)
            .with_title(title)
            .with_metadata(metadata),
    )
}

/// asciidoctor-diagram's alt-text fallback: the name the author gave the
/// image, read as words, and otherwise a generic label.
///
/// The checksum-derived file name a target-less block gets is deliberately not
/// used — as alt text it reads as noise — and neither is the block title,
/// which the caption already carries.
fn alt_text(explicit_target: Option<&str>) -> String {
    let Some(target) = explicit_target else {
        return "Diagram".to_string();
    };
    let stem = std::path::Path::new(target)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(target);
    stem.replace(['-', '_'], " ")
}

/// Build a verbatim inline node holding generated text.
fn verbatim<'arena>(
    arena: &'arena DocumentArena,
    content: &str,
    location: Location,
) -> InlineNode<'arena> {
    InlineNode::VerbatimText(Verbatim {
        content: arena.alloc_str(content),
        location,
    })
}

/// Turn a plain attribute map into parser attributes.
fn element_attributes<'a>(attributes: BTreeMap<String, String>) -> ElementAttributes<'a> {
    let mut element = ElementAttributes::default();
    for (name, value) in attributes {
        let value = if value.is_empty() {
            AttributeValue::Bool(true)
        } else {
            AttributeValue::String(std::borrow::Cow::Owned(value))
        };
        element.set(std::borrow::Cow::Owned(name), value);
    }
    element
}

/// Collect a block's attributes and options in the form the generators expect.
///
/// `slots` names the positional attributes for this block form: a block style
/// takes `target` then `format`, a block macro only `format`. Anything beyond
/// the named slots becomes a valueless attribute, as asciidoctor does.
fn block_attributes(
    metadata: &BlockMetadata<'_>,
    slots: &[&str],
) -> (BTreeMap<String, String>, Vec<String>) {
    let mut attributes = BTreeMap::new();

    for (name, value) in metadata.attributes.iter() {
        match value {
            AttributeValue::String(value) => {
                attributes.insert(
                    name.to_ascii_lowercase(),
                    acdc_parser::strip_quotes(value).to_string(),
                );
            }
            AttributeValue::Bool(set) => {
                if *set {
                    attributes.insert(name.to_ascii_lowercase(), "true".to_string());
                }
            }
            // A valueless entry is a positional attribute the parser folded
            // in; the loop below puts it back under its proper name.
            AttributeValue::None => {}
            other => {
                tracing::debug!(?other, "ignoring unsupported attribute value");
            }
        }
    }

    for (index, value) in metadata.positional_values().enumerate() {
        if value.is_empty() {
            continue;
        }
        match slots.get(index) {
            Some(slot) => {
                attributes
                    .entry((*slot).to_string())
                    .or_insert_with(|| value.to_string());
            }
            None => {
                attributes.insert(value.to_ascii_lowercase(), "true".to_string());
            }
        }
    }

    let options = metadata
        .options
        .iter()
        .map(|option| (*option).to_string())
        .collect();
    (attributes, options)
}

/// The same, for a block macro parsed out of a paragraph.
fn macro_attributes(form: &MacroForm<'_>) -> (BTreeMap<String, String>, Vec<String>) {
    let mut attributes = form.attributes.clone();
    for (index, value) in form.positional.iter().enumerate() {
        if value.is_empty() {
            continue;
        }
        if index == 0 {
            attributes
                .entry("format".to_string())
                .or_insert_with(|| value.clone());
        } else {
            attributes.insert(value.to_ascii_lowercase(), "true".to_string());
        }
    }
    (attributes, form.options.clone())
}

/// Carry a block macro's shorthand id and roles onto the generated node.
///
/// `BlockMetadata` stores roles and ids as borrowed strings, and a macro's
/// come from an attribute list the AST does not keep, so they are copied into
/// the document arena to acquire the right lifetime.
fn apply_macro_metadata<'arena>(
    metadata: &mut BlockMetadata<'arena>,
    form: &MacroForm<'_>,
    arena: &'arena DocumentArena,
) {
    for role in &form.roles {
        metadata.roles.push(arena.alloc_str(role));
    }
    if let Some(id) = &form.id {
        metadata.id = Some(Anchor::new(arena.alloc_str(id), Location::default()));
    }
}

/// The diagram type a block style names, if any.
fn diagram_name(style: Option<&str>) -> Option<&'static str> {
    registered(style?)
}

/// Match `name` against the registry, case-insensitively.
fn registered(name: &str) -> Option<&'static str> {
    NAMES
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(name))
        .copied()
}

/// The verbatim text of a listing or literal block.
fn verbatim_code(inner: &DelimitedBlockType<'_>) -> Option<String> {
    let nodes = match inner {
        DelimitedBlockType::DelimitedListing(nodes)
        | DelimitedBlockType::DelimitedLiteral(nodes)
        | DelimitedBlockType::DelimitedPass(nodes) => nodes,
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedExample(_)
        | DelimitedBlockType::DelimitedOpen(_)
        | DelimitedBlockType::DelimitedSidebar(_)
        | DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedQuote(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedStem(_) => return None,
        other => {
            tracing::debug!(?other, "cannot read diagram code from this block");
            return None;
        }
    };
    plain_text(nodes)
}

/// The text of a paragraph, when it is a single unformatted line.
fn single_line<'a>(nodes: &'a [InlineNode<'a>]) -> Option<&'a str> {
    match nodes {
        [InlineNode::PlainText(text)] if !text.content.contains('\n') => Some(text.content),
        _ => None,
    }
}

/// Concatenate the text of nodes that carry nothing but text.
///
/// Anything formatted means the block was not verbatim, and its source cannot
/// be reconstructed faithfully enough to feed to a diagram tool.
fn plain_text(nodes: &[InlineNode<'_>]) -> Option<String> {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::VerbatimText(text) => out.push_str(text.content),
            InlineNode::RawText(text) => out.push_str(text.content),
            InlineNode::PlainText(text) => out.push_str(text.content),
            InlineNode::LineBreak(_) => out.push('\n'),
            InlineNode::BoldText(_)
            | InlineNode::ItalicText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::SuperscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_) => return None,
            other => {
                tracing::debug!(?other, "cannot read diagram code from this inline node");
                return None;
            }
        }
    }
    Some(out)
}
