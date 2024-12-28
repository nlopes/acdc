use std::collections::HashMap;

use serde::{
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
    Deserialize, Serialize,
};

mod macros;
mod text;

pub use macros::*;
pub use text::*;

use crate::{
    model::{Image, ImageSource},
    BlockMetadata,
};

/// An `InlineNode` represents an inline node in a document.
///
/// An inline node is a structural element in a document that can contain other inline
/// nodes and are only valid within a paragraph (a leaf).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum InlineNode {
    PlainText(Plain),
    BoldText(Bold),
    ItalicText(Italic),
    MonospaceText(Monospace),
    HighlightText(Highlight),
    SubscriptText(Subscript),
    SuperscriptText(Superscript),
    LineBreak(LineBreak),
    Macro(InlineMacro),
}

impl InlineNode {
    pub fn shift_start_location(&mut self, line: usize, column: usize) {
        match self {
            InlineNode::PlainText(plain) => plain.location.shift_line_column(line, column),
            InlineNode::BoldText(bold) => bold.location.shift_line_column(line, column),
            InlineNode::ItalicText(italic) => italic.location.shift_line_column(line, column),
            InlineNode::MonospaceText(monospace) => {
                monospace.location.shift_line_column(line, column);
            }
            InlineNode::HighlightText(highlight) => {
                highlight.location.shift_line_column(line, column);
            }
            InlineNode::SubscriptText(subscript) => {
                subscript.location.shift_line_column(line, column);
            }
            InlineNode::SuperscriptText(superscript) => {
                superscript.location.shift_line_column(line, column);
            }
            InlineNode::LineBreak(linebreak) => linebreak.location.shift_line_column(line, column),
            InlineNode::Macro(macro_) => match macro_ {
                InlineMacro::Icon(icon) => icon.location.shift_line_column(line, column),
                InlineMacro::Image(image) => image.location.shift_line_column(line, column),
                InlineMacro::Keyboard(keyboard) => {
                    keyboard.location.shift_line_column(line, column);
                }
                InlineMacro::Button(button) => button.location.shift_line_column(line, column),
                InlineMacro::Menu(menu) => menu.location.shift_line_column(line, column),
                InlineMacro::Url(url) => url.location.shift_line_column(line, column),
                InlineMacro::Link(inline_link) => {
                    inline_link.location.shift_line_column(line, column);
                }
                InlineMacro::Autolink(autolink) => {
                    autolink.location.shift_line_column(line, column);
                }
                InlineMacro::Pass(pass) => pass.location.shift_line_column(line, column),
            },
        }
    }
}

/// An `InlineMacro` represents an inline macro in a document.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum InlineMacro {
    Icon(Icon),
    Image(Box<Image>),
    Keyboard(Keyboard),
    Button(Button),
    Menu(Menu),
    Url(Url),
    Link(Link),
    Autolink(Autolink),
    Pass(Pass),
}

impl Serialize for InlineNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;

        match self {
            InlineNode::PlainText(plain) => {
                map.serialize_entry("name", "text")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", &plain.content)?;
                map.serialize_entry("location", &plain.location)?;
            }
            InlineNode::HighlightText(highlight) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "mark")?;
                map.serialize_entry("form", "constrained")?;
                if let Some(role) = &highlight.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &highlight.content)?;
                map.serialize_entry("location", &highlight.location)?;
            }
            InlineNode::ItalicText(italic) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "emphasis")?;
                map.serialize_entry("form", "constrained")?;
                if let Some(role) = &italic.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &italic.content)?;
                map.serialize_entry("location", &italic.location)?;
            }
            InlineNode::BoldText(bold) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "strong")?;
                map.serialize_entry("form", "constrained")?;
                if let Some(role) = &bold.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &bold.content)?;
                map.serialize_entry("location", &bold.location)?;
            }
            InlineNode::MonospaceText(monospace) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "code")?;
                if let Some(role) = &monospace.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &monospace.content)?;
                map.serialize_entry("location", &monospace.location)?;
            }
            InlineNode::SubscriptText(subscript) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "subscript")?;
                map.serialize_entry("form", "constrained")?;
                if let Some(role) = &subscript.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &subscript.content)?;
                map.serialize_entry("location", &subscript.location)?;
            }
            InlineNode::SuperscriptText(superscript) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "superscript")?;
                map.serialize_entry("form", "constrained")?;
                if let Some(role) = &superscript.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &superscript.content)?;
                map.serialize_entry("location", &superscript.location)?;
            }
            InlineNode::LineBreak(line_break) => {
                map.serialize_entry("name", "break")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("location", &line_break.location)?;
            }
            InlineNode::Macro(macro_node) => {
                todo!(
                    "implement macro serialization for InlineNode: {:?}",
                    macro_node
                )
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for InlineNode {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<InlineNode, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = InlineNode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<InlineNode, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_name = None;
                let mut my_type = None;
                let mut my_value = None;
                let mut my_variant = None;
                let mut my_location = None;
                let mut my_inlines = None;
                let mut my_title = None;
                let mut my_target = None;

                // TODO(nlopes): need to deserialize the attributes!
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if my_name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            my_name = Some(map.next_value::<String>()?);
                        }
                        "type" => {
                            if my_type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            my_type = Some(map.next_value::<String>()?);
                        }
                        "value" => {
                            if my_value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            my_value = Some(map.next_value()?);
                        }
                        "location" => {
                            if my_location.is_some() {
                                return Err(de::Error::duplicate_field("location"));
                            }
                            my_location = Some(map.next_value()?);
                        }
                        "variant" => {
                            if my_variant.is_some() {
                                return Err(de::Error::duplicate_field("variant"));
                            }
                            my_variant = Some(map.next_value::<String>()?);
                        }
                        "title" => {
                            if my_title.is_some() {
                                return Err(de::Error::duplicate_field("title"));
                            }
                            my_title = Some(map.next_value()?);
                        }
                        "target" => {
                            if my_target.is_some() {
                                return Err(de::Error::duplicate_field("target"));
                            }
                            my_target = Some(map.next_value()?);
                        }
                        "inlines" => {
                            if my_inlines.is_some() {
                                return Err(de::Error::duplicate_field("inlines"));
                            }
                            my_inlines = Some(map.next_value::<Vec<InlineNode>>()?);
                        }
                        _ => {
                            // Ignore any other fields
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let my_name = my_name.ok_or_else(|| de::Error::missing_field("name"))?;
                let my_type = my_type.ok_or_else(|| de::Error::missing_field("type"))?;
                let my_location =
                    my_location.ok_or_else(|| de::Error::missing_field("location"))?;

                match (my_name.as_str(), my_type.as_str()) {
                    ("linebreak", "string") => Ok(InlineNode::LineBreak(LineBreak {
                        location: my_location,
                    })),
                    ("text", "string") => {
                        let my_value = my_value.ok_or_else(|| de::Error::missing_field("value"))?;

                        Ok(InlineNode::PlainText(Plain {
                            content: my_value,
                            location: my_location,
                        }))
                    }
                    ("icon", "inline") => {
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        Ok(InlineNode::Macro(InlineMacro::Icon(Icon {
                            attributes: HashMap::new(),
                            target: my_target,
                            location: my_location,
                        })))
                    }
                    ("image", "inline") => {
                        let my_title = my_title.ok_or_else(|| de::Error::missing_field("title"))?;
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        Ok(InlineNode::Macro(InlineMacro::Image(Box::new(Image {
                            title: my_title,
                            source: ImageSource::Path(my_target),
                            metadata: BlockMetadata::default(),
                            location: my_location,
                        }))))
                    }
                    ("keyboard", "inline") => todo!("implement keyboard deserialization"),
                    ("btn" | "button", "inline") => {
                        todo!("implement button deserialization")
                    }
                    ("menu", "inline") => todo!("implement menu deserialization"),
                    ("ref", "inline") => {
                        let my_variant =
                            my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        match my_variant.as_str() {
                            "url" => Ok(InlineNode::Macro(InlineMacro::Url(Url {
                                attributes: HashMap::new(),
                                target: my_target,
                                location: my_location,
                            }))),
                            "link" => todo!("implement link deserialization"),
                            "autolink" => todo!("implement autolink deserialization"),
                            "pass" => todo!("implement pass deserialization"),
                            _ => {
                                tracing::error!(variant = %my_variant, "invalid inline macro variant");
                                Err(de::Error::custom("invalid inline macro variant"))
                            }
                        }
                    }
                    ("span", "inline") => {
                        let my_variant =
                            my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                        let my_inlines =
                            my_inlines.ok_or_else(|| de::Error::missing_field("inlines"))?;
                        match my_variant.as_str() {
                            "strong" => Ok(InlineNode::BoldText(Bold {
                                role: None,
                                content: my_inlines,
                                location: my_location,
                            })),
                            "emphasis" => Ok(InlineNode::ItalicText(Italic {
                                role: None,
                                content: my_inlines,
                                location: my_location,
                            })),
                            "code" => Ok(InlineNode::MonospaceText(Monospace {
                                role: None,
                                content: my_inlines,
                                location: my_location,
                            })),
                            "mark" => Ok(InlineNode::HighlightText(Highlight {
                                role: None,
                                content: my_inlines,
                                location: my_location,
                            })),
                            "subscript" => Ok(InlineNode::SubscriptText(Subscript {
                                role: None,
                                content: my_inlines,
                                location: my_location,
                            })),
                            "superscript" => Ok(InlineNode::SuperscriptText(Superscript {
                                role: None,
                                content: my_inlines,
                                location: my_location,
                            })),
                            _ => {
                                tracing::error!(variant = %my_variant, "invalid inline node variant");
                                Err(de::Error::custom("invalid inline node variant"))
                            }
                        }
                    }
                    _ => {
                        tracing::error!(name = %my_name, r#type = %my_type, "invalid inline node");
                        Err(de::Error::custom("invalid inline node"))
                    }
                }
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}
