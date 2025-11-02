use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{Error as _, SerializeMap, Serializer},
};

pub(crate) mod converter;
mod macros;
mod text;

pub use macros::*;
pub use text::*;

use crate::{Anchor, BlockMetadata, ElementAttributes, Image, Source, StemNotation};

/// An `InlineNode` represents an inline node in a document.
///
/// An inline node is a structural element in a document that can contain other inline
/// nodes and are only valid within a paragraph (a leaf).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum InlineNode {
    // This is just "normal" text
    PlainText(Plain),
    // This is raw text only found in Delimited Pass blocks
    RawText(Raw),
    // This is verbatim text found in Delimited Literal and Listing blocks
    VerbatimText(Verbatim),
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
    InlineAnchor(Anchor),
    Macro(InlineMacro),
}

/// An `InlineMacro` represents an inline macro in a document.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum InlineMacro {
    Footnote(Footnote),
    Icon(Icon),
    Image(Box<Image>),
    Keyboard(Keyboard),
    Button(Button),
    Menu(Menu),
    Url(Url),
    Link(Link),
    Autolink(Autolink),
    CrossReference(CrossReference),
    Pass(Pass),
    Stem(Stem),
}

/// Macro to serialize inline format types (Bold, Italic, Monospace, etc.)
/// All these types share identical structure and serialization logic.
macro_rules! serialize_inline_format {
    ($map:expr, $value:expr, $variant:literal) => {{
        $map.serialize_entry("name", "span")?;
        $map.serialize_entry("type", "inline")?;
        $map.serialize_entry("variant", $variant)?;
        $map.serialize_entry("form", &$value.form)?;
        if let Some(role) = &$value.role {
            $map.serialize_entry("role", role)?;
        }
        if let Some(id) = &$value.id {
            $map.serialize_entry("id", id)?;
        }
        $map.serialize_entry("inlines", &$value.content)?;
        $map.serialize_entry("location", &$value.location)?;
    }};
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
                // We use "text" here to make sure the TCK passes, even though this is raw
                // text.
                map.serialize_entry("name", "text")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", &raw.content)?;
                map.serialize_entry("location", &raw.location)?;
            }
            InlineNode::VerbatimText(verbatim) => {
                // We use "text" here to make sure the TCK passes, even though this is raw
                // text.
                map.serialize_entry("name", "text")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", &verbatim.content)?;
                map.serialize_entry("location", &verbatim.location)?;
            }
            InlineNode::HighlightText(highlight) => {
                serialize_inline_format!(map, highlight, "mark");
            }
            InlineNode::ItalicText(italic) => {
                serialize_inline_format!(map, italic, "emphasis");
            }
            InlineNode::BoldText(bold) => {
                serialize_inline_format!(map, bold, "strong");
            }
            InlineNode::MonospaceText(monospace) => {
                serialize_inline_format!(map, monospace, "code");
            }
            InlineNode::SubscriptText(subscript) => {
                serialize_inline_format!(map, subscript, "subscript");
            }
            InlineNode::SuperscriptText(superscript) => {
                serialize_inline_format!(map, superscript, "superscript");
            }
            InlineNode::CurvedQuotationText(curved_quotation) => {
                serialize_inline_format!(map, curved_quotation, "curved_quotation");
            }
            InlineNode::CurvedApostropheText(curved_apostrophe) => {
                serialize_inline_format!(map, curved_apostrophe, "curved_apostrophe");
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
            InlineNode::InlineAnchor(anchor) => {
                map.serialize_entry("name", "anchor")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("id", &anchor.id)?;
                if let Some(xreflabel) = &anchor.xreflabel {
                    map.serialize_entry("xreflabel", xreflabel)?;
                }
                map.serialize_entry("location", &anchor.location)?;
            }
            InlineNode::Macro(macro_node) => {
                serialize_inline_macro::<S>(macro_node, &mut map)?;
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
        InlineMacro::Footnote(footnote) => {
            map.serialize_entry("name", "footnote")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("id", &footnote.id)?;
            map.serialize_entry("inlines", &footnote.content)?;
            map.serialize_entry("location", &footnote.location)?;
        }
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
            map.serialize_entry("keys", &keyboard.keys)?;
            map.serialize_entry("location", &keyboard.location)?;
        }
        InlineMacro::Button(button) => {
            map.serialize_entry("name", "button")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("label", &button.label)?;
            map.serialize_entry("location", &button.location)?;
        }
        InlineMacro::Menu(menu) => {
            map.serialize_entry("name", "menu")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("target", &menu.target)?;
            if !menu.items.is_empty() {
                map.serialize_entry("items", &menu.items)?;
            }
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
        InlineMacro::CrossReference(xref) => {
            map.serialize_entry("name", "xref")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("target", &xref.target)?;
            if let Some(text) = &xref.text {
                map.serialize_entry("text", text)?;
            }
            map.serialize_entry("location", &xref.location)?;
        }
        InlineMacro::Pass(_) => {
            return Err(S::Error::custom(
                "inline passthrough macros are not part of the ASG specification and cannot be serialized",
            ));
        }
        InlineMacro::Stem(stem) => {
            map.serialize_entry("name", "stem")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("content", &stem.content)?;
            map.serialize_entry("notation", &stem.notation)?;
            map.serialize_entry("location", &stem.location)?;
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
                let mut my_role = None;
                let mut my_id = None;
                let mut my_text = None;
                let mut my_items = None;
                let mut my_keys = None;
                let mut my_label = None;
                let mut my_content = None;
                let mut my_notation = None;
                let mut my_substitutions = None;

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
                        }
                        "target" => {
                            if my_target.is_some() {
                                return Err(de::Error::duplicate_field("target"));
                            }
                            my_target = Some(map.next_value::<serde_json::Value>()?);
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
                        "role" => {
                            if my_role.is_some() {
                                return Err(de::Error::duplicate_field("role"));
                            }
                            my_role = Some(map.next_value::<Option<String>>()?);
                        }
                        "id" => {
                            if my_id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            my_id = Some(map.next_value::<Option<String>>()?);
                        }
                        "text" => {
                            if my_text.is_some() {
                                return Err(de::Error::duplicate_field("text"));
                            }
                            my_text = Some(map.next_value::<String>()?);
                        }
                        "items" => {
                            if my_items.is_some() {
                                return Err(de::Error::duplicate_field("items"));
                            }
                            my_items = Some(map.next_value::<Vec<String>>()?);
                        }
                        "keys" => {
                            if my_keys.is_some() {
                                return Err(de::Error::duplicate_field("keys"));
                            }
                            my_keys = Some(map.next_value::<Vec<String>>()?);
                        }
                        "label" => {
                            if my_label.is_some() {
                                return Err(de::Error::duplicate_field("label"));
                            }
                            my_label = Some(map.next_value::<String>()?);
                        }
                        "content" => {
                            if my_content.is_some() {
                                return Err(de::Error::duplicate_field("content"));
                            }
                            my_content = Some(map.next_value::<String>()?);
                        }
                        "notation" => {
                            if my_notation.is_some() {
                                return Err(de::Error::duplicate_field("notation"));
                            }
                            my_notation = Some(map.next_value::<StemNotation>()?);
                        }
                        "substitutions" => {
                            if my_substitutions.is_some() {
                                return Err(de::Error::duplicate_field("substitutions"));
                            }
                            my_substitutions = Some(map.next_value()?);
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
                    ("verbatim", "string") => {
                        let my_value = my_value.ok_or_else(|| de::Error::missing_field("value"))?;

                        Ok(InlineNode::VerbatimText(Verbatim {
                            content: my_value,
                            location: my_location,
                        }))
                    }
                    ("curved_apostrophe", "string") => Ok(InlineNode::StandaloneCurvedApostrophe(
                        StandaloneCurvedApostrophe {
                            location: my_location,
                        },
                    )),
                    ("break", "inline") => Ok(InlineNode::LineBreak(LineBreak {
                        location: my_location,
                    })),
                    ("anchor", "inline") => {
                        let id = my_id.ok_or_else(|| de::Error::missing_field("id"))?;
                        Ok(InlineNode::InlineAnchor(Anchor {
                            id: id.ok_or_else(|| de::Error::custom("anchor id cannot be null"))?,
                            xreflabel: None, // xreflabel can be added later if needed
                            location: my_location,
                        }))
                    }
                    ("icon", "inline") => {
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        let target: Source =
                            serde_json::from_value(my_target).map_err(de::Error::custom)?;
                        Ok(InlineNode::Macro(InlineMacro::Icon(Icon {
                            attributes: my_attributes.unwrap_or_default(),
                            target,
                            location: my_location,
                        })))
                    }
                    ("image", "inline") => {
                        let my_title = my_title.ok_or_else(|| de::Error::missing_field("title"))?;
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        let source: Source =
                            serde_json::from_value(my_target).map_err(de::Error::custom)?;
                        Ok(InlineNode::Macro(InlineMacro::Image(Box::new(Image {
                            title: my_title,
                            source,
                            metadata: BlockMetadata::default(),
                            location: my_location,
                        }))))
                    }
                    ("footnote", "inline") => {
                        let my_inlines =
                            my_inlines.ok_or_else(|| de::Error::missing_field("inlines"))?;
                        Ok(InlineNode::Macro(InlineMacro::Footnote(Footnote {
                            id: my_id.flatten(),
                            content: my_inlines,
                            // TODO(nlopes): This will be set by the footnote
                            // tracker during parsing - should serialize and
                            // deserialize it too?
                            number: 0,
                            location: my_location,
                        })))
                    }
                    ("keyboard", "inline") => {
                        let keys = my_keys.ok_or_else(|| de::Error::missing_field("keys"))?;
                        Ok(InlineNode::Macro(InlineMacro::Keyboard(Keyboard {
                            keys,
                            location: my_location,
                        })))
                    }
                    ("btn" | "button", "inline") => {
                        let label = my_label.ok_or_else(|| de::Error::missing_field("label"))?;

                        Ok(InlineNode::Macro(InlineMacro::Button(Button {
                            label,
                            location: my_location,
                        })))
                    }
                    ("menu", "inline") => {
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        let target: String =
                            serde_json::from_value(my_target).map_err(de::Error::custom)?;

                        Ok(InlineNode::Macro(InlineMacro::Menu(Menu {
                            target,
                            items: my_items.unwrap_or_default(),
                            location: my_location,
                        })))
                    }
                    ("stem", "inline") => {
                        let content =
                            my_content.ok_or_else(|| de::Error::missing_field("content"))?;
                        let notation =
                            my_notation.ok_or_else(|| de::Error::missing_field("notation"))?;

                        Ok(InlineNode::Macro(InlineMacro::Stem(Stem {
                            content,
                            notation,
                            location: my_location,
                        })))
                    }
                    ("xref", "inline") => {
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        let target: String =
                            serde_json::from_value(my_target).map_err(de::Error::custom)?;
                        Ok(InlineNode::Macro(InlineMacro::CrossReference(
                            crate::model::CrossReference {
                                target,
                                text: my_text,
                                location: my_location,
                            },
                        )))
                    }
                    ("ref", "inline") => {
                        let my_variant =
                            my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                        let my_target =
                            my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                        let target: Source =
                            serde_json::from_value(my_target).map_err(de::Error::custom)?;
                        // TODO(nlopes): need to deserialize the attributes (of which the first positional attribute is the text)!
                        //
                        // Also need to handle the other inline macros!
                        //
                        //
                        match my_variant.as_str() {
                            "url" => Ok(InlineNode::Macro(InlineMacro::Url(Url {
                                text: vec![],
                                attributes: my_attributes.unwrap_or_default(),
                                target,
                                location: my_location,
                            }))),
                            "link" => Ok(InlineNode::Macro(InlineMacro::Link(Link {
                                text: None,
                                attributes: my_attributes.unwrap_or_default(),
                                target,
                                location: my_location,
                            }))),

                            "autolink" => Ok(InlineNode::Macro(InlineMacro::Autolink(Autolink {
                                url: target,
                                location: my_location,
                            }))),
                            "pass" => Ok(InlineNode::Macro(InlineMacro::Pass(Pass {
                                text: my_text,
                                substitutions: my_substitutions.unwrap_or_default(),
                                location: my_location,
                                kind: PassthroughKind::default(),
                            }))),
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
                                role: my_role.flatten(),
                                id: my_id.flatten(),
                                form: my_form.unwrap_or(Form::Constrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "emphasis" => Ok(InlineNode::ItalicText(Italic {
                                role: my_role.flatten(),
                                id: my_id.flatten(),
                                form: my_form.unwrap_or(Form::Constrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "code" => Ok(InlineNode::MonospaceText(Monospace {
                                role: my_role.flatten(),
                                id: my_id.flatten(),
                                form: my_form.unwrap_or(Form::Constrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "mark" => Ok(InlineNode::HighlightText(Highlight {
                                role: my_role.flatten(),
                                id: my_id.flatten(),
                                form: my_form.unwrap_or(Form::Constrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "subscript" => Ok(InlineNode::SubscriptText(Subscript {
                                role: my_role.flatten(),
                                id: my_id.flatten(),
                                form: my_form.unwrap_or(Form::Unconstrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "superscript" => Ok(InlineNode::SuperscriptText(Superscript {
                                role: my_role.flatten(),
                                id: my_id.flatten(),
                                form: my_form.unwrap_or(Form::Unconstrained),
                                content: my_inlines,
                                location: my_location,
                            })),
                            "curved_quotation" => {
                                Ok(InlineNode::CurvedQuotationText(CurvedQuotation {
                                    role: my_role.flatten(),
                                    id: my_id.flatten(),
                                    form: my_form.unwrap_or(Form::Unconstrained),
                                    content: my_inlines,
                                    location: my_location,
                                }))
                            }
                            "curved_apostrophe" => {
                                Ok(InlineNode::CurvedApostropheText(CurvedApostrophe {
                                    role: my_role.flatten(),
                                    id: my_id.flatten(),
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
