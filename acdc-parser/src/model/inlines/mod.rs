use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
};

mod macros;
mod text;

pub use macros::*;
pub use text::*;

use crate::{BlockMetadata, ElementAttributes, Image, Location, Source};

/// An `InlineNode` represents an inline node in a document.
///
/// An inline node is a structural element in a document that can contain other inline
/// nodes and are only valid within a paragraph (a leaf).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum InlineNode {
    PlainText(Plain),
    RawText(Raw),
    BoldText(Bold),
    ItalicText(Italic),
    MonospaceText(Monospace),
    HighlightText(Highlight),
    SubscriptText(Subscript),
    SuperscriptText(Superscript),
    CurvedQuotationText(CurvedQuotation),
    CurvedApostropheText(CurvedApostrophe),
    StandaloneCurvedApostrophe(StandaloneCurvedApostrophe),
    LineBreak(LineBreak),
    Macro(InlineMacro),

    // Internal use only - DO NOT USE unless you're inside the parser
    _PlaceholderContent(PlaceholderContent),
}

impl InlineNode {
    #[must_use]
    pub fn location(&self) -> Location {
        match self {
            InlineNode::PlainText(plain) => plain.location.clone(),
            InlineNode::RawText(raw) => raw.location.clone(),
            InlineNode::BoldText(bold) => bold.location.clone(),
            InlineNode::ItalicText(italic) => italic.location.clone(),
            InlineNode::MonospaceText(monospace) => monospace.location.clone(),
            InlineNode::HighlightText(highlight) => highlight.location.clone(),
            InlineNode::SubscriptText(subscript) => subscript.location.clone(),
            InlineNode::SuperscriptText(superscript) => superscript.location.clone(),
            InlineNode::CurvedQuotationText(curved_quotation) => curved_quotation.location.clone(),
            InlineNode::CurvedApostropheText(curved_apostrophe) => {
                curved_apostrophe.location.clone()
            }
            InlineNode::StandaloneCurvedApostrophe(standalone) => standalone.location.clone(),
            InlineNode::LineBreak(line_break) => line_break.location.clone(),
            InlineNode::Macro(macro_node) => match macro_node {
                InlineMacro::Icon(icon) => icon.location.clone(),
                InlineMacro::Image(image) => image.location.clone(),
                InlineMacro::Keyboard(keyboard) => keyboard.location.clone(),
                InlineMacro::Button(button) => button.location.clone(),
                InlineMacro::Menu(menu) => menu.location.clone(),
                InlineMacro::Url(url) => url.location.clone(),
                InlineMacro::Link(link) => link.location.clone(),
                InlineMacro::Autolink(autolink) => autolink.location.clone(),
                InlineMacro::Pass(pass) => pass.location.clone(),
            },
            InlineNode::_PlaceholderContent(placeholder) => placeholder.location.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaceholderContent {
    pub(crate) content: Pass,
    pub(crate) location: crate::Location,
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
    #[allow(clippy::too_many_lines)]
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
            InlineNode::RawText(raw) => {
                map.serialize_entry("name", "raw")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", &raw.content)?;
                map.serialize_entry("location", &raw.location)?;
            }
            InlineNode::HighlightText(highlight) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "mark")?;
                map.serialize_entry("form", &highlight.form)?;
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
                map.serialize_entry("form", &italic.form)?;
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
                map.serialize_entry("form", &bold.form)?;
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
                map.serialize_entry("form", &monospace.form)?;
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
                map.serialize_entry("form", &subscript.form)?;
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
                map.serialize_entry("form", &superscript.form)?;
                if let Some(role) = &superscript.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &superscript.content)?;
                map.serialize_entry("location", &superscript.location)?;
            }
            InlineNode::CurvedQuotationText(curved_quotation) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "curved_quotation")?;
                map.serialize_entry("form", &curved_quotation.form)?;
                if let Some(role) = &curved_quotation.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &curved_quotation.content)?;
                map.serialize_entry("location", &curved_quotation.location)?;
            }
            InlineNode::CurvedApostropheText(curved_apostrophe) => {
                map.serialize_entry("name", "span")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", "curved_apostrophe")?;
                map.serialize_entry("form", &curved_apostrophe.form)?;
                if let Some(role) = &curved_apostrophe.role {
                    map.serialize_entry("role", role)?;
                }
                map.serialize_entry("inlines", &curved_apostrophe.content)?;
                map.serialize_entry("location", &curved_apostrophe.location)?;
            }
            InlineNode::StandaloneCurvedApostrophe(standalone) => {
                map.serialize_entry("name", "curved_apostrophe")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("location", &standalone.location)?;
            }
            InlineNode::LineBreak(line_break) => {
                map.serialize_entry("name", "break")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("location", &line_break.location)?;
            }
            InlineNode::Macro(macro_node) => {
                serialize_inline_macro::<S>(macro_node, &mut map)?;
            }
            InlineNode::_PlaceholderContent(placeholder) => {
                unreachable!(
                    "PlaceholderContent must not be serialized: {:?}",
                    placeholder
                )
            }
        }
        map.end()
    }
}

fn serialize_inline_macro<S>(
    macro_node: &InlineMacro,
    map: &mut S::SerializeMap,
) -> Result<(), S::Error>
where
    S: Serializer,
{
    match macro_node {
        InlineMacro::Icon(icon) => {
            map.serialize_entry("name", "icon")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("target", &icon.target)?;
            if !icon.attributes.is_empty() {
                map.serialize_entry("attributes", &icon.attributes)?;
            }
            map.serialize_entry("location", &icon.location)?;
        }
        InlineMacro::Image(image) => {
            map.serialize_entry("name", "image")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("title", &image.title)?;
            map.serialize_entry("target", &image.source)?;
            map.serialize_entry("location", &image.location)?;
        }
        InlineMacro::Keyboard(keyboard) => {
            map.serialize_entry("name", "keyboard")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("location", &keyboard.location)?;
        }
        InlineMacro::Button(button) => {
            map.serialize_entry("name", "button")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("location", &button.location)?;
        }
        InlineMacro::Menu(menu) => {
            map.serialize_entry("name", "menu")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("location", &menu.location)?;
        }
        InlineMacro::Url(url) => {
            map.serialize_entry("name", "ref")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("variant", "link")?;
            map.serialize_entry("target", &url.target)?;
            map.serialize_entry("location", &url.location)?;
            map.serialize_entry("attributes", &url.attributes)?;
        }
        InlineMacro::Link(link) => {
            map.serialize_entry("name", "ref")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("variant", "link")?;
            map.serialize_entry("target", &link.target)?;
            map.serialize_entry("location", &link.location)?;
            map.serialize_entry("attributes", &link.attributes)?;
        }
        InlineMacro::Autolink(autolink) => {
            map.serialize_entry("name", "ref")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("variant", "autolink")?;
            map.serialize_entry("target", &autolink.url)?;
            map.serialize_entry("location", &autolink.location)?;
        }
        InlineMacro::Pass(_) => {
            unimplemented!(
                "passthrough serialization is not implemented because we only serialize to ASG what should be visible to the user"
            )
        }
    }
    Ok(())
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
                let mut my_form = None;
                let mut my_location = None;
                let mut my_inlines = None;
                let mut my_title = None;
                let mut my_target = None;
                let mut my_attributes = None;
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
                            my_title = Some(map.next_value::<Vec<InlineNode>>()?);
                            //my_title = Some(map.next_value()?);
                        }
                        "target" => {
                            if my_target.is_some() {
                                return Err(de::Error::duplicate_field("target"));
                            }
                            my_target = Some(map.next_value::<Source>()?);
                        }
                        "form" => {
                            if my_form.is_some() {
                                return Err(de::Error::duplicate_field("form"));
                            }
                            my_form = Some(map.next_value::<Form>()?);
                        }
                        "inlines" => {
                            if my_inlines.is_some() {
                                return Err(de::Error::duplicate_field("inlines"));
                            }
                            my_inlines = Some(map.next_value::<Vec<InlineNode>>()?);
                        }
                        "attributes" => {
                            if my_attributes.is_some() {
                                return Err(de::Error::duplicate_field("attributes"));
                            }
                            my_attributes = Some(map.next_value::<ElementAttributes>()?);
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
                    ("raw", "string") => {
                        let my_value = my_value.ok_or_else(|| de::Error::missing_field("value"))?;

                        Ok(InlineNode::RawText(Raw {
                            content: my_value,
                            location: my_location,
                        }))
                    }
                    ("curved_apostrophe", "string") => Ok(InlineNode::StandaloneCurvedApostrophe(
                        StandaloneCurvedApostrophe {
                            location: my_location,
                        },
                    )),
                    ("icon", "inline") => {
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        Ok(InlineNode::Macro(InlineMacro::Icon(Icon {
                            attributes: my_attributes.unwrap_or_default(),
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
                            source: my_target,
                            metadata: BlockMetadata::default(),
                            location: my_location,
                        }))))
                    }
                    ("keyboard", "inline") => {
                        Ok(InlineNode::Macro(InlineMacro::Keyboard(Keyboard {
                            keys: vec![], // Simplified deserialization, keys not stored in fixture format
                            location: my_location,
                        })))
                    }
                    ("btn" | "button", "inline") => {
                        Ok(InlineNode::Macro(InlineMacro::Button(Button {
                            label: String::new(), // Simplified deserialization, label not stored in fixture format
                            location: my_location,
                        })))
                    }
                    ("menu", "inline") => {
                        Ok(InlineNode::Macro(InlineMacro::Menu(Menu {
                            target: String::new(), // Simplified deserialization, target not stored in fixture format
                            items: vec![],
                            location: my_location,
                        })))
                    }
                    ("ref", "inline") => {
                        let my_variant =
                            my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        // TODO(nlopes): need to deserialize the attributes (of which the first positional attribute is the text)!
                        //
                        // Also need to handle the other inline macros!
                        //
                        //
                        match my_variant.as_str() {
                            "url" => todo!(
                                "implement url deserialization - this uses variant 'link' as well so need to be differentiated"
                            ),
                            "link" => Ok(InlineNode::Macro(InlineMacro::Link(Link {
                                text: None,
                                attributes: my_attributes.unwrap_or_default(),
                                target: my_target,
                                location: my_location,
                            }))),

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
                                form: my_form.unwrap_or(Form::Constrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "emphasis" => Ok(InlineNode::ItalicText(Italic {
                                role: None,
                                form: my_form.unwrap_or(Form::Constrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "code" => Ok(InlineNode::MonospaceText(Monospace {
                                role: None,
                                form: my_form.unwrap_or(Form::Constrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "mark" => Ok(InlineNode::HighlightText(Highlight {
                                role: None,
                                form: my_form.unwrap_or(Form::Constrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "subscript" => Ok(InlineNode::SubscriptText(Subscript {
                                role: None,
                                form: my_form.unwrap_or(Form::Unconstrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "superscript" => Ok(InlineNode::SuperscriptText(Superscript {
                                role: None,
                                form: my_form.unwrap_or(Form::Unconstrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "curved_quotation" => {
                                Ok(InlineNode::CurvedQuotationText(CurvedQuotation {
                                    role: None,
                                    form: my_form.unwrap_or(Form::Unconstrained),
                                    content: my_inlines,
                                    location: my_location,
                                }))
                            }
                            "curved_apostrophe" => {
                                Ok(InlineNode::CurvedApostropheText(CurvedApostrophe {
                                    role: None,
                                    form: my_form.unwrap_or(Form::Unconstrained),
                                    content: my_inlines,
                                    location: my_location,
                                }))
                            }
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
