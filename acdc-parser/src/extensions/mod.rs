//! Registered source, include, block macro, and tree processors with fresh document sessions.
//!
//! ```
//! use acdc_parser::{Options, extensions::{
//!     BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroInvocation, Registry,
//! }};
//!
//! struct Greeting;
//! impl BlockMacroProcessor for Greeting {
//!     fn process(&mut self, call: &MacroInvocation<'_>, _: &mut acdc_parser::extensions::MacroExecutionContext) -> Result<BlockMacroOutput, String> {
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
//! Each accepted macro runs once at its committed block boundary, in source order.
//! Its attribute snapshot reflects the macro's position. The callback's mutable
//! [`MacroExecutionContext`] queues attribute sets and unsets. The host applies
//! accepted changes as body attribute events before advancing the source reader.
//! Caller locks, intrinsic restrictions, and nested-document policy still apply.
//! Callback errors stop parsing; effects from earlier callbacks are not undone.
//! Deferred blocks retain their arguments and a session-local identity; they are not
//! renderable content. `ExtensionParseResult::finish()` rejects unresolved requests
//! and returns an ordinary `ParseResult` for conversion.
//!
//! Source ordering also applies inside compound bodies and pipe/exclamation
//! `AsciiDoc` cells. Other table formats retain buffered preprocessing. Nested
//! attributes follow the normal inheritance policy and remain local to the cell.
//! A table cannot silently discard a cell after its callback executed; that
//! layout failure is a located extension error. Preprocessing selects authored
//! cell content once; duplicate cells run their own handlers over that selection.
//!
//! ```
//! use acdc_parser::{Options, extensions::{
//!     BlockMacro, BlockMacroOutput, MacroExecutionContext, MacroInvocation, Registry,
//! }};
//!
//! let mut builder = Registry::builder();
//! builder.register(BlockMacro::new("configure", || {
//!     |call: &MacroInvocation<'_>, context: &mut MacroExecutionContext| {
//!         context.set_attribute("audience", call.target.to_string());
//!         Ok(BlockMacroOutput::None)
//!     }
//! }))?;
//! let parsed = builder.build().parse(
//!     "configure::expert[]\n\nifdef::audience[]\nVisible to {audience}.\nendif::[]",
//!     &Options::default(),
//! )?.finish()?;
//! assert!(parsed.conditionals().next().unwrap().is_active());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Source preprocessors run once on the normalized entry text. Return line edits
//! in source order. The first include processor to supply bytes handles that
//! resolved target. All supplied bytes pass through ordinary include processing.
//!
//! ```
//! use acdc_parser::{IncludeSource, IncludeSourceTarget, Options, SafeMode,
//!     extensions::{Registry, SourceEdit, SourceProcessorContext}};
//!
//! let mut builder = Registry::builder();
//! builder.register_preprocessor("strip-marker", || |text: &str, _: &SourceProcessorContext<'_>| {
//!     Ok(if text.lines().next() == Some("GENERATED") {
//!         vec![SourceEdit::Remove(1)]
//!     } else {
//!         Vec::new()
//!     })
//! })?;
//! builder.register_include_processor("version", || |target: &IncludeSourceTarget, _: &SourceProcessorContext<'_>| {
//!     Ok(match target {
//!         IncludeSourceTarget::File(path) if path.ends_with("version.adoc") =>
//!             Some(IncludeSource::from_string("Version 1.")),
//!         _ => None,
//!     })
//! })?;
//! let options = Options::builder().with_safe_mode(SafeMode::Safe).build()?;
//! let parsed = builder.build().parse("GENERATED\ninclude::version.adoc[]", &options)?.finish()?;
//! assert_eq!(parsed.preprocessed_source(), "Version 1.");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
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
//! [`BlockView::title_source`] retains raw title input, its inline substitution
//! order, original location, and document attributes before title interpretation.
//! This includes titles that expand to empty content, attribute titles, sections,
//! and discrete headings. Titles supplied through builder attribute lists retain
//! that input with the target's location and source-position attributes.
//! Ordinary parser entry points do not retain these extension snapshots.
//!
//! [`DocumentEditor::capture`] freezes a title and its source context. Use
//! [`CapturedTarget::display`] with [`BlockBuilder::paragraph_inlines`] or
//! [`BlockBuilder::unordered_list`] for explicit caption/title links. Compose
//! these requests with literal text, hard breaks, and groups through [`InlineSpec`].
//! Finalization follows surviving handles and their latest caption data, without
//! refreshing captured titles. Converter core prepares a fresh ordinary document
//! through [`PreparedDocument`] before traversal; the shared parse stays unchanged.
//! Checked ASG export rejects symbolic displays until they have been prepared.
//!
//! Tree processors run after block macros. Register a macro's resolver with
//! [`TreeProcessorRegistration::for_macro`] to receive its surviving requests in
//! source order. Only that processor can replace or discard those placeholders.
//! All tree processors can query the document and edit ordinary blocks.
//!
//! Builder bodies are literal; attribute lists use the parser's
//! metadata rules. Use [`DocumentEditor::replace_source`] explicitly to interpret
//! retained paragraph or listing/literal source with its original substitutions.
//!
//! ```
//! use acdc_parser::{Error, Options, extensions::{
//!     BlockBuilder, BlockMacro, BlockMacroOutput, BlockMacroProcessor,
//!     DocumentEditor, MacroInvocation, Registry, TreeProcessor, TreeProcessorRegistration,
//! }};
//!
//! struct Request;
//! impl BlockMacroProcessor for Request {
//!     fn process(&mut self, _: &MacroInvocation<'_>, _: &mut acdc_parser::extensions::MacroExecutionContext) -> Result<BlockMacroOutput, String> {
//!         Ok(BlockMacroOutput::Deferred)
//!     }
//! }
//! struct Fill;
//! impl TreeProcessor for Fill {
//!     fn process(&mut self, document: &mut DocumentEditor<'_>) -> Result<(), Error> {
//!         let targets: Vec<_> = document.deferred_blocks()
//!             .map(|request| request.target().clone()).collect();
//!         for target in targets {
//!             document.replace_placeholder(&target, BlockBuilder::unordered_list(["One", "Two"]))?;
//!         }
//!         Ok(())
//!     }
//! }
//! let mut builder = Registry::builder();
//! builder.register(BlockMacro::new("request", || Request))?;
//! builder.register_tree_processor(TreeProcessorRegistration::for_macro("request", || Fill))?;
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
//! [`DocumentEditor::replace_block`] offers the same construction path for ordinary
//! blocks, invalidating descendant handles and requests. Images and verbatim blocks
//! render through the usual converters. Attribute defaults are weaker than inherited
//! metadata and explicit lists; unsets apply afterward. Textual figures opt into
//! [`crate::CaptionKind::Figure`] through [`BlockBuilder::with_caption_kind`]. New
//! numbers advance beyond original and generated ordinals, including removed blocks.
//! Source-position caption labels and surviving numbers remain unchanged. Asciidoctor
//! 2.0.26 also retains surviving numbers on removal; its tree-processor image helper
//! may reuse lower ordinals, while acdc deliberately allocates monotonically.
//!
//! Each edit validates before mutation and repairs catalogs and reference warnings
//! before returning. An edit error leaves the visible tree, handles, numbering, and
//! diagnostics unchanged. Earlier successful edits and external effects remain if
//! a later processor fails; the processor sequence is not transactional. Generated
//! summaries and copied selections are snapshots. [`ExtensionParseResult::normalize`]
//! repeats structural repair idempotently without rerunning callbacks or numbering.
//! The working result retains session state and pending requests; consuming it with
//! [`ExtensionParseResult::finish`] yields an ordinary finalized [`crate::ParseResult`]
//! or a located error for an unresolved request. Checked ASG export rejects pending
//! blocks in both profiles and compatibility modes.

mod block_macro;
mod block_style;
pub use block_style::{
    BlockStyle, BlockStyleOutput, MAX_STYLE_NESTING, RawBlockContext, RawBlockInput,
    RawBlockProcessor,
};
pub(crate) use deferred::Invocation;
mod builder;
mod deferred;
mod display;
mod editor;
pub(crate) mod finalization;
mod inline;
pub(crate) mod late_inline;
mod query;
mod recognition;
mod registry;
mod session;
mod source;
pub(crate) use source::IncludeProcessors;
pub use source::{IncludeProcessor, SourceEdit, SourcePreprocessor, SourceProcessorContext};
mod target;
mod title;
mod traversal;
pub(crate) use traversal::{TablePart, compound_body_mut, ordinary_list_items_mut, table_cell_mut};
mod tree_processor;

pub use block_macro::{
    BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroContext, MacroExecutionContext,
    MacroInvocation, MacroTarget,
};
pub use builder::BlockBuilder;
pub(crate) use builder::SourceContext;
pub use deferred::{DeferredBlock, ExtensionBlock, PlaceholderId};
pub use display::{
    CapturedTarget, DisplayIntent, DisplayLabel, DisplayRequest, GeneratedDisplayRef,
    PreparedDocument,
};
pub(crate) use display::{DisplayState, GeneratedDisplay};
pub use editor::DocumentEditor;
pub use inline::InlineSpec;
pub use late_inline::{
    InlineConversionContext, InlineInvocation, InlineMacroOutput, InlineMacroProcessor,
    InlineMatcher, RetainedInline,
};
pub use query::{BlockContext, BlockView, DocumentQuery};
pub(crate) use registry::ReferenceNormalizer;
pub use registry::{RegistrationError, Registry, RegistryBuilder};
pub use session::ExtensionParseResult;
pub use target::{TargetError, TargetId};
pub use title::TitleSource;
pub use tree_processor::{TreeProcessor, TreeProcessorRegistration};

pub(crate) use recognition::recognize;
pub(crate) use session::Session;

#[cfg(test)]
mod tests;
