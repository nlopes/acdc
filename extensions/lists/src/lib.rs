//! Native `list-of::<element>[]` block macros.
//!
//! Register [`registry`] with the parser directly, or select it through converter
//! core's extension catalog and pipeline. Construction uses ordinary paragraphs,
//! cross references, and hard line breaks, so every backend shares the same list.

use acdc_parser::{
    AttributeValue, Error,
    ast::CaptionRef,
    extensions::{
        BlockBuilder, BlockContext, BlockMacro, BlockMacroOutput, BlockMacroProcessor,
        DeferredBlock, DisplayIntent, DocumentEditor, InlineSpec, MacroExecutionContext,
        MacroInvocation, RegistrationError, Registry, TargetId, TreeProcessor,
        TreeProcessorRegistration,
    },
};

/// Register the deferred macro and its document-local tree processor.
///
/// # Errors
/// Returns an error if a built-in name conflicts with this registration.
pub fn registry() -> Result<Registry, RegistrationError> {
    let mut builder = Registry::builder();
    builder.register(
        BlockMacro::new("list-of", || Defer).with_positional_attributes(["hide_empty_section"]),
    )?;
    builder.register_tree_processor(TreeProcessorRegistration::new("lists", || Lists))?;
    Ok(builder.build())
}

struct Defer;

impl BlockMacroProcessor for Defer {
    fn process(
        &mut self,
        invocation: &MacroInvocation<'_>,
        _: &mut MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        if invocation.attributes.get("enhanced_rendering").is_some() {
            return Err("list-of does not support enhanced_rendering; title formatting is automatic, so remove this option".into());
        }
        if invocation.positional.iter().skip(1).any(Option::is_some) {
            return Err("list-of supports one positional argument: hide_empty_section".into());
        }
        Ok(BlockMacroOutput::Deferred)
    }
}

struct Lists;

#[derive(Clone, Copy)]
enum EmptySection {
    Keep,
    Hide,
}

enum Request {
    Generate {
        output: TargetId,
        context: BlockContext,
        empty_section: EmptySection,
    },
    Unknown {
        output: TargetId,
        element: String,
        source: String,
    },
}

impl From<&DeferredBlock> for Request {
    fn from(request: &DeferredBlock) -> Self {
        let output = request.target().clone();
        let invocation = &request.invocation;
        if let Some(context) = context(invocation.target.trim()) {
            let empty_section = match invocation.attributes.get("hide_empty_section") {
                None | Some(AttributeValue::Bool(false) | AttributeValue::None) => {
                    EmptySection::Keep
                }
                Some(_) => EmptySection::Hide,
            };
            Self::Generate {
                output,
                context,
                empty_section,
            }
        } else {
            Self::Unknown {
                output,
                element: invocation.target.to_string(),
                source: invocation.source.to_string(),
            }
        }
    }
}

impl Request {
    fn output(&self) -> &TargetId {
        match self {
            Self::Generate { output, .. } | Self::Unknown { output, .. } => output,
        }
    }
}

impl TreeProcessor for Lists {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let requests: Vec<_> = editor
            .deferred_blocks()
            .iter()
            .filter(|request| request.invocation.name == "list-of")
            .map(Request::from)
            .collect();
        for request in requests {
            // A previous request may have removed the containing section and
            // consumed every remaining request in its subtree.
            if editor.query().get(request.output()).is_none() {
                continue;
            }
            let (output, block_context, empty_section) = match request {
                Request::Generate {
                    output,
                    context,
                    empty_section,
                } => (output, context, empty_section),
                Request::Unknown {
                    output,
                    element,
                    source,
                } => {
                    editor.warn(&output, format!("unknown list-of element: {element}"))?;
                    editor.replace_placeholder(&output, BlockBuilder::paragraph(source))?;
                    continue;
                }
            };
            // Snapshot this request's membership before assigning IDs or editing
            // the placeholder. Later requests observe earlier completed edits.
            let mut found = false;
            let entries: Vec<_> = editor
                .query()
                .select(block_context)
                .filter_map(|view| {
                    found = true;
                    let captioned = has_caption(view.caption());
                    (captioned || view.title().is_some()).then(|| {
                        (
                            view.target().clone(),
                            if captioned {
                                DisplayIntent::CaptionAndTitle
                            } else {
                                DisplayIntent::Title
                            },
                        )
                    })
                })
                .collect();
            if entries.is_empty() {
                if !found
                    && matches!(empty_section, EmptySection::Hide)
                    && let Some(section) = editor
                        .query()
                        .get(&output)
                        .and_then(|view| view.containing_section())
                        .cloned()
                {
                    editor.remove_section(&section)?;
                    continue;
                }
                editor.discard_placeholder(&output)?;
                continue;
            }
            let mut content = Vec::with_capacity(entries.len() * 2);
            for (target, intent) in entries {
                content.push(editor.capture(&target)?.display(intent));
                content.push(InlineSpec::LineBreak);
            }
            editor.replace_placeholder(&output, BlockBuilder::paragraph_inlines(content))?;
        }
        Ok(())
    }
}

fn has_caption(caption: Option<CaptionRef<'_>>) -> bool {
    match caption {
        Some(CaptionRef::Numbered {
            number: Some(_), ..
        }) => true,
        Some(CaptionRef::Custom(text)) => !text.is_empty(),
        Some(CaptionRef::CustomContent(content)) => !content.is_empty(),
        _ => false,
    }
}

fn context(element: &str) -> Option<BlockContext> {
    Some(match element {
        "image" => BlockContext::Image,
        "table" => BlockContext::Table,
        "listing" => BlockContext::Listing,
        "literal" => BlockContext::Literal,
        "example" => BlockContext::Example,
        "quote" => BlockContext::Quote,
        "verse" => BlockContext::Verse,
        "sidebar" => BlockContext::Sidebar,
        "open" => BlockContext::Open,
        "pass" => BlockContext::Passthrough,
        "stem" => BlockContext::Stem,
        "audio" => BlockContext::Audio,
        "video" => BlockContext::Video,
        "admonition" => BlockContext::Admonition,
        "paragraph" => BlockContext::Paragraph,
        "section" => BlockContext::Section,
        "olist" => BlockContext::OrderedList,
        "ulist" => BlockContext::UnorderedList,
        "dlist" => BlockContext::DescriptionList,
        "colist" => BlockContext::CalloutList,
        _ => return None,
    })
}
