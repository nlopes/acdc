use std::convert::Infallible;

use serde::{
    Serialize,
    ser::{Error as _, SerializeMap, Serializer},
};

mod icon;
mod macros;
mod text;

pub use icon::Icon;
pub use macros::*;
pub use text::*;

use crate::{Anchor, ElementAttributes, InlineImage, Location, Source};

/// An `InlineNode` represents an inline node in a document.
///
/// An inline node is a structural element in a document that can contain other inline
/// nodes and are only valid within a paragraph (a leaf).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum InlineNode<'a> {
    /// Ordinary text that inherits substitutions from its enclosing block.
    PlainText(Plain<'a>),
    /// Passthrough text that carries its own substitutions instead of inheriting them.
    RawText(Raw<'a>),
    /// Literal or listing text rendered with verbatim whitespace and escaping rules.
    VerbatimText(Verbatim<'a>),
    BoldText(Bold<'a>),
    ItalicText(Italic<'a>),
    MonospaceText(Monospace<'a>),
    HighlightText(Highlight<'a>),
    SubscriptText(Subscript<'a>),
    SuperscriptText(Superscript<'a>),
    CurvedQuotationText(CurvedQuotation<'a>),
    CurvedApostropheText(CurvedApostrophe<'a>),
    StandaloneCurvedApostrophe(StandaloneCurvedApostrophe),
    LineBreak(LineBreak),
    InlineAnchor(Anchor<'a>),
    Macro(InlineMacro<'a>),
    /// Callout reference marker in verbatim content: `<1>`, `<.>`, etc.
    CalloutRef(CalloutRef),
}

impl InlineNode<'_> {
    /// Returns the source location of this inline node.
    #[must_use]
    pub fn location(&self) -> &Location {
        match self {
            InlineNode::PlainText(t) => &t.location,
            InlineNode::RawText(t) => &t.location,
            InlineNode::VerbatimText(t) => &t.location,
            InlineNode::BoldText(t) => &t.location,
            InlineNode::ItalicText(t) => &t.location,
            InlineNode::MonospaceText(t) => &t.location,
            InlineNode::HighlightText(t) => &t.location,
            InlineNode::SubscriptText(t) => &t.location,
            InlineNode::SuperscriptText(t) => &t.location,
            InlineNode::CurvedQuotationText(t) => &t.location,
            InlineNode::CurvedApostropheText(t) => &t.location,
            InlineNode::StandaloneCurvedApostrophe(t) => &t.location,
            InlineNode::LineBreak(l) => &l.location,
            InlineNode::InlineAnchor(a) => &a.location,
            InlineNode::Macro(m) => m.location(),
            InlineNode::CalloutRef(c) => &c.location,
        }
    }
}

impl InlineNode<'_> {
    /// Mutable access to this inline node's own location (used by the post-parse
    /// source remap pass). Counterpart to [`InlineNode::location`].
    pub(crate) fn location_mut(&mut self) -> &mut Location {
        match self {
            InlineNode::PlainText(t) => &mut t.location,
            InlineNode::RawText(t) => &mut t.location,
            InlineNode::VerbatimText(t) => &mut t.location,
            InlineNode::BoldText(t) => &mut t.location,
            InlineNode::ItalicText(t) => &mut t.location,
            InlineNode::MonospaceText(t) => &mut t.location,
            InlineNode::HighlightText(t) => &mut t.location,
            InlineNode::SubscriptText(t) => &mut t.location,
            InlineNode::SuperscriptText(t) => &mut t.location,
            InlineNode::CurvedQuotationText(t) => &mut t.location,
            InlineNode::CurvedApostropheText(t) => &mut t.location,
            InlineNode::StandaloneCurvedApostrophe(t) => &mut t.location,
            InlineNode::LineBreak(l) => &mut l.location,
            InlineNode::InlineAnchor(a) => &mut a.location,
            InlineNode::Macro(m) => m.location_mut(),
            InlineNode::CalloutRef(c) => &mut c.location,
        }
    }
}

impl InlineMacro<'_> {
    /// Mutable access to this macro's own location. Counterpart to [`Self::location`].
    pub(crate) fn location_mut(&mut self) -> &mut Location {
        match self {
            Self::Footnote(f) => &mut f.location,
            Self::Icon(i) => &mut i.location,
            Self::Image(img) => &mut img.location,
            Self::Keyboard(k) => &mut k.location,
            Self::Button(b) => &mut b.location,
            Self::Menu(m) => &mut m.location,
            Self::Url(u) => &mut u.location,
            Self::Mailto(m) => &mut m.location,
            Self::Link(l) => &mut l.location,
            Self::Autolink(a) => &mut a.location,
            Self::CrossReference(x) => &mut x.location,
            Self::Pass(p) => &mut p.location,
            Self::Stem(s) => &mut s.location,
            Self::IndexTerm(i) => &mut i.location,
        }
    }
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
///         InlineNode::Macro(InlineMacro::CrossReference(xref)) => Some(xref.target.to_string()),
///         _ => None,
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum InlineMacro<'a> {
    /// Footnote reference: `footnote:[content]` or `footnote:id[content]`
    Footnote(Footnote<'a>),
    /// Icon macro: `icon:name[attributes]`
    Icon(Box<Icon<'a>>),
    /// Inline image: `image:path[alt,width,height]`
    Image(Box<InlineImage<'a>>),
    /// Keyboard shortcut: `kbd:[Ctrl+C]`
    Keyboard(Keyboard<'a>),
    /// UI button: `btn:[Label]`
    Button(Button<'a>),
    /// Menu path: `menu:TopLevel[Item > Subitem]`
    Menu(Menu<'a>),
    /// URL with optional text: parsed from `link:` macro or bare URLs
    Url(Box<Url<'a>>),
    /// Explicit link macro: `link:target[text]`
    Link(Box<Link<'a>>),
    /// Email link: `mailto:address[text]`
    Mailto(Box<Mailto<'a>>),
    /// Auto-detected URL: `<\https://example.com>`
    Autolink(Autolink<'a>),
    /// Cross-reference: `<<id,text>>` or `xref:id[text]`
    CrossReference(CrossReference<'a>),
    /// Inline passthrough: `pass:[content]` - not serialized to ASG
    Pass(Pass<'a>),
    /// Inline math: `stem:[formula]` or `latexmath:[...]` / `asciimath:[...]`
    Stem(Stem<'a>),
    /// Index term: `((term))` (visible) or `(((term)))` (hidden)
    IndexTerm(Box<IndexTerm<'a>>),
}

impl InlineMacro<'_> {
    /// Returns the source location of this inline macro.
    #[must_use]
    pub fn location(&self) -> &Location {
        match self {
            Self::Footnote(f) => &f.location,
            Self::Icon(i) => &i.location,
            Self::Image(img) => &img.location,
            Self::Keyboard(k) => &k.location,
            Self::Button(b) => &b.location,
            Self::Menu(m) => &m.location,
            Self::Url(u) => &u.location,
            Self::Mailto(m) => &m.location,
            Self::Link(l) => &l.location,
            Self::Autolink(a) => &a.location,
            Self::CrossReference(x) => &x.location,
            Self::Pass(p) => &p.location,
            Self::Stem(s) => &s.location,
            Self::IndexTerm(i) => &i.location,
        }
    }
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

impl Serialize for InlineNode<'_> {
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
                // ASG represents verbatim leaf content as text rather than raw content.
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
    macro_node: &InlineMacro<'_>,
    map: &mut S::SerializeMap,
) -> Result<(), S::Error>
where
    S: Serializer,
{
    let tooltip = match macro_node {
        InlineMacro::Link(value) => value.tooltip.as_ref(),
        InlineMacro::Url(value) => value.tooltip.as_ref(),
        InlineMacro::Mailto(value) => value.tooltip.as_ref(),

        InlineMacro::Icon(_)
        | InlineMacro::Footnote(_)
        | InlineMacro::Image(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Button(_)
        | InlineMacro::Menu(_)
        | InlineMacro::Autolink(_)
        | InlineMacro::CrossReference(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Stem(_)
        | InlineMacro::IndexTerm(_) => None,
    };
    if let Some(tooltip) = tooltip {
        map.serialize_entry("tooltip", tooltip)?;
    }
    match macro_node {
        InlineMacro::Footnote(f) => serialize_footnote::<S>(f, map),
        InlineMacro::Icon(i) => serialize_icon::<S>(i, map),
        InlineMacro::Image(i) => serialize_image::<S>(i, map),
        InlineMacro::Keyboard(k) => serialize_keyboard::<S>(k, map),
        InlineMacro::Button(b) => serialize_button::<S>(b, map),
        InlineMacro::Menu(m) => serialize_menu::<S>(m, map),
        InlineMacro::Url(u) => {
            serialize_ref::<S>(map, "link", &u.target, &u.text, &u.attributes, &u.location)
        }
        InlineMacro::Mailto(m) => serialize_ref::<S>(
            map,
            "mailto",
            &m.target,
            &m.text,
            &m.attributes,
            &m.location,
        ),
        InlineMacro::Link(l) => {
            serialize_ref::<S>(map, "link", &l.target, &l.text, &l.attributes, &l.location)
        }
        InlineMacro::Autolink(a) => serialize_autolink::<S>(a, map),
        InlineMacro::CrossReference(x) => serialize_xref::<S>(x, map),
        InlineMacro::Stem(s) => serialize_stem::<S>(s, map),
        InlineMacro::IndexTerm(i) => serialize_indexterm::<S>(i, map),
        InlineMacro::Pass(_) => Err(S::Error::custom(
            "inline passthrough macros are not part of the ASG specification and cannot be serialized",
        )),
    }
}

fn serialize_footnote<S>(f: &Footnote<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "footnote")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("id", &f.id)?;
    map.serialize_entry("inlines", &f.content)?;
    map.serialize_entry("location", &f.location)
}

impl Serialize for Icon<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        serialize_icon::<S>(self, &mut map)?;
        map.end()
    }
}

fn serialize_icon<S>(i: &Icon<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "icon")?;
    map.serialize_entry("type", "inline")?;
    if i.has_metadata() {
        map.serialize_entry("metadata", &i.metadata_view())?;
    }
    if let Some(alt) = &i.alt {
        map.serialize_entry("alt", alt)?;
    }
    if let Some(tooltip) = &i.tooltip {
        map.serialize_entry("tooltip", tooltip)?;
    }
    map.serialize_entry("target", &i.target)?;
    map.serialize_entry("location", &i.location)
}

impl Serialize for InlineImage<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        serialize_image::<S>(self, &mut map)?;
        map.end()
    }
}

fn serialize_image<S>(i: &InlineImage<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "image")?;
    map.serialize_entry("type", "inline")?;
    if i.has_metadata() {
        map.serialize_entry("metadata", &i.metadata_view())?;
    }
    if let Some(alt) = &i.alt {
        map.serialize_entry("alt", alt)?;
    }
    if let Some(tooltip) = &i.tooltip {
        map.serialize_entry("tooltip", tooltip)?;
    }
    map.serialize_entry("target", &i.source)?;
    map.serialize_entry("location", &i.location)
}

fn serialize_keyboard<S>(k: &Keyboard<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "keyboard")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("keys", &k.keys)?;
    map.serialize_entry("location", &k.location)
}

fn serialize_button<S>(b: &Button<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "button")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("label", &b.label)?;
    map.serialize_entry("location", &b.location)
}

fn serialize_menu<S>(m: &Menu<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
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

/// Serialize a `link`/`mailto`-style inline reference (`Url`, `Mailto`, `Link` —
/// all structurally identical) into the shared ASG `ref` shape.
fn serialize_ref<S>(
    map: &mut S::SerializeMap,
    variant: &str,
    target: &Source<'_>,
    text: &[InlineNode<'_>],
    attributes: &ElementAttributes<'_>,
    location: &Location,
) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "ref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("variant", variant)?;
    map.serialize_entry("target", target)?;
    if !text.is_empty() {
        map.serialize_entry("inlines", text)?;
    }
    map.serialize_entry("location", location)?;
    map.serialize_entry("attributes", attributes)
}

fn serialize_autolink<S>(a: &Autolink<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "ref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("variant", "autolink")?;
    map.serialize_entry("target", &a.url)?;
    map.serialize_entry("location", &a.location)
}

fn serialize_xref<S>(x: &CrossReference<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "xref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("target", &x.target)?;
    if !x.text.is_empty() {
        map.serialize_entry("inlines", &x.text)?;
    }
    map.serialize_entry("location", &x.location)
}

fn serialize_stem<S>(s: &Stem<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "stem")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("content", &s.content)?;
    map.serialize_entry("notation", &s.notation)?;
    map.serialize_entry("location", &s.location)
}

fn serialize_indexterm<S>(i: &IndexTerm<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
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
    if let Some(relationship) = &i.relationship {
        map.serialize_entry("relationship", relationship)?;
    }
    if let Some(catalog) = &i.catalog {
        map.serialize_entry("catalog", catalog)?;
    }
    map.serialize_entry("visible", &i.is_visible())?;
    map.serialize_entry("location", &i.location)
}

impl<'a> InlineMacro<'a> {
    pub(crate) fn for_each_content_attribute_mut(
        &mut self,
        mut visit: impl FnMut(&mut Vec<InlineNode<'a>>),
    ) {
        self.try_for_each_content_attribute_mut(|nodes| {
            visit(nodes);
            Ok(())
        })
        .unwrap_or_else(|never: Infallible| match never {});
    }

    /// Visit mutable content attributes in order, stopping at the first error.
    pub(crate) fn try_for_each_content_attribute_mut<E>(
        &mut self,
        mut visit: impl FnMut(&mut Vec<InlineNode<'a>>) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut content = |value: &mut crate::ContentAttribute<'a>| visit(value.inlines_mut());
        match self {
            Self::Image(image) => {
                for value in [&mut image.alt, &mut image.tooltip].into_iter().flatten() {
                    content(value)?;
                }
            }
            Self::Icon(icon) => {
                for value in [&mut icon.alt, &mut icon.tooltip].into_iter().flatten() {
                    content(value)?;
                }
            }
            Self::Link(link) => {
                if let Some(value) = &mut link.tooltip {
                    content(value)?;
                }
            }
            Self::Url(url) => {
                if let Some(value) = &mut url.tooltip {
                    content(value)?;
                }
            }
            Self::Mailto(mailto) => {
                if let Some(value) = &mut mailto.tooltip {
                    content(value)?;
                }
            }
            Self::Button(button) => content(&mut button.label)?,
            Self::Menu(menu) => {
                content(&mut menu.target)?;
                for item in &mut menu.items {
                    content(item)?;
                }
            }
            Self::Keyboard(keyboard) => {
                for key in &mut keyboard.keys {
                    content(key)?;
                }
            }
            Self::Footnote(_)
            | Self::Autolink(_)
            | Self::CrossReference(_)
            | Self::Pass(_)
            | Self::Stem(_)
            | Self::IndexTerm(_) => {}
        }
        Ok(())
    }
}

impl<'a> InlineMacro<'a> {
    /// Visit explicit alternative text, tooltips, and UI labels.
    /// Link labels and footnote bodies are separate macro content.
    pub fn for_each_content_attribute(&self, mut visit: impl FnMut(&crate::ContentAttribute<'a>)) {
        match self {
            Self::Image(image) => {
                for value in [&image.alt, &image.tooltip].into_iter().flatten() {
                    visit(value);
                }
            }
            Self::Icon(icon) => {
                for value in [&icon.alt, &icon.tooltip].into_iter().flatten() {
                    visit(value);
                }
            }
            Self::Link(link) => {
                if let Some(value) = &link.tooltip {
                    visit(value);
                }
            }
            Self::Url(url) => {
                if let Some(value) = &url.tooltip {
                    visit(value);
                }
            }
            Self::Mailto(mailto) => {
                if let Some(value) = &mailto.tooltip {
                    visit(value);
                }
            }
            Self::Button(button) => visit(&button.label),
            Self::Menu(menu) => {
                visit(&menu.target);
                for item in &menu.items {
                    visit(item);
                }
            }
            Self::Keyboard(keyboard) => {
                for key in &keyboard.keys {
                    visit(key);
                }
            }
            Self::Footnote(_)
            | Self::Autolink(_)
            | Self::CrossReference(_)
            | Self::Pass(_)
            | Self::Stem(_)
            | Self::IndexTerm(_) => {}
        }
    }
}
