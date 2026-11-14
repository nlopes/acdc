use std::{borrow::Cow, fmt, sync::Arc};

use crate::{
    DocumentAttributeAssignment, DocumentAttributeValue, DocumentAttributes, ElementAttributes,
    SourceLocation,
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
pub struct MacroExecutionContext {
    pub(crate) assignments: Vec<(Cow<'static, str>, DocumentAttributeAssignment<'static>)>,
}

impl MacroExecutionContext {
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
    /// Remove the macro and its block metadata.
    None,
    /// Retain a placeholder and its arguments for document-level construction.
    Deferred,
}

impl MacroInvocation<'_> {
    pub(super) fn to_owned_invocation(&self) -> MacroInvocation<'static> {
        MacroInvocation {
            name: Cow::Owned(self.name.to_string()),
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
