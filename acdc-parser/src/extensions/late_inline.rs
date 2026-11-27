//! Authored inline macros evaluated against one conversion's finalized document.

use std::{collections::HashMap, sync::Arc};

use crate::{DocumentAttributes, SourceLocation, ast::DocumentRef};

/// Recognize an authored macro in text where macro substitution is enabled.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InlineMatcher {
    /// `name:target[body]`.
    Full,
    /// `name:[body]`. The bracket content supplies both target and body.
    Short,
    /// Literal delimiters around a body, for example `@(` and `)`.
    Delimited { open: String, close: String },
}

/// Arguments and originating source span of one inline macro.
pub struct InlineInvocation<'a> {
    /// Full-syntax target or short-syntax bracket content. Delimited calls have an empty target.
    pub target: &'a str,
    /// Body after the enabled attribute substitutions for this conversion.
    pub body: &'a str,
    /// Retained source spelling before conversion attribute substitution.
    pub source_body: &'a str,
    pub location: &'a SourceLocation,
}

/// Values and services available to one conversion.
pub struct InlineConversionContext<'a> {
    /// Name of the backend currently writing output.
    pub backend: &'a str,
    /// Finalized document after all tree processors have run.
    pub document: DocumentRef<'a>,
    /// Attributes configured by the backend performing this conversion.
    attributes: &'a DocumentAttributes<'a>,
    /// Document attributes at the authored invocation.
    source_attributes: &'a DocumentAttributes<'a>,
    pub(super) counters: &'a mut HashMap<String, usize>,
    pub(super) warnings: &'a mut dyn FnMut(String),
}

impl<'a> InlineConversionContext<'a> {
    /// Look up a conversion override, then the value in effect at the call's
    /// source position, then the backend default. Explicit unsets hide defaults.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&'a crate::DocumentAttributeValue<'a>> {
        if let Some(assignment) = self.attributes.assignment(name) {
            return assignment.value();
        }
        if let Some(assignment) = self.source_attributes.assignment(name) {
            return assignment.value();
        }
        self.attributes
            .get(name)
            .or_else(|| self.source_attributes.get(name))
    }

    /// Increment a conversion-local named counter, starting at one.
    pub fn increment(&mut self, name: &str) -> usize {
        if let Some(value) = self.counters.get_mut(name) {
            *value += 1;
            *value
        } else {
            self.counters.insert(name.to_owned(), 1);
            1
        }
    }

    /// Emit a non-fatal diagnostic at this invocation.
    pub fn warn(&mut self, message: impl Into<String>) {
        (self.warnings)(message.into());
    }
}

/// Prepared inline output. Text is literal and uses ordinary backend escaping.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum InlineMacroOutput {
    Text(String),
    Strong(String),
    Emphasis(String),
    LineBreak,
}

/// A processor instance belongs to one conversion and is created on first use.
pub trait InlineMacroProcessor {
    /// # Errors
    /// Returning an error stops preparation before output is written.
    fn process(
        &mut self,
        call: &InlineInvocation<'_>,
        ctx: &mut InlineConversionContext<'_>,
    ) -> Result<InlineMacroOutput, String>;
}

impl<F> InlineMacroProcessor for F
where
    F: FnMut(
        &InlineInvocation<'_>,
        &mut InlineConversionContext<'_>,
    ) -> Result<InlineMacroOutput, String>,
{
    fn process(
        &mut self,
        call: &InlineInvocation<'_>,
        context: &mut InlineConversionContext<'_>,
    ) -> Result<InlineMacroOutput, String> {
        self(call, context)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Registration {
    pub(super) handler: super::registry::ProcessorFactory<dyn InlineMacroProcessor>,
    pub(crate) matcher: InlineMatcher,
    pub(crate) prefix: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Arguments<'a> {
    pub(crate) substitute_attributes: bool,
    pub(crate) file: Option<std::path::PathBuf>,
    pub(crate) name: String,
    pub(crate) target: &'a str,
    pub(crate) body: &'a str,
    pub(crate) attributes: Arc<DocumentAttributes<'a>>,
}

/// Retained spelling of an authored inline invocation before conversion.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedInline<'a> {
    pub(crate) literal: crate::Plain<'a>,
    pub(crate) args: Arc<Arguments<'a>>,
}

impl Registration {
    pub(crate) fn name(&self) -> &str {
        &self.handler.name
    }
}

pub(super) struct ConversionSession<'a> {
    registry: &'a super::Registry,
    backend: &'a str,
    attributes: &'a DocumentAttributes<'a>,
    processors: HashMap<usize, Box<dyn InlineMacroProcessor>>,
    counters: HashMap<String, usize>,
}

impl<'a> ConversionSession<'a> {
    pub(super) fn new(
        registry: &'a super::Registry,
        backend: &'a str,
        attributes: &'a DocumentAttributes<'a>,
    ) -> Self {
        Self {
            registry,
            backend,
            attributes,
            processors: HashMap::new(),
            counters: HashMap::new(),
        }
    }

    pub(super) fn expand<'doc>(
        &mut self,
        arena: &'doc bumpalo::Bump,
        document: DocumentRef<'_>,
        call: &RetainedInline<'doc>,
        warn: &mut impl FnMut(crate::SourceLocation, String),
    ) -> Result<crate::InlineNode<'doc>, crate::Error> {
        use crate::{Bold, FormattingForm, InlineNode, Italic, LineBreak};
        let location = &call.literal.location;
        let args = &call.args;
        let origin = SourceLocation::at_location(args.file.clone(), location.clone());
        let Some((index, entry)) = self.registry.inline_registration(&args.name) else {
            return Ok(InlineNode::Plain(call.literal.clone()));
        };
        let processor = self
            .processors
            .entry(index)
            .or_insert_with(|| (entry.handler.factory)());
        let mut warnings = |message| warn(origin.clone(), message);
        let mut context = InlineConversionContext {
            backend: self.backend,
            document,
            attributes: self.attributes,
            source_attributes: &args.attributes,
            counters: &mut self.counters,
            warnings: &mut warnings,
        };
        let substitute = |text| {
            if args.substitute_attributes {
                crate::model::substitution::substitute_attributes(
                    text,
                    args.attributes.reference_mode(),
                    |name| context.attribute(name),
                )
            } else {
                std::borrow::Cow::Borrowed(text)
            }
        };
        let target = substitute(args.target);
        let body = substitute(args.body);
        let output = processor
            .process(
                &InlineInvocation {
                    target: &target,
                    body: &body,
                    source_body: args.body,
                    location: &origin,
                },
                &mut context,
            )
            .map_err(|message| {
                crate::Error::Extension(Box::new(origin), format!("{}: {message}", args.name))
            })?;
        let text = |s: String| super::inline::plain(arena.alloc_str(&s), location);
        Ok(match output {
            InlineMacroOutput::Text(s) => text(s),
            InlineMacroOutput::Strong(s) => InlineNode::Bold(Bold::new(
                vec![text(s)],
                FormattingForm::Constrained,
                location.clone(),
            )),
            InlineMacroOutput::Emphasis(s) => InlineNode::Italic(Italic::new(
                vec![text(s)],
                FormattingForm::Constrained,
                location.clone(),
            )),
            InlineMacroOutput::LineBreak => InlineNode::LineBreak(LineBreak::new(location.clone())),
        })
    }
}
