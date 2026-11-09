use std::collections::HashSet;

use acdc_parser::{
    Block, BlockMetadata, Caption, DelimitedBlockType, Document,
    ast::{DocumentRef, ElementIdRef, IdOrigin, StructuralRef},
};
use serde_json::Value;

use super::Error;

// The current TCK compares a smaller graph than regular ASG export. Prepare a
// private copy so caption snapshots and generated identities remain in the AST.
pub(super) fn document(document: DocumentRef<'_>) -> Result<Value, Error> {
    let generated = document
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node {
                block.id()
            } else {
                None
            }
        })
        .filter(|id| id.origin() == IdOrigin::Generated)
        .map(ElementIdRef::as_str)
        .collect::<HashSet<_>>();
    let mut copy = Document::from(document);
    let blocks = blocks(copy.take_blocks())?;
    copy.set_blocks(blocks);
    let references = copy
        .take_references()
        .map(|(id, mut reference)| {
            let mut caption = reference.take_caption();
            omit_derived_caption(&mut caption);
            reference.set_caption(caption);
            (id, reference)
        })
        .collect::<Vec<_>>();
    copy.set_references(references);
    let mut value = acdc_parser::asg::export_document((&copy).into())?;
    let mut required = HashSet::new();
    reference_targets(&value, &mut required);
    omit_generated_ids(&mut value, &generated, &required);
    Ok(value)
}

fn omit_derived_caption(caption: &mut Option<Caption<'_>>) {
    if matches!(
        caption,
        Some(Caption::Numbered { .. } | Caption::Unnumbered)
    ) {
        *caption = None;
    }
}

fn metadata(mut metadata: BlockMetadata<'_>) -> BlockMetadata<'_> {
    omit_derived_caption(&mut metadata.caption);
    metadata
}

fn blocks<'doc>(values: impl IntoIterator<Item = Block<'doc>>) -> Result<Vec<Block<'doc>>, Error> {
    values
        .into_iter()
        .map(|mut block| {
            match &mut block {
                Block::Section(section) => {
                    let content = blocks(section.take_blocks())?;
                    section.set_blocks(content);
                }
                Block::Paragraph(paragraph) => {
                    paragraph.set_metadata(metadata(paragraph.clone_metadata()))?;
                }
                Block::Image(image) => {
                    image.set_metadata(metadata(image.clone_metadata()))?;
                }
                Block::DelimitedBlock(delimited) => {
                    delimited.set_metadata(metadata(delimited.clone_metadata()))?;
                    let mut body =
                        delimited.replace_body(DelimitedBlockType::DelimitedOpen(Vec::new()));
                    if let DelimitedBlockType::DelimitedExample(content)
                    | DelimitedBlockType::DelimitedOpen(content)
                    | DelimitedBlockType::DelimitedSidebar(content)
                    | DelimitedBlockType::DelimitedQuote(content) = &mut body
                    {
                        *content = blocks(std::mem::take(content))?;
                    }
                    delimited.replace_body(body);
                }
                Block::Admonition(admonition) => {
                    let content = blocks(admonition.take_blocks())?;
                    admonition.set_blocks(content);
                }
                Block::UnorderedList(list) => {
                    let mut items = list.take_items().collect::<Vec<_>>();
                    for item in &mut items {
                        let content = blocks(item.take_blocks())?;
                        item.set_blocks(content);
                    }
                    list.set_items(items);
                }
                Block::OrderedList(list) => {
                    let mut items = list.take_items().collect::<Vec<_>>();
                    for item in &mut items {
                        let content = blocks(item.take_blocks())?;
                        item.set_blocks(content);
                    }
                    list.set_items(items);
                }
                Block::DescriptionList(list) => {
                    let mut items = list.take_items().collect::<Vec<_>>();
                    for item in &mut items {
                        if let Some(mut definition) = item.take_definition() {
                            let content = blocks(definition.take_blocks())?;
                            definition.set_blocks(content);
                            item.set_definition(Some(definition));
                        }
                    }
                    list.set_items(items);
                }
                Block::CalloutList(list) => {
                    let mut items = list.take_items().collect::<Vec<_>>();
                    for item in &mut items {
                        let content = blocks(item.take_blocks())?;
                        item.set_blocks(content);
                    }
                    list.set_items(items);
                }
                Block::TableOfContents(_)
                | Block::DiscreteHeading(_)
                | Block::DocumentAttribute(_)
                | Block::ThematicBreak(_)
                | Block::PageBreak(_)
                | Block::Audio(_)
                | Block::Video(_)
                | Block::Comment(_)
                | _ => {}
            }
            Ok(block)
        })
        .collect()
}

fn reference_targets(value: &Value, targets: &mut HashSet<String>) {
    match value {
        Value::Object(node) => {
            if node.get("name").and_then(Value::as_str) == Some("ref")
                && let Some(target) = node.get("target").and_then(Value::as_str)
            {
                targets.insert(target.rsplit_once('#').map_or(target, |(_, id)| id).into());
            }
            for value in node.values() {
                reference_targets(value, targets);
            }
        }
        Value::Array(values) => {
            for value in values {
                reference_targets(value, targets);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn omit_generated_ids(value: &mut Value, generated: &HashSet<&str>, required: &HashSet<String>) {
    match value {
        Value::Object(node) => {
            if node.get("name").and_then(Value::as_str) == Some("section")
                && node
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| generated.contains(id) && !required.contains(id))
            {
                node.remove("id");
            }
            for value in node.values_mut() {
                omit_generated_ids(value, generated, required);
            }
        }
        Value::Array(values) => {
            for value in values {
                omit_generated_ids(value, generated, required);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use acdc_parser::{CompatibilityMode, Options, asg, parse};

    #[test]
    fn projection_preserves_the_original_tree_and_prepared_catalogs()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(
            "= Doc\n\n== Section\n\n[#code]\n.Title\n----\nCode\n----\n\nSee <<code>> and <<_section>>.",
            &Options::builder()
                .with_compatibility_mode(CompatibilityMode::Draft)
                .build()?,
        )?;
        let document = parsed.document();
        let native = serde_json::to_value(document)?;
        let rich = asg::export_document_with_profile(document, asg::Profile::Acdc)?;
        let smaller = super::document(document)?;
        assert_eq!(
            smaller.pointer("/blocks/0/id"),
            Some(&serde_json::json!("_section"))
        );
        assert!(smaller.pointer("/blocks/0/blocks/0/caption").is_none());
        assert_eq!(native, serde_json::to_value(document)?);
        assert_eq!(
            rich,
            asg::export_document_with_profile(document, asg::Profile::Acdc)?
        );
        assert!(
            matches!(asg::export_document(document), Err(asg::Error::Unsupported { path, .. }) if path.ends_with("/caption"))
        );
        Ok(())
    }
}
