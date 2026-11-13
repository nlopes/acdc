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

use std::{cell::RefCell, collections::BTreeMap, path::PathBuf};

use acdc_converters_core::{Diagnostics, Warning, WarningSource, document_attribute_text};
use acdc_parser::{
    Anchor, AttributeValue, Block, BlockMetadata, ContentAttribute, DelimitedBlock,
    DelimitedBlockType, Document, DocumentArena, DocumentAttributes, ElementAttributes, Image,
    InlineNode, Location, Plain, Source, SourceLocation, Table, Title, Verbatim,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind, ParagraphRef},
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
        let attributes = document.take_attributes();
        let mut blocks = document.take_blocks().collect();
        let abort_on_error = matches!(
            document_attribute_text(attributes.get("diagram-on-error")),
            Some("abort")
        );
        let context = Context {
            options: &self.options,
            commands: &self.commands,
            attributes: &attributes,
            arena,
            abort_on_error,
        };
        let mut diagnostics = Diagnostics::new(&self.warning_source, warnings);
        let result = walk_blocks(&context, &mut blocks, &mut diagnostics);
        document.set_blocks(blocks);
        document.set_attributes(attributes);
        result
    }
}

/// Everything the walk needs, threaded down the tree.
struct Context<'ctx, 'arena> {
    options: &'ctx Options,
    commands: &'ctx CommandCache,
    attributes: &'ctx DocumentAttributes<'ctx>,
    arena: &'arena DocumentArena,
    abort_on_error: bool,
}

impl Context<'_, '_> {
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
        Block::Section(section) => {
            let mut blocks = section.take_blocks().collect();
            let result = walk_blocks(context, &mut blocks, diagnostics);
            section.set_blocks(blocks);
            result
        }
        Block::Admonition(admonition) => {
            let mut blocks = admonition.take_blocks().collect();
            let result = walk_blocks(context, &mut blocks, diagnostics);
            admonition.set_blocks(blocks);
            result
        }
        Block::UnorderedList(list) => {
            let mut items: Vec<_> = list.take_items().collect();
            let result = items.iter_mut().try_for_each(|item| {
                let mut blocks = item.take_blocks().collect();
                let result = walk_blocks(context, &mut blocks, diagnostics);
                item.set_blocks(blocks);
                result
            });
            list.set_items(items);
            result
        }
        Block::OrderedList(list) => {
            let mut items: Vec<_> = list.take_items().collect();
            let result = items.iter_mut().try_for_each(|item| {
                let mut blocks = item.take_blocks().collect();
                let result = walk_blocks(context, &mut blocks, diagnostics);
                item.set_blocks(blocks);
                result
            });
            list.set_items(items);
            result
        }
        Block::DescriptionList(list) => {
            let mut items: Vec<_> = list.take_items().collect();
            let result = items.iter_mut().try_for_each(|item| {
                let Some(mut definition) = item.take_definition() else {
                    return Ok(());
                };
                let mut blocks = definition.take_blocks().collect();
                let result = walk_blocks(context, &mut blocks, diagnostics);
                definition.set_blocks(blocks);
                item.set_definition(Some(definition));
                result
            });
            list.set_items(items);
            result
        }
        Block::DelimitedBlock(_) => walk_delimited(context, block, diagnostics),
        Block::Paragraph(_) => walk_paragraph(context, block, diagnostics),
        Block::CalloutList(_)
        | Block::TableOfContents(_)
        | Block::DiscreteHeading(_)
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
    let mut header = table.take_header();
    let mut rows: Vec<_> = table.take_rows().collect();
    let mut footer = table.take_footer();
    let result = header
        .iter_mut()
        .chain(rows.iter_mut())
        .chain(footer.iter_mut())
        .try_for_each(|row| {
            let mut cells: Vec<_> = row.take_cells().collect();
            let result = cells.iter_mut().try_for_each(|cell| {
                let mut blocks = cell.take_blocks().collect();
                let result = walk_blocks(context, &mut blocks, diagnostics);
                cell.set_blocks(blocks);
                result
            });
            row.set_cells(cells);
            result
        });
    table.set_header(header);
    table.set_rows(rows);
    table.set_footer(footer);
    result
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

    if let Some(name) = diagram_name(delimited.clone_metadata().style) {
        return render_delimited(context, block, name, diagnostics);
    }

    let mut body = delimited.replace_body(DelimitedBlockType::DelimitedOpen(Vec::new()));
    let result = match &mut body {
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
    };
    delimited.replace_body(body);
    result
}

/// Render a `[graphviz]`-style block.
fn render_delimited<'arena>(
    context: &Context<'_, 'arena>,
    block: &mut Block<'arena>,
    name: &'static str,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<()> {
    let location = block.location().clone();
    let Some(code) = verbatim_code(block) else {
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

    let Some(metadata) = block.clone_metadata() else {
        return Ok(());
    };
    let (attributes, options) = block_attributes(&metadata, &["target", "format"]);
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
        Ok(rendered) => apply(context, block, rendered),
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
        let Some(line) = single_line(ParagraphRef::from(&*paragraph).content()) else {
            return Ok(());
        };
        (line.to_string(), block.location().clone())
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
        Ok(rendered) => apply_to_paragraph(context, block, &form, rendered),
        Err(error) => context.report(error, &location, diagnostics),
    }
}

/// Replace a rendered `[graphviz]`-style block in place.
fn apply<'arena>(
    context: &Context<'_, 'arena>,
    block: &mut Block<'arena>,
    rendered: Rendered,
) -> Result<()> {
    let location = block.location().clone();
    let Block::DelimitedBlock(delimited) = block else {
        return Ok(());
    };

    match rendered {
        Rendered::Text {
            content,
            attributes,
        } => {
            // Staying a delimited block keeps the title, id, roles and
            // delimiter the author wrote; only the body and the style change.
            let mut metadata = delimited.clone_metadata();
            metadata.style = None;
            metadata.attributes = element_attributes(attributes);
            delimited
                .set_metadata(metadata)
                .map_err(|error| Error::config(error.to_string()))?;
            delimited.replace_body(DelimitedBlockType::DelimitedLiteral(vec![verbatim(
                context.arena,
                &content,
                location,
            )]));
        }
        Rendered::Image {
            target,
            attributes,
            explicit_target,
        } => {
            let parts = ImageBlock {
                target,
                attributes,
                explicit_target,
                title: delimited.take_title(),
                metadata: delimited.take_metadata(),
                location,
            };
            *block = image_block(context.arena, parts)?;
        }
    }
    Ok(())
}

/// Replace a rendered block macro, which arrived as a paragraph.
fn apply_to_paragraph<'arena>(
    context: &Context<'_, 'arena>,
    block: &mut Block<'arena>,
    form: &MacroForm<'_>,
    rendered: Rendered,
) -> Result<()> {
    let location = block.location().clone();
    let (title, mut metadata) = {
        let Block::Paragraph(paragraph) = block else {
            return Ok(());
        };
        (paragraph.take_title(), paragraph.take_metadata())
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
                .map_err(|error| Error::config(error.to_string()))?
                .with_title(title),
            )
        }
        Rendered::Image {
            target,
            attributes,
            explicit_target,
        } => image_block(
            context.arena,
            ImageBlock {
                target,
                attributes,
                explicit_target,
                title,
                metadata,
                location,
            },
        )?,
    };
    Ok(())
}

/// The pieces an image block is assembled from.
struct ImageBlock<'a> {
    target: String,
    attributes: BTreeMap<String, String>,
    explicit_target: Option<String>,
    title: Title<'a>,
    metadata: BlockMetadata<'a>,
    location: Location,
}

fn image_block<'arena>(
    arena: &'arena DocumentArena,
    parts: ImageBlock<'arena>,
) -> Result<Block<'arena>> {
    let ImageBlock {
        target,
        mut attributes,
        explicit_target,
        title,
        mut metadata,
        location,
    } = parts;

    let alt = attributes
        .remove("alt")
        .unwrap_or_else(|| alt_text(explicit_target.as_deref()));

    metadata.style = None;
    metadata.attributes = element_attributes(attributes);

    let mut image = Image::new(Source::Path(PathBuf::from(target)), location.clone())
        .with_title(title)
        .with_metadata(metadata)
        .map_err(|error| Error::config(error.to_string()))?;
    image.set_alt(Some(ContentAttribute::new(vec![InlineNode::PlainText(
        Plain::new(arena.alloc_str(&alt), location),
    )])));
    Ok(Block::Image(image))
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
    InlineNode::VerbatimText(Verbatim::new(arena.alloc_str(content), location))
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
                    crate::source::strip_quotes(value).to_string(),
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

    for (index, value) in metadata.positional_attributes().enumerate() {
        let Some(value) = value else { continue };
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

    let options = (&metadata.options)
        .into_iter()
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
fn verbatim_code(block: &Block<'_>) -> Option<String> {
    let content = match BlockRef::from(block).kind() {
        BlockKind::Listing(node) => node.content(),
        BlockKind::Literal(node) => node.content(),
        BlockKind::Passthrough(node) => node.content(),
        BlockKind::TableOfContents(_)
        | BlockKind::Admonition(_)
        | BlockKind::DiscreteHeading(_)
        | BlockKind::DocumentAttribute(_)
        | BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::CalloutList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::Section(_)
        | BlockKind::Paragraph(_)
        | BlockKind::Image(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::Comment(_)
        | BlockKind::DelimitedComment(_)
        | BlockKind::Example(_)
        | BlockKind::Open(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Table(_)
        | BlockKind::Quote(_)
        | BlockKind::Verse(_)
        | BlockKind::Stem(_)
        | _ => return None,
    };
    plain_text(content)
}

/// The text of a paragraph, when it is a single unformatted line.
fn single_line(content: ContentRef<'_>) -> Option<&str> {
    let mut nodes = content.inlines();
    let InlineKind::Plain(text) = nodes.next()?.kind() else {
        return None;
    };
    (nodes.next().is_none() && !text.text().contains('\n')).then_some(text.text())
}

/// Concatenate unformatted text that can be passed to a diagram tool.
fn plain_text(content: ContentRef<'_>) -> Option<String> {
    let mut out = String::new();
    for node in content.inlines() {
        match node.kind() {
            InlineKind::Verbatim(text) => out.push_str(text.text()),
            InlineKind::Raw(text) => out.push_str(text.text()),
            InlineKind::Plain(text) => out.push_str(text.text()),
            InlineKind::LineBreak(_) => out.push('\n'),
            InlineKind::Symbol(_)
            | InlineKind::CharacterReference(_)
            | InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::DoubleQuotedSpan(_)
            | InlineKind::SingleQuotedSpan(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::Anchor(_)
            | InlineKind::Callout(_)
            | InlineKind::Footnote(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::KeyboardShortcut(_)
            | InlineKind::Button(_)
            | InlineKind::MenuPath(_)
            | InlineKind::UrlMacro(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Passthrough(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests;
