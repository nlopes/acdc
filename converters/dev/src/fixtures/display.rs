//! Generic public-API consumer shared by converter fixtures.

use acdc_parser::{
    Error,
    extensions::{
        BlockBuilder, BlockMacro, BlockMacroOutput, BlockMacroProcessor, DisplayIntent,
        DocumentEditor, MacroExecutionContext, MacroInvocation, Registry, TreeProcessor,
        TreeProcessorRegistration,
    },
};

pub(super) fn registry() -> Result<Registry, acdc_parser::extensions::RegistrationError> {
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("fixture-display", || Defer))?;
    builder.register_tree_processor(TreeProcessorRegistration::for_macro(
        "fixture-display",
        || Display,
    ))?;
    Ok(builder.build())
}

struct Defer;

impl BlockMacroProcessor for Defer {
    fn process(
        &mut self,
        _: &MacroInvocation<'_>,
        _: &mut MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        Ok(BlockMacroOutput::Deferred)
    }
}

struct Display;

impl TreeProcessor for Display {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let requests: Vec<_> = editor
            .deferred_blocks()
            .map(|request| {
                (
                    request.target().clone(),
                    request.invocation.target.to_string(),
                    request.invocation.location.clone(),
                )
            })
            .collect();
        // Capture the full membership before constructing output or editing targets.
        let mut selected = Vec::new();
        for (output, id, origin) in requests {
            let target = editor
                .query()
                .blocks()
                .find(|view| view.output_id() == Some(id.as_str()))
                .map(|view| view.target().clone())
                .ok_or_else(|| {
                    Error::Extension(
                        Box::new(origin.clone()),
                        format!("fixture display target {id} is absent"),
                    )
                })?;
            selected.push((output, target.clone(), editor.capture(&target)?));
        }
        for (output, _, target) in &selected {
            editor.replace_placeholder(
                output,
                BlockBuilder::unordered_list([
                    vec![target.display(DisplayIntent::Caption)],
                    vec![target.display(DisplayIntent::Title)],
                    vec![target.display(DisplayIntent::CaptionAndTitle)],
                ]),
            )?;
        }
        // The frozen titles keep their formatting and original attribute context;
        // caption requests observe this later edit and the renamed output ID.
        for (_, target, _) in selected {
            if editor
                .query()
                .get(&target)
                .is_some_and(|view| view.output_id() == Some("edited"))
            {
                editor.replace_block(
                    &target,
                    BlockBuilder::listing("changed").with_attributes(
                        "id=renamed,title='Changed *later*',caption='*Updated* <&> '",
                    ),
                )?;
            }
            if editor
                .query()
                .get(&target)
                .is_some_and(|view| view.output_id() == Some("removed"))
            {
                editor.remove_section(&target)?;
            }
        }
        Ok(())
    }
}
