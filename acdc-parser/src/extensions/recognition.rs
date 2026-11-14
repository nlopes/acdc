use std::borrow::Cow;

use crate::{
    AttributeValue, Block, BlockMetadata, DocumentAttributes, ElementAttributes, Error, Location,
    Substitution, Title, apply_attribute_substitutions, grammar::ParserState,
};

use super::{ExtensionBlock, MacroInvocation, MacroTarget};

pub(crate) fn recognize<'a>(
    state: &ParserState<'a>,
    name: &'a str,
    target: &'a str,
    arguments: &'a str,
    metadata: BlockMetadata<'a>,
    title: Title<'a>,
    location: Location,
) -> Result<Block<'a>, Error> {
    let (processor, descriptor) = state
        .options
        .extensions
        .as_ref()
        .and_then(|r| r.find(name))
        .ok_or_else(|| {
            Error::Extension(
                Box::new(state.create_error_source_location(location.clone())),
                format!("unregistered block macro: {name}"),
            )
        })?;
    let decode = |source: &'a str| {
        state.raw_cell_separator.map_or(source, |separator| {
            state.intern_cow(crate::blocks::table::decode_cell_escapes(source, separator))
        })
    };
    let target = decode(target);
    let arguments = decode(arguments);
    let target = state.intern_cow(apply_attribute_substitutions(
        target,
        &[Substitution::Attributes],
        &state.document_attributes,
    ));
    if matches!(descriptor.target, MacroTarget::Required) && target.is_empty()
        || matches!(descriptor.target, MacroTarget::Empty) && !target.is_empty()
    {
        return Err(Error::Extension(
            Box::new(state.create_error_source_location(location)),
            format!("invalid target for block macro {name}"),
        ));
    }
    let scanned = if arguments.trim().is_empty() {
        Vec::new()
    } else {
        crate::grammar::attributes::extension_attributes(state, arguments)
    };
    let mut positional = Vec::new();
    let mut attributes = ElementAttributes::default();
    for (slot, argument) in scanned.into_iter().enumerate() {
        if argument.name.is_some() && argument.is_unset() {
            continue;
        }
        let empty_slot = argument.is_empty_slot();
        let value = state.intern_str(&argument.value);
        if let Some(name) = argument.name {
            attributes.set(
                Cow::Borrowed(state.intern_str(&name)),
                AttributeValue::String(Cow::Borrowed(value)),
            );
        } else {
            positional.resize(slot, None);
            positional.push((!empty_slot).then_some(value));
        }
    }
    for (name, value) in descriptor.positional.iter().zip(&positional) {
        if let Some(value) = value {
            attributes.insert_if_absent(
                Cow::Borrowed(state.intern_str(name)),
                AttributeValue::String(Cow::Borrowed(value)),
            );
        }
    }
    attributes.merge_defaults(descriptor.defaults.clone());
    Ok(Block::Extension(Box::new(ExtensionBlock {
        metadata,
        title,
        invocation: MacroInvocation {
            name: Cow::Borrowed(name),
            target: Cow::Borrowed(target),
            positional: positional
                .into_iter()
                .map(|value| value.map(Cow::Borrowed))
                .collect(),
            attributes,
            document_attributes: DocumentAttributes::clone(&state.document_attributes),
            location: state.create_error_source_location(location.clone()),
        },
        location,
        processor,
        placeholder: None,
    })))
}
