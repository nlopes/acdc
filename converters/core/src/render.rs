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
    html_script_hashes: &'static [&'static str],
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
            html_script_hashes: &[],
            factory: Arc::new(move || Box::new(factory())),
        }
    }
    /// Declare trusted inline scripts for the HTML host's Content Security Policy.
    #[must_use]
    pub fn with_html_script_hashes(mut self, hashes: &'static [&'static str]) -> Self {
        self.html_script_hashes = hashes;
        self
    }
}

/// A mutable renderer for one conversion. HTML dispatches paragraph, listing,
/// and literal blocks. Children of other blocks follow ordinary traversal.
pub trait BlockRenderProcessor {
    /// Select this block before gaining access to output. A decline can warn.
    ///
    /// # Errors
    /// Stops conversion on a selection failure.
    fn accepts(
        &mut self,
        block: BlockRef<'_>,
        context: &mut dyn RenderContext,
    ) -> Result<bool, RenderError>;

    /// Replace the selected block, or decorate the remaining chain with [`Next::render`].
    ///
    /// # Errors
    /// An error stops the chain. Earlier writes and external effects are observable.
    fn render(&mut self, block: BlockRef<'_>, next: Next<'_, '_>) -> Result<(), RenderError>;
}

/// Services available during selection. Output becomes available only on acceptance.
pub trait RenderContext {
    /// Selected backend variant.
    fn variant(&self) -> &str;
    /// Attributes at the current block, including nested scopes.
    fn document_attributes(&self) -> &TraversalContext<'_>;
    /// Validated converter options and safe mode.
    fn options(&self) -> &Options;
    /// Borrow this conversion’s diagnostic sink.
    fn diagnostics(&mut self) -> Diagnostics<'_>;
}

/// HTML-specific output services. The HTML host owns element wrappers, CSS
/// classes, title rendering, and escaping document metadata.
pub trait HtmlRenderHost: RenderContext {
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

/// A one-shot continuation for an accepted block. Write a prefix through
/// [`Self::host`], delegate, then write a suffix through the returned host.
/// Dropping the continuation replaces the block. A delegated error propagates
/// without trying another handler or replaying any writes.
pub struct Next<'a, 'doc> {
    host: &'a mut dyn DispatchHost,
    remaining: &'a mut [Box<dyn BlockRenderProcessor>],
    block: BlockRef<'doc>,
}

impl<'a> Next<'a, '_> {
    /// Write output before delegating, or implement a complete replacement.
    pub fn host(&mut self) -> &mut dyn HtmlRenderHost {
        self.host.output_host()
    }

    /// Run the rest of the chain, falling back to the built-in visitor.
    ///
    /// # Errors
    /// Returns the delegated processor or backend failure.
    pub fn render(self) -> Result<&'a mut dyn HtmlRenderHost, RenderError> {
        dispatch(self.remaining, self.block, self.host)?;
        Ok(self.host.output_host())
    }
}

trait DispatchHost {
    fn output_host(&mut self) -> &mut dyn HtmlRenderHost;
    fn base(&mut self) -> Result<(), RenderError>;
}

struct Host<'a, H, F> {
    host: &'a mut H,
    base: F,
}

impl<H: HtmlRenderHost, F: FnMut(&mut H) -> Result<(), RenderError>> DispatchHost
    for Host<'_, H, F>
{
    fn output_host(&mut self) -> &mut dyn HtmlRenderHost {
        self.host
    }
    fn base(&mut self) -> Result<(), RenderError> {
        (self.base)(self.host)
    }
}

fn dispatch(
    mut processors: &mut [Box<dyn BlockRenderProcessor>],
    block: BlockRef<'_>,
    host: &mut dyn DispatchHost,
) -> Result<(), RenderError> {
    while let Some((processor, remaining)) = processors.split_first_mut() {
        if processor.accepts(block, host.output_host())? {
            return processor.render(
                block,
                Next {
                    host,
                    remaining,
                    block,
                },
            );
        }
        processors = remaining;
    }
    host.base()
}

/// Fresh handlers in package-selection and registration order. The first
/// accepting handler can replace the block or delegate to later handlers.
pub struct RenderSession {
    processors: Vec<Box<dyn BlockRenderProcessor>>,
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
    pub(crate) fn new(processors: Vec<Box<dyn BlockRenderProcessor>>) -> Self {
        Self { processors }
    }

    /// Dispatch an eligible HTML block. The generic fallback uses the original visitor.
    ///
    /// # Errors
    /// Stops at the first selection, processor, or backend failure.
    pub fn render<H: HtmlRenderHost>(
        &mut self,
        block: BlockRef<'_>,
        host: &mut H,
        base: impl FnMut(&mut H) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        dispatch(&mut self.processors, block, &mut Host { host, base })
    }
}

impl BlockRenderer {
    pub(crate) fn html_script_hashes(&self) -> &'static [&'static str] {
        self.html_script_hashes
    }
}
