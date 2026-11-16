//! Native `list-of::<element>[]` block macros.
//!
//! Register [`registry`] with the parser directly, or select it through converter
//! core's extension catalog and pipeline. Construction uses ordinary paragraphs,
//! cross references, and hard line breaks, so every backend shares the same list.

use acdc_parser::{
    Error,
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
    builder.register(BlockMacro::new("list-of", || Defer))?;
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
        if invocation.attributes.get("enhanced_rendering").is_some()
            || invocation.attributes.get("hide_empty_section").is_some()
            || invocation.positional.iter().any(Option::is_some)
        {
            return Err("list-of currently supports only default options; enhanced_rendering and hide_empty_section are not implemented".into());
        }
        Ok(BlockMacroOutput::Deferred)
    }
}

struct Lists;

enum Request {
    Generate {
        output: TargetId,
        context: BlockContext,
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
            Self::Generate { output, context }
        } else {
            Self::Unknown {
                output,
                element: invocation.target.to_string(),
                source: invocation.source.to_string(),
            }
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
            let (output, block_context) = match request {
                Request::Generate { output, context } => (output, context),
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
            let entries: Vec<_> = editor
                .query()
                .select(block_context)
                .filter_map(|view| {
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
