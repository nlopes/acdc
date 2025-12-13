use std::collections::HashSet;

use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer},
    ser::{Error as _, SerializeMap, Serializer},
};

pub(crate) mod converter;
mod macros;
mod text;

pub use converter::inlines_to_string;
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
                map.serialize_entry("name", "raw")?;
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

// =============================================================================
// InlineNode Deserialization Infrastructure
// =============================================================================

/// Raw field collector for `InlineNode` deserialization.
#[derive(Default, Deserialize)]
#[serde(default)]
struct RawInlineFields {
    name: Option<String>,
    r#type: Option<String>,
    value: Option<String>,
    variant: Option<String>,
    form: Option<Form>,
    location: Option<crate::Location>,
    inlines: Option<Vec<InlineNode>>,
    title: Option<Vec<InlineNode>>,
    target: Option<serde_json::Value>,
    attributes: Option<ElementAttributes>,
    role: Option<String>,
    id: Option<String>,
    text: Option<String>,
    items: Option<Vec<String>>,
    keys: Option<Vec<String>>,
    label: Option<String>,
    content: Option<String>,
    notation: Option<StemNotation>,
    substitutions: Option<HashSet<crate::Substitution>>,
    xreflabel: Option<String>,
}

// -----------------------------------------------------------------------------
// Per-variant InlineNode constructors
// -----------------------------------------------------------------------------

fn construct_plain_text<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    Ok(InlineNode::PlainText(Plain {
        content: raw.value.ok_or_else(|| E::missing_field("value"))?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_raw_text<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    Ok(InlineNode::RawText(Raw {
        content: raw.value.ok_or_else(|| E::missing_field("value"))?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_verbatim_text<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    Ok(InlineNode::VerbatimText(Verbatim {
        content: raw.value.ok_or_else(|| E::missing_field("value"))?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_standalone_curved_apostrophe<E: de::Error>(
    raw: RawInlineFields,
) -> Result<InlineNode, E> {
    Ok(InlineNode::StandaloneCurvedApostrophe(
        StandaloneCurvedApostrophe {
            location: raw.location.ok_or_else(|| E::missing_field("location"))?,
        },
    ))
}

fn construct_line_break<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    Ok(InlineNode::LineBreak(LineBreak {
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_anchor<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    Ok(InlineNode::InlineAnchor(Anchor {
        id: raw.id.ok_or_else(|| E::missing_field("id"))?,
        xreflabel: raw.xreflabel,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_icon<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    let target_val = raw.target.ok_or_else(|| E::missing_field("target"))?;
    let target: Source = serde_json::from_value(target_val).map_err(E::custom)?;
    Ok(InlineNode::Macro(InlineMacro::Icon(Icon {
        attributes: raw.attributes.unwrap_or_default(),
        target,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    })))
}

fn construct_image<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    let title = raw.title.ok_or_else(|| E::missing_field("title"))?;
    let target_val = raw.target.ok_or_else(|| E::missing_field("target"))?;
    let source: Source = serde_json::from_value(target_val).map_err(E::custom)?;
    Ok(InlineNode::Macro(InlineMacro::Image(Box::new(Image {
        title,
        source,
        metadata: BlockMetadata::default(),
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))))
}

fn construct_footnote<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    let inlines = raw.inlines.ok_or_else(|| E::missing_field("inlines"))?;
    Ok(InlineNode::Macro(InlineMacro::Footnote(Footnote {
        id: raw.id,
        content: inlines,
        number: 0,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    })))
}

fn construct_keyboard<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    Ok(InlineNode::Macro(InlineMacro::Keyboard(Keyboard {
        keys: raw.keys.ok_or_else(|| E::missing_field("keys"))?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    })))
}

fn construct_button<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    Ok(InlineNode::Macro(InlineMacro::Button(Button {
        label: raw.label.ok_or_else(|| E::missing_field("label"))?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    })))
}

fn construct_menu<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    let target_val = raw.target.ok_or_else(|| E::missing_field("target"))?;
    let target: String = serde_json::from_value(target_val).map_err(E::custom)?;
    Ok(InlineNode::Macro(InlineMacro::Menu(Menu {
        target,
        items: raw.items.unwrap_or_default(),
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    })))
}

fn construct_stem<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    Ok(InlineNode::Macro(InlineMacro::Stem(Stem {
        content: raw.content.ok_or_else(|| E::missing_field("content"))?,
        notation: raw.notation.ok_or_else(|| E::missing_field("notation"))?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    })))
}

fn construct_xref<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    let target_val = raw.target.ok_or_else(|| E::missing_field("target"))?;
    let target: String = serde_json::from_value(target_val).map_err(E::custom)?;
    Ok(InlineNode::Macro(InlineMacro::CrossReference(
        crate::model::CrossReference {
            target,
            text: raw.text,
            location: raw.location.ok_or_else(|| E::missing_field("location"))?,
        },
    )))
}

fn construct_ref<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    let variant = raw.variant.ok_or_else(|| E::missing_field("variant"))?;
    let target_val = raw.target.ok_or_else(|| E::missing_field("target"))?;
    let target: Source = serde_json::from_value(target_val).map_err(E::custom)?;
    let location = raw.location.ok_or_else(|| E::missing_field("location"))?;

    match variant.as_str() {
        "url" => Ok(InlineNode::Macro(InlineMacro::Url(Url {
            text: vec![],
            attributes: raw.attributes.unwrap_or_default(),
            target,
            location,
        }))),
        "link" => Ok(InlineNode::Macro(InlineMacro::Link(Link {
            text: None,
            attributes: raw.attributes.unwrap_or_default(),
            target,
            location,
        }))),
        "autolink" => Ok(InlineNode::Macro(InlineMacro::Autolink(Autolink {
            url: target,
            location,
        }))),
        "pass" => Ok(InlineNode::Macro(InlineMacro::Pass(Pass {
            text: raw.text,
            substitutions: raw.substitutions.unwrap_or_default(),
            location,
            kind: PassthroughKind::default(),
        }))),
        _ => {
            tracing::error!(variant = %variant, "invalid inline macro variant");
            Err(E::custom("invalid inline macro variant"))
        }
    }
}

fn construct_span<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    let variant = raw.variant.ok_or_else(|| E::missing_field("variant"))?;
    let inlines = raw.inlines.ok_or_else(|| E::missing_field("inlines"))?;
    let location = raw.location.ok_or_else(|| E::missing_field("location"))?;
    let role = raw.role;
    let id = raw.id;

    match variant.as_str() {
        "strong" => Ok(InlineNode::BoldText(Bold {
            role,
            id,
            form: raw.form.unwrap_or(Form::Constrained),
            content: inlines,
            location,
        })),
        "emphasis" => Ok(InlineNode::ItalicText(Italic {
            role,
            id,
            form: raw.form.unwrap_or(Form::Constrained),
            content: inlines,
            location,
        })),
        "code" => Ok(InlineNode::MonospaceText(Monospace {
            role,
            id,
            form: raw.form.unwrap_or(Form::Constrained),
            content: inlines,
            location,
        })),
        "mark" => Ok(InlineNode::HighlightText(Highlight {
            role,
            id,
            form: raw.form.unwrap_or(Form::Constrained),
            content: inlines,
            location,
        })),
        "subscript" => Ok(InlineNode::SubscriptText(Subscript {
            role,
            id,
            form: raw.form.unwrap_or(Form::Unconstrained),
            content: inlines,
            location,
        })),
        "superscript" => Ok(InlineNode::SuperscriptText(Superscript {
            role,
            id,
            form: raw.form.unwrap_or(Form::Unconstrained),
            content: inlines,
            location,
        })),
        "curved_quotation" => Ok(InlineNode::CurvedQuotationText(CurvedQuotation {
            role,
            id,
            form: raw.form.unwrap_or(Form::Unconstrained),
            content: inlines,
            location,
        })),
        "curved_apostrophe" => Ok(InlineNode::CurvedApostropheText(CurvedApostrophe {
            role,
            id,
            form: raw.form.unwrap_or(Form::Unconstrained),
            content: inlines,
            location,
        })),
        _ => {
            tracing::error!(variant = %variant, "invalid inline node variant");
            Err(E::custom("invalid inline node variant"))
        }
    }
}

/// Dispatch to the appropriate `InlineNode` constructor based on name/type
fn dispatch_inline<E: de::Error>(raw: RawInlineFields) -> Result<InlineNode, E> {
    let name = raw.name.clone().ok_or_else(|| E::missing_field("name"))?;
    let ty = raw.r#type.clone().ok_or_else(|| E::missing_field("type"))?;

    match (name.as_str(), ty.as_str()) {
        ("text", "string") => construct_plain_text(raw),
        ("raw", "string") => construct_raw_text(raw),
        ("verbatim", "string") => construct_verbatim_text(raw),
        ("curved_apostrophe", "string") => construct_standalone_curved_apostrophe(raw),
        ("break", "inline") => construct_line_break(raw),
        ("anchor", "inline") => construct_anchor(raw),
        ("icon", "inline") => construct_icon(raw),
        ("image", "inline") => construct_image(raw),
        ("footnote", "inline") => construct_footnote(raw),
        ("keyboard", "inline") => construct_keyboard(raw),
        ("btn" | "button", "inline") => construct_button(raw),
        ("menu", "inline") => construct_menu(raw),
        ("stem", "inline") => construct_stem(raw),
        ("xref", "inline") => construct_xref(raw),
        ("ref", "inline") => construct_ref(raw),
        ("span", "inline") => construct_span(raw),
        _ => {
            tracing::error!(name = %name, r#type = %ty, "invalid inline node");
            Err(E::custom("invalid inline node"))
        }
    }
}

impl<'de> Deserialize<'de> for InlineNode {
    fn deserialize<D>(deserializer: D) -> Result<InlineNode, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: RawInlineFields = RawInlineFields::deserialize(deserializer)?;
        dispatch_inline(raw)
    }
}
