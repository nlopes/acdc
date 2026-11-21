use std::{
    borrow::Cow,
    cell::Cell,
    fmt,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    DocumentAttributeAssignment, DocumentAttributeValue, DocumentAttributes, ElementAttributes,
    IncludeSourceTarget, Options, SafeMode, SourceLocation,
};

/// The parser scope in which a block macro can be recognized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MacroContext {
    /// Document and section bodies, including `AsciiDoc` table cells.
    Document,
    /// Bodies of compound blocks such as examples and sidebars.
    Compound,
}

/// Whether a macro requires a target before its attribute list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MacroTarget {
    #[default]
    Optional,
    Required,
    Empty,
}

/// Immutable syntax and factory for one block macro.
#[derive(Clone)]
pub struct BlockMacro {
    pub(crate) name: String,
    pub(crate) contexts: Vec<MacroContext>,
    pub(crate) target: MacroTarget,
    pub(crate) positional: Vec<String>,
    pub(crate) defaults: ElementAttributes<'static>,
    pub(super) factory: Arc<dyn Fn() -> Box<dyn BlockMacroProcessor> + Send + Sync>,
}

impl fmt::Debug for BlockMacro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockMacro")
            .field("name", &self.name)
            .field("contexts", &self.contexts)
            .field("target", &self.target)
            .field("positional", &self.positional)
            .field("defaults", &self.defaults)
            .finish_non_exhaustive()
    }
}

impl BlockMacro {
    /// Create a macro whose factory runs once per document, in registration order.
    ///
    /// Factories can share immutable resources. Processors need not be `Send` or
    /// `Sync`; each processor belongs to one document session.
    pub fn new<P: BlockMacroProcessor + 'static>(
        name: impl Into<String>,
        factory: impl Fn() -> P + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            contexts: vec![MacroContext::Document, MacroContext::Compound],
            target: MacroTarget::Optional,
            positional: Vec::new(),
            defaults: ElementAttributes::default(),
            factory: Arc::new(move || Box::new(factory())),
        }
    }

    #[must_use]
    pub fn with_contexts(mut self, contexts: impl IntoIterator<Item = MacroContext>) -> Self {
        self.contexts = contexts.into_iter().collect();
        self
    }

    #[must_use]
    pub const fn with_target(mut self, target: MacroTarget) -> Self {
        self.target = target;
        self
    }

    /// Map positional arguments to names. Empty slots keep their positions.
    /// Named arguments take precedence over positional arguments and defaults.
    #[must_use]
    pub fn with_positional_attributes(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.positional = names.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn with_defaults(mut self, defaults: ElementAttributes<'static>) -> Self {
        self.defaults = defaults;
        self
    }
}

/// Arguments and source-position attributes of one accepted macro invocation.
/// The document attributes are a read-only snapshot at the macro's location.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MacroInvocation<'a> {
    pub name: Cow<'a, str>,
    /// The accepted call from its name through the closing bracket, before
    /// target expansion and argument interpretation. Table-cell escapes follow
    /// the ordinary cell rules. Outer block metadata and trailing content are
    /// excluded. Quotes, spacing, and argument slots remain.
    pub source: Cow<'a, str>,
    pub target: Cow<'a, str>,
    pub positional: Vec<Option<Cow<'a, str>>>,
    pub attributes: ElementAttributes<'a>,
    pub document_attributes: DocumentAttributes<'a>,
    pub location: SourceLocation,
}

/// A processor instance is used only within one document session.
pub trait BlockMacroProcessor {
    /// Process an accepted invocation. Returning an error stops the parse.
    ///
    /// # Errors
    /// Return a message describing why this invocation cannot be processed.
    fn process(
        &mut self,
        invocation: &MacroInvocation<'_>,
        context: &mut MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String>;
}

impl<F> BlockMacroProcessor for F
where
    F: FnMut(&MacroInvocation<'_>, &mut MacroExecutionContext) -> Result<BlockMacroOutput, String>,
{
    fn process(
        &mut self,
        invocation: &MacroInvocation<'_>,
        context: &mut MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        self(invocation, context)
    }
}

/// Attribute changes requested by one committed callback.
///
/// The host applies these changes in order after a successful callback, using
/// normal document assignment policy, before advancing to subsequent source.
/// Values are already resolved; literal attribute references are not expanded again.
#[derive(Debug, Default)]
pub struct MacroExecutionContext<'a> {
    pub(super) file: Option<LocalTarget<'a>>,
    pub(super) warnings: Vec<String>,
    pub(crate) assignments: Vec<(Cow<'static, str>, DocumentAttributeAssignment<'static>)>,
}

#[derive(Debug)]
pub(super) struct LocalTarget<'a> {
    pub(super) call: &'a MacroInvocation<'a>,
    pub(super) options: &'a Options<'a>,
    pub(super) entry: Option<&'a Path>,
    pub(super) budget: &'a Cell<usize>,
}

impl MacroExecutionContext<'_> {
    /// Read this invocation's relative local text target through the caller's
    /// include loader. Return the resolved path alongside the text so processors
    /// can resolve references within it. Reads share the document's include byte budget.
    ///
    /// # Errors
    /// Requires unsafe mode, an enabled loader, and a local source origin.
    /// Invalid targets, loader errors, invalid text encodings, and oversized files fail.
    pub fn read_local_target(&self) -> io::Result<(PathBuf, String)> {
        let input = self
            .file
            .as_ref()
            .ok_or_else(|| io::Error::other("no active macro invocation"))?;
        if input.options.safe_mode != SafeMode::Unsafe {
            return Err(io::Error::other("local macro targets require unsafe mode"));
        }
        let provider = input
            .options
            .include_loader
            .provider()
            .ok_or_else(|| io::Error::other("include loading is disabled"))?;
        let target = Path::new(input.call.target.as_ref());
        if target.as_os_str().is_empty()
            || !target.is_relative()
            || input.call.target.contains(':')
            || input.call.target.starts_with('\\')
        {
            return Err(io::Error::other("expected a relative local macro target"));
        }
        let origin = input.call.location.file.as_deref();
        if origin.is_some_and(|p| p.to_string_lossy().contains("://")) {
            return Err(io::Error::other(
                "local macro target has a URI source origin",
            ));
        }
        let from = origin.and_then(Path::parent);
        let base = if origin == input.entry {
            input.options.base_dir.as_deref().or(from)
        } else {
            from.or(input.options.base_dir.as_deref())
        };
        let path =
            crate::preprocessor::absolute_normalized(&base.unwrap_or(Path::new(".")).join(target))
                .map_err(io::Error::other)?;
        let source = provider
            .open(&IncludeSourceTarget::File(path.clone()))
            .map_err(io::Error::other)?;
        let remaining = input.budget.get();
        let mut bytes = Vec::new();
        let result = source
            .into_reader()
            .take((remaining as u64).saturating_add(1))
            .read_to_end(&mut bytes);
        input.budget.set(remaining.saturating_sub(bytes.len()));
        result?;
        if bytes.len() > remaining {
            return Err(io::Error::other(
                "macro target exceeds the remaining include byte budget",
            ));
        }
        let text = crate::preprocessor::decode_bytes(&bytes, None, &path.to_string_lossy())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok((path, text))
    }

    /// Report a recoverable warning at the current macro invocation.
    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    pub fn set_attribute(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        value: impl Into<DocumentAttributeValue<'static>>,
    ) {
        self.assignments
            .push((name.into(), DocumentAttributeAssignment::Set(value.into())));
    }

    pub fn unset_attribute(&mut self, name: impl Into<Cow<'static, str>>) {
        self.assignments
            .push((name.into(), DocumentAttributeAssignment::Unset));
    }
}

/// Content produced by a registered block macro.
#[derive(Debug)]
#[non_exhaustive]
pub enum BlockMacroOutput {
    /// Emit one plain-text node in a paragraph, preserving the macro's outer
    /// title and metadata. The text is not parsed as inline markup.
    Paragraph(String),
    /// Construct an ordinary block at the invocation, inheriting its title and metadata.
    /// Captured target displays require the document available during tree processing.
    Block(super::BlockBuilder),
    /// Remove the macro and its block metadata.
    None,
    /// Retain a placeholder and its arguments for document-level construction.
    Deferred,
}

impl From<super::BlockBuilder> for BlockMacroOutput {
    fn from(builder: super::BlockBuilder) -> Self {
        Self::Block(builder)
    }
}

impl MacroInvocation<'_> {
    pub(super) fn to_owned_invocation(&self) -> MacroInvocation<'static> {
        MacroInvocation {
            name: Cow::Owned(self.name.to_string()),
            source: Cow::Owned(self.source.to_string()),
            target: Cow::Owned(self.target.to_string()),
            positional: self
                .positional
                .iter()
                .map(|value| value.as_ref().map(|value| Cow::Owned(value.to_string())))
                .collect(),
            attributes: self.attributes.clone().into_static(),
            document_attributes: self.document_attributes.clone().into_static(),
            location: self.location.clone(),
        }
    }
}
