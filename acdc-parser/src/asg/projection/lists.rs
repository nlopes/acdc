use crate::{
    BlockMetadata, CalloutList, CalloutListItem, CalloutRefKind, DescriptionList,
    DescriptionListItem, ListItem, Title,
};

use super::{Error, Export, Node, Target, inlines, metadata, node, pointer, unsupported};
use serde_json::Value;

impl<'a, 'doc> Export<'a, 'doc> {
    pub(super) fn list(
        &mut self,
        values: &'a [ListItem<'doc>],
        marker: &str,
        variant: &str,
        path: &str,
    ) -> Result<Node, Error> {
        require_items(values, path)?;
        if marker.is_empty() {
            return Err(unsupported(
                &pointer(path, "marker"),
                "a list needs a known marker",
            ));
        }
        let items = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let at = pointer(&pointer(path, "items"), &index.to_string());
                self.list_item(value, &at)
                    .map_err(|error| error.with_context("list item", &value.location))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut result = node("list", "block");
        result.insert("marker".into(), marker.into());
        result.insert("variant".into(), variant.into());
        result.insert("items".into(), Value::Array(items));
        Ok(result)
    }

    fn list_item(&mut self, value: &'a ListItem<'doc>, path: &str) -> Result<Value, Error> {
        if value.checked.is_some() {
            return Err(unsupported(
                &pointer(path, "checked"),
                "checklist state has no supported upstream field",
            ));
        }
        if ListItem::parse_depth_from_marker(value.marker) != Some(usize::from(value.level)) {
            return Err(unsupported(
                &pointer(path, "level"),
                "the item level cannot be recovered from its marker",
            ));
        }
        let mut result = node("listItem", "block");
        result.insert("marker".into(), value.marker.into());
        result.insert(
            "principal".into(),
            inlines(&value.principal, &pointer(path, "principal"))?,
        );
        result.insert(
            "blocks".into(),
            self.nonsection_blocks(&value.blocks, &pointer(path, "blocks"))?,
        );
        metadata::location(&mut result, &value.location, path)?;
        Ok(Value::Object(result))
    }

    pub(super) fn list_metadata(
        &mut self,
        result: &mut Node,
        value: &'a BlockMetadata<'doc>,
        title: &'a Title<'doc>,
        path: &str,
    ) -> Result<(), Error> {
        metadata::apply(result, value, path)?;
        metadata::title(result, title, path)?;
        self.target(value, title, None, path)
    }

    pub(super) fn callout_list(
        &mut self,
        value: &'a CalloutList<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        let first = require_items(&value.items, path)?;
        let mut result = node("list", "block");
        result.insert("marker".into(), callout_marker(first).into());
        result.insert("variant".into(), "callout".into());
        let items = value
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let at = pointer(&pointer(path, "items"), &index.to_string());
                self.callout_item(item, index, &at)
                    .map_err(|error| error.with_context("callout list item", &item.location))
            })
            .collect::<Result<Vec<_>, _>>()?;
        result.insert("items".into(), Value::Array(items));
        self.list_metadata(&mut result, &value.metadata, &value.title, path)?;
        Ok(result)
    }

    fn callout_item(
        &mut self,
        value: &'a CalloutListItem<'doc>,
        index: usize,
        path: &str,
    ) -> Result<Value, Error> {
        if value.callout.number == 0
            || matches!(value.callout.kind, CalloutRefKind::Auto)
                && value.callout.number != index + 1
        {
            return Err(unsupported(
                &pointer(&pointer(path, "callout"), "number"),
                "an automatic callout number must be recoverable from item order",
            ));
        }
        let mut result = node("listItem", "block");
        result.insert("marker".into(), callout_marker(value).into());
        result.insert(
            "principal".into(),
            inlines(&value.principal, &pointer(path, "principal"))?,
        );
        result.insert(
            "blocks".into(),
            self.nonsection_blocks(&value.blocks, &pointer(path, "blocks"))?,
        );
        metadata::location(&mut result, &value.location, path)?;
        Ok(Value::Object(result))
    }

    pub(super) fn description_list(
        &mut self,
        value: &'a DescriptionList<'doc>,
        path: &str,
    ) -> Result<Node, Error> {
        let first = require_items(&value.items, path)?;
        let mut result = node("dlist", "block");
        result.insert("marker".into(), first.delimiter.into());
        let items = value
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let at = pointer(&pointer(path, "items"), &index.to_string());
                self.description_item(item, &at)
                    .map_err(|error| error.with_context("description list item", &item.location))
            })
            .collect::<Result<Vec<_>, _>>()?;
        result.insert("items".into(), Value::Array(items));
        self.list_metadata(&mut result, &value.metadata, &value.title, path)?;
        Ok(result)
    }

    fn description_item(
        &mut self,
        value: &'a DescriptionListItem<'doc>,
        path: &str,
    ) -> Result<Value, Error> {
        if value.delimiter.is_empty() {
            return Err(unsupported(
                &pointer(path, "delimiter"),
                "a description item needs a known delimiter",
            ));
        }
        let mut result = node("dlistItem", "block");
        result.insert("marker".into(), value.delimiter.into());
        result.insert(
            "terms".into(),
            Value::Array(vec![inlines(
                &value.term,
                &pointer(&pointer(path, "terms"), "0"),
            )?]),
        );
        result.insert(
            "principal".into(),
            inlines(&value.principal_text, &pointer(path, "principal"))?,
        );
        result.insert(
            "blocks".into(),
            self.nonsection_blocks(&value.description, &pointer(path, "blocks"))?,
        );
        if value.anchors.len() > 1 {
            return Err(unsupported(
                &pointer(path, "anchors"),
                "multiple item identities need a lossless mapping",
            ));
        }
        if let Some(anchor) = value.anchors.first() {
            if anchor.is_bibliography() {
                return Err(unsupported(
                    &pointer(path, "anchors"),
                    "bibliography identity has no supported upstream mapping",
                ));
            }
            result.insert("id".into(), anchor.id.into());
            if let Some(label) = &anchor.xreflabel {
                result.insert("reftext".into(), inlines(label, &pointer(path, "reftext"))?);
            }
            self.insert_target(
                anchor.id.into(),
                Target {
                    title: None,
                    label: anchor.xreflabel.as_ref(),
                    section: None,
                },
                path,
            )?;
        }
        metadata::location(&mut result, &value.location, path)?;
        Ok(Value::Object(result))
    }
}

fn require_items<'a, T>(values: &'a [T], path: &str) -> Result<&'a T, Error> {
    values
        .first()
        .ok_or_else(|| unsupported(&pointer(path, "items"), "a list requires at least one item"))
}

fn callout_marker(value: &CalloutListItem<'_>) -> String {
    match value.callout.kind {
        CalloutRefKind::Explicit => format!("<{}>", value.callout.number),
        CalloutRefKind::Auto => "<.>".into(),
    }
}
