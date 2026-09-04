//! Parsing `AsciiDoc` documents into command graphs via `acdc-parser`.

use crate::command::{
    BuildError, CommandBlock, CommandGraph, CommandGraphBuilder, CommandId, InvalidCommandId,
};
use acdc_parser as acdc;

impl TryFrom<&acdc::Document<'_>> for CommandGraph {
    type Error = Error;

    fn try_from(value: &acdc::Document<'_>) -> Result<Self, Self::Error> {
        let mut builder = CommandGraphBuilder::new();
        collect_commands(&value.blocks, &mut builder)?;
        Ok(builder.build()?)
    }
}

/// Error converting a parsed [`Document`](acdc::Document) into a [`CommandGraph`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A command or one of its dependencies had an invalid id.
    #[error("invalid command id: {0}")]
    InvalidId(#[from] InvalidCommandId),
    /// Resolving the parsed commands into a graph failed.
    #[error(transparent)]
    Build(#[from] BuildError),
    /// A block carried the `command` role but declared no id.
    #[error("command block at line {line} is missing an id")]
    MissingId {
        /// Source line where the offending block begins.
        line: u32,
    },
    /// A block carried the `command` role and an id but was not a listing (script) block.
    #[error("command `{id}` at line {line} is not a listing block")]
    NotAScript {
        /// The id declared on the offending block.
        id: String,
        /// Source line where the offending block begins.
        line: u32,
    },
}

/// Walk `blocks`, registering every command block with `builder`. Recurses into every block that
/// nests other blocks — sections, admonitions, list items (ordered, unordered, callout, and
/// description), and the container delimited blocks — so a command placed under a heading, inside a
/// `NOTE`, or attached to a list step is found, not just top-level ones. Blocks that can only hold
/// inline content (paragraphs, media, tables, verbatim delimited blocks) are left alone.
pub(crate) fn collect_commands(
    blocks: &[acdc::Block<'_>],
    builder: &mut CommandGraphBuilder,
) -> Result<(), Error> {
    for block in blocks {
        if let acdc::Block::DelimitedBlock(db) = block
            && db.metadata.roles.contains(&"command")
        {
            let (command, deps) = parse_command_block(db)?;
            builder.add(command, deps);
            continue; // Listing blocks have no nested content to traverse.
        }
        for nested in child_blocks(block) {
            collect_commands(nested, builder)?;
        }
    }
    Ok(())
}

/// Every child-block slice that `block` nests, in document order, or an empty vector for leaf
/// blocks (paragraphs, media, tables, and verbatim delimited blocks such as listings) that cannot
/// contain a command. A list contributes one slice per item, since each item carries its own
/// attached blocks.
fn child_blocks<'a, 'b>(block: &'b acdc::Block<'a>) -> Vec<&'b [acdc::Block<'a>]> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Block is non_exhaustive; leaf variants added later hold no nested blocks"
    )]
    match block {
        acdc::Block::Section(section) => vec![section.content.as_slice()],
        acdc::Block::Admonition(admonition) => vec![admonition.blocks.as_slice()],
        acdc::Block::UnorderedList(list) => {
            list.items.iter().map(|i| i.blocks.as_slice()).collect()
        }
        acdc::Block::OrderedList(list) => list.items.iter().map(|i| i.blocks.as_slice()).collect(),
        acdc::Block::CalloutList(list) => list.items.iter().map(|i| i.blocks.as_slice()).collect(),
        acdc::Block::DescriptionList(list) => list
            .items
            .iter()
            .map(|i| i.description.as_slice())
            .collect(),
        acdc::Block::DelimitedBlock(db) => nested_blocks(db).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// The child blocks of a delimited block that nests other blocks, or `None` for the leaf
/// (inline-bearing) variants such as listings.
fn nested_blocks<'a, 'b>(db: &'b acdc::DelimitedBlock<'a>) -> Option<&'b [acdc::Block<'a>]> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "DelimitedBlockType is non_exhaustive; verbatim variants hold no nested blocks"
    )]
    match &db.inner {
        // `DelimitedOpen` structurally nests blocks but in practice cannot carry a command:
        // AsciiDoc allows a listing inside an open block, but the parser misparses the
        // nested `----` against the open `--`, swallowing the `[.command, id=...]` line as
        // literal paragraph text and dropping the listing. The arm is kept so open blocks
        // are at least traversed rather than silently skipped.
        acdc::DelimitedBlockType::DelimitedExample(blocks)
        | acdc::DelimitedBlockType::DelimitedOpen(blocks)
        | acdc::DelimitedBlockType::DelimitedSidebar(blocks)
        | acdc::DelimitedBlockType::DelimitedQuote(blocks) => Some(blocks),
        _ => None,
    }
}

/// Parse a [`CommandBlock`] and its declared dependencies from a delimited block already known to
/// carry the `command` role. A command must declare an id ([`Error::MissingId`]) and be a listing
/// block ([`Error::NotAScript`]); a malformed id yields [`Error::InvalidId`].
fn parse_command_block(
    db: &acdc::DelimitedBlock<'_>,
) -> Result<(CommandBlock, Vec<CommandId>), Error> {
    let meta = &db.metadata;
    let line = db.location.start.line;
    let anchor = meta.id.as_ref().ok_or(Error::MissingId { line })?;
    let acdc::DelimitedBlockType::DelimitedListing(inlines) = &db.inner else {
        return Err(Error::NotAScript {
            id: anchor.id.to_owned(),
            line,
        });
    };

    let id: CommandId = anchor.id.parse()?;
    let deps = meta
        .attributes
        .get_string("deps")
        .map(|d| {
            d.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::parse)
                .collect::<Result<_, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let interpreter = source_interpreter(meta);
    let description = meta
        .attributes
        .get_string("description")
        .map(std::borrow::Cow::into_owned);
    let script = listing_inlines_to_string(inlines);

    Ok((
        CommandBlock::new(id, script, interpreter, db.location.clone())
            .with_description(description),
        deps,
    ))
}

/// The interpreter selected by a `[source,<lang>]` block, e.g. `bash` for `[source, bash]`.
///
/// The parser stores the style (`"source"`) in `meta.style` and moves remaining positional
/// attributes into `meta.attributes` as value-less (`AttributeValue::None`) keys. For a
/// well-formed source block the language is the *only* such key; other annotating syntax
/// (options like `%linenums`, named attributes like `id=`) lands in different fields, so the
/// first — and in practice sole — `None`-valued attribute is the language.
fn source_interpreter(meta: &acdc::BlockMetadata<'_>) -> Option<String> {
    if meta.style != Some("source") {
        return None;
    }
    meta.attributes.iter().find_map(|(name, value)| {
        matches!(value, acdc::AttributeValue::None).then(|| name.as_ref().to_owned())
    })
}

/// Render listing inline content as script text.
///
/// Text nodes contribute their content verbatim and hard line breaks become newlines. Any other
/// inline node carries no script text: listing content is a verbatim context, so anchors,
/// formatting spans, and callout markers do not appear in well-formed command blocks.
fn listing_inlines_to_string(inlines: &[acdc::InlineNode<'_>]) -> String {
    let mut out = String::new();
    for node in inlines {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "InlineNode is non_exhaustive; future variants carry no script text"
        )]
        match node {
            acdc::InlineNode::PlainText(text) => out.push_str(text.content),
            acdc::InlineNode::RawText(text) => out.push_str(text.content),
            acdc::InlineNode::VerbatimText(text) => out.push_str(text.content),
            acdc::InlineNode::LineBreak(_) => out.push('\n'),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
