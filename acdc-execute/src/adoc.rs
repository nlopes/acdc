//! Discover executable commands from parsed `AsciiDoc` source.

use acdc_converters_core::{TraversalContext, visitor::Visitor};
use acdc_parser as acdc;

use crate::command::{
    BuildError, CommandBlock, CommandGraph, CommandGraphBuilder, CommandId, InvalidCommandId,
};

impl TryFrom<&acdc::ParseResult> for CommandGraph {
    type Error = Error;

    fn try_from(parsed: &acdc::ParseResult) -> Result<Self, Self::Error> {
        validate_source(parsed)?;
        let document = parsed.document();
        let mut traversal = TraversalContext::new(&document.attributes);
        let mut collector = CommandCollector {
            parsed,
            builder: CommandGraphBuilder::new(),
        };
        collector.visit_document(&mut traversal, document)?;
        Ok(collector.builder.build()?)
    }
}

/// A document could not provide a complete, valid command graph.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The parser could not preserve source content, structure, or requested substitutions.
    #[error("cannot execute recovered source: {source}")]
    RecoveredSource {
        /// The parser's typed recovery diagnostic.
        #[source]
        source: acdc::WarningKind,
        /// The original source position, when available.
        location: Option<Box<acdc::SourceLocation>>,
    },
    /// A command or dependency has an invalid identifier.
    #[error("command `{command}`: {source}")]
    InvalidId {
        /// The command declaring the invalid identifier or dependency.
        command: String,
        /// The identifier validation error.
        source: InvalidCommandId,
        /// Where the command is declared.
        location: Box<acdc::SourceLocation>,
    },
    /// Command dependencies could not form a valid graph.
    #[error(transparent)]
    Build(#[from] BuildError),
    /// A command block has no identifier.
    #[error("command block is missing an id")]
    MissingId {
        /// Where the block is declared.
        location: Box<acdc::SourceLocation>,
    },
    /// A marked block is not a listing or source paragraph.
    #[error("command `{id}` is not a listing or source block")]
    NotAScript {
        /// The declared identifier.
        id: String,
        /// Where the block is declared.
        location: Box<acdc::SourceLocation>,
    },
    /// A script block has no retained source text.
    #[error("command `{id}` has no retained script source")]
    MissingSource {
        /// The declared identifier.
        id: String,
        /// Where the block is declared.
        location: Box<acdc::SourceLocation>,
    },
    /// An interpreter must be a nonempty executable name or path.
    #[error("command `{id}` requires a nonempty interpreter name or path without NUL bytes")]
    InvalidInterpreter {
        /// The declared identifier.
        id: String,
        /// Where the block is declared.
        location: Box<acdc::SourceLocation>,
    },
    /// Attribute substitution referenced an unset or unknown document attribute.
    #[error("command `{id}` references missing document attribute `{name}`")]
    MissingAttribute {
        /// The command containing the reference.
        id: String,
        /// The first unresolved attribute name in the script.
        name: String,
        /// Where the command is declared.
        location: Box<acdc::SourceLocation>,
    },
}

impl Error {
    /// The original file and position associated with this error, when known.
    #[must_use]
    pub fn source_location(&self) -> Option<&acdc::SourceLocation> {
        match self {
            Self::RecoveredSource { location, .. } => location.as_deref(),
            Self::Build(error) => Some(error.source_location()),
            Self::InvalidId { location, .. }
            | Self::MissingId { location }
            | Self::NotAScript { location, .. }
            | Self::MissingSource { location, .. }
            | Self::InvalidInterpreter { location, .. }
            | Self::MissingAttribute { location, .. } => Some(location),
        }
    }

    /// A second source position involved in a duplicate identifier or cycle.
    #[must_use]
    pub fn related_location(&self) -> Option<&acdc::SourceLocation> {
        match self {
            Self::Build(error) => error.related_location(),
            Self::RecoveredSource { .. }
            | Self::InvalidId { .. }
            | Self::MissingId { .. }
            | Self::NotAScript { .. }
            | Self::MissingSource { .. }
            | Self::InvalidInterpreter { .. }
            | Self::MissingAttribute { .. } => None,
        }
    }
}

fn validate_source(parsed: &acdc::ParseResult) -> Result<(), Error> {
    if let Some(warning) = parsed.source_recovery() {
        return Err(Error::RecoveredSource {
            source: warning.kind.clone(),
            location: warning.source_location().cloned().map(Box::new),
        });
    }
    Ok(())
}

struct CommandCollector<'parsed> {
    parsed: &'parsed acdc::ParseResult,
    builder: CommandGraphBuilder,
}

impl<'doc> Visitor<'doc> for CommandCollector<'_> {
    type Error = Error;

    fn before_block(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: &'doc acdc::Block<'doc>,
    ) -> Result<(), Self::Error> {
        if let Some(metadata) = block.metadata()
            && metadata.roles.contains(&"command")
        {
            let (command, dependencies) = parse_command_block(
                block,
                metadata,
                self.parsed.source_location(block.location()),
                traversal,
            )?;
            self.builder.add(command, dependencies);
        }
        Ok(())
    }

    fn visit_inline_nodes(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _nodes: &[acdc::InlineNode<'_>],
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn parse_command_block(
    block: &acdc::Block<'_>,
    metadata: &acdc::BlockMetadata<'_>,
    location: acdc::SourceLocation,
    traversal: &TraversalContext<'_>,
) -> Result<(CommandBlock, Vec<CommandId>), Error> {
    let anchor = metadata.id.as_ref().ok_or_else(|| Error::MissingId {
        location: Box::new(location.clone()),
    })?;
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only script block variants are accepted"
    )]
    let source_text = match block {
        acdc::Block::DelimitedBlock(block)
            if matches!(block.inner, acdc::DelimitedBlockType::DelimitedListing(_)) =>
        {
            block.source_text()
        }
        acdc::Block::Paragraph(paragraph)
            if matches!(metadata.style, Some("source" | "listing")) =>
        {
            paragraph.source_text()
        }
        _ => {
            return Err(Error::NotAScript {
                id: anchor.id.to_owned(),
                location: Box::new(location),
            });
        }
    }
    .ok_or_else(|| Error::MissingSource {
        id: anchor.id.to_owned(),
        location: Box::new(location.clone()),
    })?;

    let invalid_id = |source| Error::InvalidId {
        command: anchor.id.to_owned(),
        source,
        location: Box::new(location.clone()),
    };
    let id = anchor.id.parse().map_err(invalid_id)?;
    let dependencies = metadata
        .attributes
        .get_string("deps")
        .map(|dependencies| {
            dependencies
                .split(',')
                .map(str::trim)
                .filter(|dependency| !dependency.is_empty())
                .map(str::parse)
                .collect::<Result<Vec<_>, _>>()
                .map_err(invalid_id)
        })
        .transpose()?
        .unwrap_or_default();
    let interpreter = source_interpreter(metadata).map_err(|()| Error::InvalidInterpreter {
        id: anchor.id.to_owned(),
        location: Box::new(location.clone()),
    })?;
    let description = metadata
        .attributes
        .get_string("description")
        .map(std::borrow::Cow::into_owned);
    let script = if metadata.uses_substitution(&acdc::Substitution::Attributes, acdc::VERBATIM) {
        prepare_script(source_text, traversal, anchor.id, &location)?
    } else {
        source_text.to_owned()
    };
    Ok((
        CommandBlock::new(id, script, interpreter, location).with_description(description),
        dependencies,
    ))
}

fn prepare_script(
    source: &str,
    traversal: &TraversalContext<'_>,
    id: &str,
    location: &acdc::SourceLocation,
) -> Result<String, Error> {
    let mut missing = None;
    let script = acdc::substitute_attributes(source, |name| {
        let value = traversal.get(name);
        if value.is_none() && missing.is_none() {
            missing = Some(name.to_owned());
        }
        value
    });
    if let Some(name) = missing {
        return Err(Error::MissingAttribute {
            id: id.to_owned(),
            name,
            location: Box::new(location.clone()),
        });
    }
    Ok(script.into_owned())
}

fn source_interpreter(metadata: &acdc::BlockMetadata<'_>) -> Result<Option<String>, ()> {
    let interpreter = match metadata.attributes.get("interpreter") {
        Some(acdc::AttributeValue::String(value)) => Some(value.to_string()),
        Some(_) => return Err(()),
        None if metadata.style == Some("source") => metadata
            .attributes
            .get_string("language")
            .map(std::borrow::Cow::into_owned),
        None => None,
    };
    if interpreter
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.contains('\0'))
    {
        return Err(());
    }
    Ok(interpreter)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
