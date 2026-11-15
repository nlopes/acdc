//! Shared processing from configured backend and explicit registrations to output.
//!
//! Construct the backend first so its attributes govern preprocessing. A pipeline
//! with no active registrations calls the ordinary parser directly. Each active
//! parse creates fresh handlers, runs ordered tree processors, and finalizes once.
//! Returned parse results retain structured parser warnings and can be converted
//! repeatedly. Every conversion prepares fresh display content without modifying
//! the shared document. Backends own their per-conversion render state.
//!
//! # Lifecycle and ownership
//!
//! [`ExtensionCatalog`] describes build availability; only selected registrations
//! execute. Selection order determines package order, followed by registration
//! order within each package. Duplicate selections are ignored. Macro callbacks
//! run once at committed source boundaries; tree processors run once after parsing.
//! No handler is shared across documents, even when registry factories are shared.
//!
//! [`Registry`] exposes working sessions for callers that need checked tree edits.
//! Borrowed queries end before editing; query again to observe repairs. Copied
//! selections stay snapshots. Document-local target handles survive insertions and
//! replacement, but removed handles never bind to another block. Normalization is
//! idempotent and never replays callbacks. Finishing consumes the working session,
//! rejects unresolved requests, and binds deferred displays to surviving targets.
//! Pipeline parse methods finish this session before returning a [`ParseResult`].
//!
//! A processor failure stops the sequence. Individual checked edits are atomic;
//! the sequence and external callback effects are not transactions. Parser warnings
//! stay on the parse result; conversion warnings stay in [`Diagnostics`] or
//! [`ConversionResult`]. The application decides how to present them.
//!
//! Each conversion resolves a fresh display overlay and uses fresh backend counters
//! and traversal state. Shared parsed storage is never mutated. Custom backends
//! must honor this same per-conversion state contract in [`Converter::write_to`].

use std::{
    io::{Read, Write},
    path::Path,
};

use acdc_parser::{Error, ParseResult, ast::DocumentRef, extensions::Registry};

use crate::{ConversionResult, Converter, Diagnostics, display};

mod extensions;
pub use extensions::{ExtensionCatalog, ExtensionSelectionError};

/// A configured backend and the registrations explicitly active for its documents.
#[derive(Debug)]
pub struct Pipeline<C> {
    converter: C,
    extensions: Option<Registry>,
}

impl<C> Pipeline<C> {
    /// Use a configured backend without activating any extensions.
    pub const fn new(converter: C) -> Self {
        Self {
            converter,
            extensions: None,
        }
    }

    /// Activate registrations for subsequent parses. Empty registries keep the direct path.
    #[must_use]
    pub fn with_extensions(mut self, registrations: Registry) -> Self {
        self.extensions = (!registrations.is_empty()).then_some(registrations);
        self
    }

    /// Read the backend configuration selected before parsing.
    #[must_use]
    pub const fn converter(&self) -> &C {
        &self.converter
    }
}

impl<'options, C: Converter<'options>> Pipeline<C> {
    /// Parse and structurally finalize a source string with fresh document-local handlers.
    ///
    /// # Errors
    /// Returns located input, processor, or finalization errors. Earlier external
    /// callback effects cannot be rolled back when a later stage fails.
    pub fn parse(&self, source: &str) -> Result<ParseResult, Error> {
        let options = self.converter.parser_options();
        match &self.extensions {
            Some(registry) => registry.parse(source, options)?.finish(),
            None => acdc_parser::parse(source, options),
        }
    }

    /// Parse and finalize a file, preserving its path, include policy, and diagnostics.
    ///
    /// # Errors
    /// Returns input, processor, or finalization errors.
    pub fn parse_file(&self, path: impl AsRef<Path>) -> Result<ParseResult, Error> {
        let options = self.converter.parser_options();
        match &self.extensions {
            Some(registry) => registry.parse_file(path, options)?.finish(),
            None => acdc_parser::parse_file(path, options),
        }
    }

    /// Parse and finalize reader input using the same stages as strings and files.
    ///
    /// # Errors
    /// Returns input, processor, or finalization errors.
    pub fn parse_from_reader(&self, reader: impl Read) -> Result<ParseResult, Error> {
        let options = self.converter.parser_options();
        match &self.extensions {
            Some(registry) => registry.parse_from_reader(reader, options)?.finish(),
            None => acdc_parser::parse_from_reader(reader, options),
        }
    }

    /// Resolve display content and write one conversion with structured diagnostics.
    ///
    /// # Errors
    /// Returns display, conversion, or output errors.
    pub fn write_to(
        &self,
        document: DocumentRef<'_>,
        writer: impl Write,
        source_file: Option<&Path>,
        output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), C::Error> {
        let prepared = display::prepare(document)?;
        self.converter.write_to(
            prepared.document(),
            writer,
            source_file,
            output_path,
            diagnostics,
        )
    }

    /// Resolve displays and use the backend's configured output destination.
    /// Parser warnings remain on the parse result; conversion warnings remain in the return value.
    ///
    /// # Errors
    /// Returns display, conversion, or output errors.
    pub fn convert(
        &self,
        document: DocumentRef<'_>,
        source_file: Option<&Path>,
    ) -> Result<ConversionResult, C::Error> {
        let prepared = display::prepare(document)?;
        self.converter.convert(prepared.document(), source_file)
    }
}
