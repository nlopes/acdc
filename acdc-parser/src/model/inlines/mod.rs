use serde::{
    Serialize,
    ser::{Error as _, SerializeMap, Serializer},
};

pub(crate) mod converter;
mod macros;
mod text;

pub use converter::inlines_to_string;
pub use macros::*;
pub use text::*;

use crate::{Anchor, Image};

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
    /// Callout reference marker in verbatim content: `<1>`, `<.>`, etc.
    CalloutRef(CalloutRef),
}

/// An inline macro - a functional element that produces inline content.
///
/// Unlike a struct with `name`/`target`/`attributes` fields, `InlineMacro` is an **enum**
/// where each variant represents a specific macro type with its own specialized fields.
///
/// # Variants Overview
///
/// | Variant | `AsciiDoc` Syntax | Description |
/// |---------|-----------------|-------------|
/// | `Link` | `link:url[text]` | Explicit link with optional text |
/// | `Url` | `\https://...` or `link:` | URL reference |
/// | `Mailto` | `mailto:addr[text]` | Email link |
/// | `Autolink` | `<\https://...>` | Auto-detected URL |
/// | `CrossReference` | `<<id>>` or `xref:id[]` | Internal document reference |
/// | `Image` | `image:file.png[alt]` | Inline image |
/// | `Icon` | `icon:name[]` | Icon reference (font or image) |
/// | `Footnote` | `footnote:[text]` | Footnote reference |
/// | `Keyboard` | `kbd:[Ctrl+C]` | Keyboard shortcut |
/// | `Button` | `btn:[OK]` | UI button label |
/// | `Menu` | `menu:File[Save]` | Menu navigation path |
/// | `Pass` | `pass:[content]` | Passthrough (no processing) |
/// | `Stem` | `stem:[formula]` | Math notation |
/// | `IndexTerm` | `((term))` or `(((term)))` | Index term (visible or hidden) |
///
/// # Example
///
/// ```
/// # use acdc_parser::{InlineMacro, InlineNode};
/// fn extract_link_target(node: &InlineNode) -> Option<String> {
///     match node {
///         InlineNode::Macro(InlineMacro::Link(link)) => Some(link.target.to_string()),
///         InlineNode::Macro(InlineMacro::Url(url)) => Some(url.target.to_string()),
///         InlineNode::Macro(InlineMacro::CrossReference(xref)) => Some(xref.target.clone()),
///         _ => None,
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum InlineMacro {
    /// Footnote reference: `footnote:[content]` or `footnote:id[content]`
    Footnote(Footnote),
    /// Icon macro: `icon:name[attributes]`
    Icon(Icon),
    /// Inline image: `image:path[alt,width,height]`
    Image(Box<Image>),
    /// Keyboard shortcut: `kbd:[Ctrl+C]`
    Keyboard(Keyboard),
    /// UI button: `btn:[Label]`
    Button(Button),
    /// Menu path: `menu:TopLevel[Item > Subitem]`
    Menu(Menu),
    /// URL with optional text: parsed from `link:` macro or bare URLs
    Url(Url),
    /// Explicit link macro: `link:target[text]`
    Link(Link),
    /// Email link: `mailto:address[text]`
    Mailto(Mailto),
    /// Auto-detected URL: `<\https://example.com>`
    Autolink(Autolink),
    /// Cross-reference: `<<id,text>>` or `xref:id[text]`
    CrossReference(CrossReference),
    /// Inline passthrough: `pass:[content]` - not serialized to ASG
    Pass(Pass),
    /// Inline math: `stem:[formula]` or `latexmath:[...]` / `asciimath:[...]`
    Stem(Stem),
    /// Index term: `((term))` (visible) or `(((term)))` (hidden)
    IndexTerm(IndexTerm),
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
            InlineNode::CalloutRef(callout_ref) => {
                map.serialize_entry("name", "callout_reference")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("variant", &callout_ref.kind)?;
                map.serialize_entry("number", &callout_ref.number)?;
                map.serialize_entry("location", &callout_ref.location)?;
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
        InlineMacro::Footnote(f) => serialize_footnote::<S>(f, map),
        InlineMacro::Icon(i) => serialize_icon::<S>(i, map),
        InlineMacro::Image(i) => serialize_image::<S>(i, map),
        InlineMacro::Keyboard(k) => serialize_keyboard::<S>(k, map),
        InlineMacro::Button(b) => serialize_button::<S>(b, map),
        InlineMacro::Menu(m) => serialize_menu::<S>(m, map),
        InlineMacro::Url(u) => serialize_url::<S>(u, map),
        InlineMacro::Mailto(m) => serialize_mailto::<S>(m, map),
        InlineMacro::Link(l) => serialize_link::<S>(l, map),
        InlineMacro::Autolink(a) => serialize_autolink::<S>(a, map),
        InlineMacro::CrossReference(x) => serialize_xref::<S>(x, map),
        InlineMacro::Stem(s) => serialize_stem::<S>(s, map),
        InlineMacro::IndexTerm(i) => serialize_indexterm::<S>(i, map),
        InlineMacro::Pass(_) => Err(S::Error::custom(
            "inline passthrough macros are not part of the ASG specification and cannot be serialized",
        )),
    }
}

fn serialize_footnote<S>(f: &Footnote, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "footnote")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("id", &f.id)?;
    map.serialize_entry("inlines", &f.content)?;
    map.serialize_entry("location", &f.location)
}

fn serialize_icon<S>(i: &Icon, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "icon")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("target", &i.target)?;
    if !i.attributes.is_empty() {
        map.serialize_entry("attributes", &i.attributes)?;
    }
    map.serialize_entry("location", &i.location)
}

fn serialize_image<S>(i: &Image, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "image")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("title", &i.title)?;
    map.serialize_entry("target", &i.source)?;
    map.serialize_entry("location", &i.location)
}

fn serialize_keyboard<S>(k: &Keyboard, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "keyboard")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("keys", &k.keys)?;
    map.serialize_entry("location", &k.location)
}

fn serialize_button<S>(b: &Button, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "button")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("label", &b.label)?;
    map.serialize_entry("location", &b.location)
}

fn serialize_menu<S>(m: &Menu, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "menu")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("target", &m.target)?;
    if !m.items.is_empty() {
        map.serialize_entry("items", &m.items)?;
    }
    map.serialize_entry("location", &m.location)
}

fn serialize_url<S>(u: &Url, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "ref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("variant", "link")?;
    map.serialize_entry("target", &u.target)?;
    map.serialize_entry("location", &u.location)?;
    map.serialize_entry("attributes", &u.attributes)
}

fn serialize_mailto<S>(m: &Mailto, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "ref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("variant", "mailto")?;
    map.serialize_entry("target", &m.target)?;
    map.serialize_entry("location", &m.location)?;
    map.serialize_entry("attributes", &m.attributes)
}

fn serialize_link<S>(l: &Link, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "ref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("variant", "link")?;
    map.serialize_entry("target", &l.target)?;
    map.serialize_entry("location", &l.location)?;
    map.serialize_entry("attributes", &l.attributes)
}

fn serialize_autolink<S>(a: &Autolink, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "ref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("variant", "autolink")?;
    map.serialize_entry("target", &a.url)?;
    map.serialize_entry("location", &a.location)
}

fn serialize_xref<S>(x: &CrossReference, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "xref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("target", &x.target)?;
    if let Some(text) = &x.text {
        map.serialize_entry("text", text)?;
    }
    map.serialize_entry("location", &x.location)
}

fn serialize_stem<S>(s: &Stem, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "stem")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("content", &s.content)?;
    map.serialize_entry("notation", &s.notation)?;
    map.serialize_entry("location", &s.location)
}

fn serialize_indexterm<S>(i: &IndexTerm, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "indexterm")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("term", i.term())?;
    if let Some(secondary) = i.secondary() {
        map.serialize_entry("secondary", secondary)?;
    }
    if let Some(tertiary) = i.tertiary() {
        map.serialize_entry("tertiary", tertiary)?;
    }
    map.serialize_entry("visible", &i.is_visible())?;
    map.serialize_entry("location", &i.location)
}
