//! Native block rendering with fresh processors for each conversion.

use std::{fmt, io::Write, sync::Arc};

use acdc_parser::ast::BlockRef;

use crate::{Diagnostics, Options, TraversalContext};

/// A native renderer or host-service failure, preserving its original source.
pub type RenderError = Box<dyn std::error::Error + Send + Sync>;

/// A backend-specific renderer factory. Constructing it executes no processor.
#[derive(Clone)]
pub struct BlockRenderer {
    pub(crate) backend: &'static str,
    pub(crate) factory: Arc<dyn Fn() -> Box<dyn BlockRenderProcessor> + Send + Sync>,
}

impl fmt::Debug for BlockRenderer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockRenderer")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl BlockRenderer {
    /// Register a factory for a backend family such as `html`.
    /// Processors may contain non-`Send` state; factories must be shareable.
    pub fn new<P: BlockRenderProcessor + 'static>(
        backend: &'static str,
        factory: impl Fn() -> P + Send + Sync + 'static,
    ) -> Self {
        Self {
            backend,
            factory: Arc::new(move || Box::new(factory())),
        }
    }
}

/// A mutable processor belonging to one conversion, including its nested blocks.
pub trait BlockRenderProcessor {
    /// Render a block or decline it so the next renderer or backend can handle it.
    /// A declined block must not write output; structured fallback warnings are allowed.
    ///
    /// # Errors
    /// Stops conversion on processor, host, or output failure. Earlier writes and
    /// external effects cannot be rolled back.
    fn render(
        &mut self,
        block: BlockRef<'_>,
        host: &mut dyn RenderHost,
    ) -> Result<bool, RenderError>;

    /// Inline script hashes required by this selected renderer, for the host's CSP.
    fn script_hashes(&self) -> &'static [&'static str] {
        &[]
    }
}

/// Backend services for a native renderer. Titles and metadata retain host semantics.
pub trait RenderHost {
    /// Current backend variant, such as `html5` or `html5s`.
    fn variant(&self) -> &str;
    /// Attributes in effect at this block, including body and table-cell assignments.
    fn document_attributes(&self) -> &TraversalContext<'_>;
    /// Validated application options, including safe mode.
    fn options(&self) -> &Options;
    /// Open a backend element with the block's ID, roles, and default classes.
    /// Native renderers supply trusted target markup; the host escapes document metadata.
    ///
    /// # Errors
    /// Returns output or metadata errors.
    fn open_block(
        &mut self,
        block: BlockRef<'_>,
        element: &str,
        classes: &str,
    ) -> Result<(), RenderError>;
    /// Render the block's caption and title between target-specific wrappers.
    ///
    /// # Errors
    /// Returns title-rendering or output errors.
    fn title(&mut self, block: BlockRef<'_>, prefix: &str, suffix: &str)
    -> Result<(), RenderError>;
    /// Borrow output, structured diagnostics, and the current traversal together.
    fn output(&mut self) -> (&mut dyn Write, Diagnostics<'_>, &TraversalContext<'_>);
    /// Write trusted backend output.
    ///
    /// # Errors
    /// Returns output errors.
    fn write(&mut self, bytes: &[u8]) -> Result<(), RenderError> {
        let (writer, _, _) = self.output();
        Ok(writer.write_all(bytes)?)
    }
}

/// Selected processors instantiated once for a conversion. Dispatch follows registration order.
pub struct RenderSession {
    pub(crate) processors: Vec<Box<dyn BlockRenderProcessor>>,
}

impl fmt::Debug for RenderSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RenderSession")
            .field("processors", &self.processors.len())
            .finish()
    }
}

impl RenderSession {
    /// Dispatch until a processor handles the block, otherwise use ordinary rendering.
    ///
    /// # Errors
    /// Stops at the first processor failure without retrying it.
    pub fn render(
        &mut self,
        block: BlockRef<'_>,
        host: &mut dyn RenderHost,
    ) -> Result<bool, RenderError> {
        for processor in &mut self.processors {
            if processor.render(block, host)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// CSP hashes belonging to selected processors for this backend.
    pub fn script_hashes(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.processors
            .iter()
            .flat_map(|processor| processor.script_hashes().iter().copied())
    }
}
