//! Registered block macros and ordered tree processors with fresh document sessions.
//!
//! ```
//! use acdc_parser::{Options, extensions::{
//!     BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroInvocation, Registry,
//! }};
//!
//! struct Greeting;
//! impl BlockMacroProcessor for Greeting {
//!     fn process(&mut self, call: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
//!         Ok(BlockMacroOutput::Paragraph(format!("Hello, {}!", call.target)))
//!     }
//! }
//!
//! let mut builder = Registry::builder();
//! builder.register(BlockMacro::new("greet", || Greeting))?;
//! let registry = builder.build();
//! let parsed = registry.parse("greet::World[]", &Options::default())?;
//! assert!(parsed.deferred_blocks().is_empty());
//! assert_eq!(parsed.finish()?.document().blocks().count(), 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! On success, each accepted macro runs once, in document order, after the document is accepted.
//! Its attribute snapshot reflects the macro's position. The host repairs catalogs
//! after the callbacks finish.
//! Callbacks cannot affect conditionals, includes, or later parsing. Deferred
//! blocks retain their arguments and a session-local identity; they are not
//! renderable content. `ExtensionParseResult::finish()` rejects unresolved requests
//! and returns an ordinary `ParseResult` for conversion.
//!
//! Query parsed blocks by semantic context and let the host assign link targets:
//!
//! ```
//! use acdc_parser::{Options, extensions::{BlockContext, Registry}};
//!
//! let mut result = Registry::default().parse(".A figure\nimage::figure.svg[]", &Options::default())?;
//! let images: Vec<_> = result.query().select(BlockContext::Image)
//!     .map(|image| image.target().clone()).collect();
//! for target in images {
//!     let id = result.ensure_id(&target)?.to_owned();
//!     assert_eq!(result.query().get(&target).unwrap().output_id(), Some(id.as_str()));
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Queries include nested list attachments, compound blocks, and `AsciiDoc` cells.
//! Titles and captions carry their parsed source-position values. Assigning an ID
//! preserves existing IDs and updates the reference catalog; it does not change
//! block order or rerun processors. Deferred requests expose the same target handles.
//!
//! Tree processors run after block macros and can replace pending requests with
//! ordinary content. Builders accept literal text; markup and substitutions are
//! not run again.
//!
//! ```
//! use acdc_parser::{Error, Options, extensions::{
//!     BlockBuilder, BlockMacro, BlockMacroOutput, BlockMacroProcessor,
//!     DocumentEditor, MacroInvocation, Registry, TreeProcessor, TreeProcessorRegistration,
//! }};
//!
//! struct Request;
//! impl BlockMacroProcessor for Request {
//!     fn process(&mut self, _: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
//!         Ok(BlockMacroOutput::Deferred)
//!     }
//! }
//! struct Fill;
//! impl TreeProcessor for Fill {
//!     fn process(&mut self, document: &mut DocumentEditor<'_>) -> Result<(), Error> {
//!         let targets: Vec<_> = document.deferred_blocks().iter()
//!             .map(|request| request.target().clone()).collect();
//!         for target in targets {
//!             document.replace_placeholder(&target, BlockBuilder::unordered_list(["One", "Two"]))?;
//!         }
//!         Ok(())
//!     }
//! }
//! let mut builder = Registry::builder();
//! builder.register(BlockMacro::new("request", || Request))?;
//! builder.register_tree_processor(TreeProcessorRegistration::new("fill", || Fill))?;
//! let result = builder.build().parse("request::[]", &Options::default())?;
//! assert!(result.deferred_blocks().is_empty());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Each tree processor runs once in registration order and sees earlier edits.
//! Queries cannot remain borrowed across an edit. Surviving handles remain valid;
//! removed handles never resolve to another block. Section removal repairs TOC and
//! reference catalogs, preserves surviving section/caption numbers, and compacts
//! footnotes. Replacing a placeholder preserves its handle, metadata, and source.

mod block_macro;
mod builder;
mod deferred;
mod editor;
pub(crate) mod finalization;
mod query;
mod recognition;
mod registry;
mod session;
mod target;
mod traversal;
mod tree_processor;

pub use block_macro::{
    BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroContext, MacroInvocation, MacroTarget,
};
pub use builder::BlockBuilder;
pub use deferred::{DeferredBlock, ExtensionBlock, PlaceholderId};
pub use editor::DocumentEditor;
pub use query::{BlockContext, BlockView, DocumentQuery};
pub use registry::{RegistrationError, Registry, RegistryBuilder};
pub use session::ExtensionParseResult;
pub use target::{TargetError, TargetId};
pub use tree_processor::{TreeProcessor, TreeProcessorRegistration};

pub(crate) use recognition::recognize;
pub(crate) use session::Session;

#[cfg(test)]
mod tests;
