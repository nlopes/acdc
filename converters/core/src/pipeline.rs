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
//! [`acdc_parser::extensions::Registry`] exposes working sessions for callers that need checked tree edits.
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
//! Selected inline macros run after tree edits, at document preparation for the
//! selected backend. A factory runs on its first surviving invocation. Copied
//! title and catalog labels share that invocation's output. Attribute substitution
//! uses conversion overrides and the source-position attributes. Processor failures
//! precede backend output. Returning typed text leaves escaping to the backend.
//!
//! Each conversion resolves a fresh display overlay and uses fresh backend counters
//! and traversal state. Shared parsed storage is never mutated. Custom backends
//! must honor this same per-conversion state contract in [`Converter::write_to`].

use std::{
    io::{Read, Write},
    path::Path,
};

use acdc_parser::{Error, ParseResult, ast::DocumentRef};

use crate::{
    ConversionResult, Converter, Diagnostics, display,
    postprocess::{PostprocessContext, PostprocessError},
};

mod extensions;
mod registrations;
pub use extensions::{ExtensionCatalog, ExtensionSelectionError};
pub use registrations::Extensions;

/// A configured backend and the registrations explicitly active for its documents.
#[derive(Debug)]
pub struct Pipeline<C> {
    converter: C,
    extensions: Extensions,
}

impl<C> Pipeline<C> {
    /// Use a configured backend without activating any extensions.
    pub const fn new(converter: C) -> Self {
        Self {
            converter,
            extensions: Extensions {
                parser: None,
                renderers: Vec::new(),
                docinfo: Vec::new(),
                postprocessors: Vec::new(),
            },
        }
    }

    /// Activate registrations for subsequent parses. Empty registries keep the direct path.
    #[must_use]
    pub fn with_extensions(mut self, registrations: impl Into<Extensions>) -> Self {
        self.extensions = registrations.into();
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
        match self.extensions.parser_registry() {
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
        match self.extensions.parser_registry() {
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
        match self.extensions.parser_registry() {
            Some(registry) => registry.parse_from_reader(reader, options)?.finish(),
            None => acdc_parser::parse_from_reader(reader, options),
        }
    }

    fn prepare<'doc>(
        &self,
        document: DocumentRef<'doc>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<acdc_parser::extensions::PreparedDocument<'doc>, Error> {
        if let Some(registry) = self.extensions.parser_registry()
            && document.has_inline_macros()
        {
            acdc_parser::extensions::PreparedDocument::with_inline_macros(
                document,
                display::resolve,
                registry,
                self.converter.name(),
                self.converter.parser_options().document_attributes(),
                |location, message| {
                    diagnostics.emit(
                        crate::Warning::new(diagnostics.source().clone(), message).at(location),
                    );
                },
            )
        } else {
            display::prepare(document)
        }
    }

    fn has_postprocessors(&self) -> bool {
        self.extensions
            .postprocessors(self.converter.name(), self.converter.hook_target())
            .next()
            .is_some()
    }

    fn completed_text(
        &self,
        document: DocumentRef<'_>,
        source_file: Option<&Path>,
        output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<String, C::Error> {
        use std::io::{Error as IoError, ErrorKind};

        let backend = self.converter.name();
        if !self.converter.supports_text_output() {
            return Err(IoError::new(
                ErrorKind::Unsupported,
                PostprocessError::Unsupported(backend),
            )
            .into());
        }
        let prepared = self.prepare(document, diagnostics)?;
        let context = PostprocessContext {
            backend,
            document: prepared.document(),
            source_file,
            output_path,
        };
        let mut bytes = Vec::new();
        self.converter.write_to_with_extensions(
            context.document,
            &mut bytes,
            source_file,
            output_path,
            &self.extensions,
            diagnostics,
        )?;
        let mut text = String::from_utf8(bytes).map_err(|e| {
            IoError::new(
                ErrorKind::InvalidData,
                PostprocessError::InvalidText {
                    backend,
                    source: e.utf8_error(),
                },
            )
        })?;
        for processor in self
            .extensions
            .postprocessors(backend, self.converter.hook_target())
        {
            (processor.process)(&mut text, context, diagnostics)
                .map_err(|source| IoError::other(PostprocessError::Callback { backend, source }))?;
        }
        Ok(text)
    }

    /// Resolve display content and write one conversion with structured diagnostics.
    ///
    /// # Errors
    /// Returns display, conversion, or output errors.
    pub fn write_to(
        &self,
        document: DocumentRef<'_>,
        mut writer: impl Write,
        source_file: Option<&Path>,
        output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), C::Error> {
        if self.has_postprocessors() {
            let text = self.completed_text(document, source_file, output_path, diagnostics)?;
            writer.write_all(text.as_bytes())?;
            return Ok(writer.flush()?);
        }
        let prepared = self.prepare(document, diagnostics)?;
        self.converter.write_to_with_extensions(
            prepared.document(),
            writer,
            source_file,
            output_path,
            &self.extensions,
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
        if self.has_postprocessors() || self.converter.uses_render_extensions(&self.extensions) {
            return <Self as Converter<'options>>::convert(self, document, source_file);
        }
        self.convert_prepared(document, |prepared| {
            self.converter.convert(prepared, source_file)
        })
    }

    fn convert_prepared(
        &self,
        document: DocumentRef<'_>,
        convert: impl FnOnce(DocumentRef<'_>) -> Result<ConversionResult, C::Error>,
    ) -> Result<ConversionResult, C::Error> {
        let mut warnings = Vec::new();
        let prepared = self.prepare(
            document,
            &mut Diagnostics::new(&self.converter.warning_source(), &mut warnings),
        )?;
        let mut result = convert(prepared.document())?;
        if !warnings.is_empty() {
            warnings.append(&mut result.warnings);
            result.warnings = warnings;
        }
        Ok(result)
    }
}

impl<'options, C: Converter<'options>> Converter<'options> for Pipeline<C> {
    type Error = C::Error;

    fn document_attributes_defaults()
    -> &'static [(&'static str, acdc_parser::AttributeValue<'static>)] {
        C::document_attributes_defaults()
    }

    fn new(
        options: crate::Options,
        parser: acdc_parser::OptionsBuilder<'options>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::new(C::new(options, parser)?))
    }

    fn options(&self) -> &crate::Options {
        self.converter.options()
    }
    fn parser_options(&self) -> &acdc_parser::Options<'options> {
        self.converter.parser_options()
    }
    fn name(&self) -> &'static str {
        self.converter.name()
    }
    fn hook_target(&self) -> crate::BackendTarget<'_> {
        self.converter.hook_target()
    }
    fn supports_text_output(&self) -> bool {
        self.converter.supports_text_output()
    }

    fn convert_to_file(
        &self,
        doc: DocumentRef<'_>,
        source_file: Option<&Path>,
        output_path: &Path,
    ) -> Result<ConversionResult, C::Error> {
        let postprocess = self.has_postprocessors();
        if !postprocess && !self.converter.uses_render_extensions(&self.extensions) {
            return self.convert_prepared(doc, |prepared| {
                self.converter
                    .convert_to_file(prepared, source_file, output_path)
            });
        }
        crate::report_file_conversion(self, source_file, output_path, |diagnostics| {
            if postprocess {
                // Renderers may place companion files beside the final output.
                if self.converter.supports_text_output()
                    && let Some(parent) = output_path.parent()
                {
                    std::fs::create_dir_all(parent)?;
                }
                let text = self.completed_text(doc, source_file, Some(output_path), diagnostics)?;
                let mut writer = crate::create_output(output_path)?;
                writer.write_all(text.as_bytes())?;
                writer.flush()?;
                Ok(())
            } else {
                self.write_to(
                    doc,
                    crate::create_output(output_path)?,
                    source_file,
                    Some(output_path),
                    diagnostics,
                )
            }
        })
    }

    fn warning_source(&self) -> crate::WarningSource {
        self.converter.warning_source()
    }
    fn derive_output_path(
        &self,
        input: &Path,
        document: DocumentRef<'_>,
    ) -> Result<Option<std::path::PathBuf>, Self::Error> {
        self.converter.derive_output_path(input, document)
    }
    fn write_to<W: Write>(
        &self,
        document: DocumentRef<'_>,
        writer: W,
        source: Option<&Path>,
        output: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        Self::write_to(self, document, writer, source, output, diagnostics)
    }
}
