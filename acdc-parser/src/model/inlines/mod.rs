use std::convert::Infallible;

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

mod character_reference;
mod hyperlinks;
mod icon;
mod macros;
mod symbol;
mod text;

pub use character_reference::{CharacterReference, CharacterReferenceKind};
pub use hyperlinks::{Link, Mailto, UrlMacro};
pub use icon::Icon;
pub use macros::*;
pub use symbol::{Symbol, SymbolKind};
pub use text::*;

use crate::{Anchor, InlineImage, Location};

/// An `InlineNode` represents an inline node in a document.
///
/// An inline node is a structural element in a document that can contain other inline
/// nodes and are only valid within a paragraph (a leaf).
///
/// Variants match [`crate::ast::InlineKind`] one to one, apart from
/// [`InlineNode::Extension`], which the read API presents as plain text.
/// Large macro nodes are boxed to keep the enum small; build them with
/// `InlineNode::from(node)` to avoid naming the box.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum InlineNode<'a> {
    /// Inline anchor: `[[id]]` or `anchor:id[]`.
    Anchor(Anchor<'a>),
    /// Auto-detected URL: `<\https://example.com>`.
    Autolink(Autolink<'a>),
    Bold(Bold<'a>),
    /// UI button: `btn:[Label]`.
    Button(Button<'a>),
    /// Callout reference marker in verbatim content: `<1>`, `<.>`, etc.
    Callout(CalloutMarker),
    CharacterReference(CharacterReference<'a>),
    /// Cross-reference: `<<id,text>>` or `xref:id[text]`.
    CrossReference(Box<CrossReference<'a>>),
    DoubleQuotedSpan(DoubleQuotedSpan<'a>),
    /// Authored macro spelling retained for conversion.
    Extension(Box<crate::extensions::RetainedInline<'a>>),
    /// Footnote reference: `footnote:[content]` or `footnote:id[content]`.
    Footnote(Footnote<'a>),
    Highlight(Highlight<'a>),
    /// Icon macro: `icon:name[attributes]`.
    Icon(Box<Icon<'a>>),
    /// Inline image: `image:path[alt,width,height]`.
    Image(Box<InlineImage<'a>>),
    /// Index term: `((term))` (visible) or `(((term)))` (hidden).
    IndexTerm(Box<IndexTerm<'a>>),
    Italic(Italic<'a>),
    /// Keyboard shortcut: `kbd:[Ctrl+C]`.
    KeyboardShortcut(KeyboardShortcut<'a>),
    LineBreak(LineBreak),
    /// Explicit link macro: `link:target[text]`.
    Link(Box<Link<'a>>),
    /// Email link: `mailto:address[text]`.
    Mailto(Box<Mailto<'a>>),
    /// Menu path: `menu:TopLevel[Item > Subitem]`.
    MenuPath(MenuPath<'a>),
    Monospace(Monospace<'a>),
    /// Inline passthrough: `pass:[content]`.
    Passthrough(InlinePassthrough<'a>),
    /// Ordinary text that inherits substitutions from its enclosing block.
    Plain(Plain<'a>),
    /// Passthrough text that carries its own substitutions instead of inheriting them.
    Raw(Raw<'a>),
    SingleQuotedSpan(SingleQuotedSpan<'a>),
    StandaloneCurvedApostrophe(StandaloneCurvedApostrophe),
    /// Inline math: `stem:[formula]` or `latexmath:[...]` / `asciimath:[...]`.
    Stem(Stem<'a>),
    Subscript(Subscript<'a>),
    Superscript(Superscript<'a>),
    /// Resolved typography that requires no further language interpretation.
    Symbol(Symbol),
    /// URL with optional text: parsed from `link:` macro or bare URLs.
    UrlMacro(Box<UrlMacro<'a>>),
    /// Literal or listing text rendered with verbatim whitespace and escaping rules.
    Verbatim(Verbatim<'a>),
}

/// Wrap a node in its variant, boxing the large ones.
macro_rules! inline_node_from {
    ($($variant:ident($node:ident)),+ $(,)?) => { $(
        impl<'a> From<$node<'a>> for InlineNode<'a> {
            fn from(node: $node<'a>) -> Self {
                Self::$variant(node.into())
            }
        }
    )+ };
}

inline_node_from! {
    Anchor(Anchor),
    Autolink(Autolink),
    Bold(Bold),
    Button(Button),
    CharacterReference(CharacterReference),
    CrossReference(CrossReference),
    DoubleQuotedSpan(DoubleQuotedSpan),
    Footnote(Footnote),
    Highlight(Highlight),
    Icon(Icon),
    Image(InlineImage),
    IndexTerm(IndexTerm),
    Italic(Italic),
    KeyboardShortcut(KeyboardShortcut),
    Link(Link),
    Mailto(Mailto),
    MenuPath(MenuPath),
    Monospace(Monospace),
    Passthrough(InlinePassthrough),
    Plain(Plain),
    Raw(Raw),
    SingleQuotedSpan(SingleQuotedSpan),
    Stem(Stem),
    Subscript(Subscript),
    Superscript(Superscript),
    UrlMacro(UrlMacro),
    Verbatim(Verbatim),
}

impl From<CalloutMarker> for InlineNode<'_> {
    fn from(node: CalloutMarker) -> Self {
        Self::Callout(node)
    }
}

impl From<LineBreak> for InlineNode<'_> {
    fn from(node: LineBreak) -> Self {
        Self::LineBreak(node)
    }
}

impl From<StandaloneCurvedApostrophe> for InlineNode<'_> {
    fn from(node: StandaloneCurvedApostrophe) -> Self {
        Self::StandaloneCurvedApostrophe(node)
    }
}

impl From<Symbol> for InlineNode<'_> {
    fn from(node: Symbol) -> Self {
        Self::Symbol(node)
    }
}

impl InlineNode<'_> {
    /// Returns the source location of this inline node.
    #[must_use]
    pub fn location(&self) -> &Location {
        match self {
            Self::Anchor(node) => &node.location,
            Self::Autolink(node) => &node.location,
            Self::Bold(node) => &node.location,
            Self::Button(node) => &node.location,
            Self::Callout(node) => &node.location,
            Self::CharacterReference(node) => &node.location,
            Self::CrossReference(node) => &node.location,
            Self::DoubleQuotedSpan(node) => &node.location,
            Self::Extension(node) => &node.literal.location,
            Self::Footnote(node) => &node.location,
            Self::Highlight(node) => &node.location,
            Self::Icon(node) => &node.location,
            Self::Image(node) => &node.location,
            Self::IndexTerm(node) => &node.location,
            Self::Italic(node) => &node.location,
            Self::KeyboardShortcut(node) => &node.location,
            Self::LineBreak(node) => &node.location,
            Self::Link(node) => &node.location,
            Self::Mailto(node) => &node.location,
            Self::MenuPath(node) => &node.location,
            Self::Monospace(node) => &node.location,
            Self::Passthrough(node) => &node.location,
            Self::Plain(node) => &node.location,
            Self::Raw(node) => &node.location,
            Self::SingleQuotedSpan(node) => &node.location,
            Self::StandaloneCurvedApostrophe(node) => &node.location,
            Self::Stem(node) => &node.location,
            Self::Subscript(node) => &node.location,
            Self::Superscript(node) => &node.location,
            Self::Symbol(node) => &node.location,
            Self::UrlMacro(node) => &node.location,
            Self::Verbatim(node) => &node.location,
        }
    }

    /// Mutable access to this inline node's own location (used by the post-parse
    /// source remap pass). Counterpart to [`InlineNode::location`].
    pub(crate) fn location_mut(&mut self) -> &mut Location {
        match self {
            Self::Anchor(node) => &mut node.location,
            Self::Autolink(node) => &mut node.location,
            Self::Bold(node) => &mut node.location,
            Self::Button(node) => &mut node.location,
            Self::Callout(node) => &mut node.location,
            Self::CharacterReference(node) => &mut node.location,
            Self::CrossReference(node) => &mut node.location,
            Self::DoubleQuotedSpan(node) => &mut node.location,
            Self::Extension(node) => &mut node.literal.location,
            Self::Footnote(node) => &mut node.location,
            Self::Highlight(node) => &mut node.location,
            Self::Icon(node) => &mut node.location,
            Self::Image(node) => &mut node.location,
            Self::IndexTerm(node) => &mut node.location,
            Self::Italic(node) => &mut node.location,
            Self::KeyboardShortcut(node) => &mut node.location,
            Self::LineBreak(node) => &mut node.location,
            Self::Link(node) => &mut node.location,
            Self::Mailto(node) => &mut node.location,
            Self::MenuPath(node) => &mut node.location,
            Self::Monospace(node) => &mut node.location,
            Self::Passthrough(node) => &mut node.location,
            Self::Plain(node) => &mut node.location,
            Self::Raw(node) => &mut node.location,
            Self::SingleQuotedSpan(node) => &mut node.location,
            Self::StandaloneCurvedApostrophe(node) => &mut node.location,
            Self::Stem(node) => &mut node.location,
            Self::Subscript(node) => &mut node.location,
            Self::Superscript(node) => &mut node.location,
            Self::Symbol(node) => &mut node.location,
            Self::UrlMacro(node) => &mut node.location,
            Self::Verbatim(node) => &mut node.location,
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
            $map.serialize_entry("id", &id.id)?;
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
            InlineNode::Extension(call) => {
                map.serialize_entry("name", "text")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", &call.literal.content)?;
                map.serialize_entry("location", &call.literal.location)?;
            }
            InlineNode::Plain(plain) => {
                map.serialize_entry("name", "text")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", &plain.content)?;
                map.serialize_entry("location", &plain.location)?;
            }
            InlineNode::Raw(raw) => serialize_raw::<S>(raw, &mut map)?,
            InlineNode::Verbatim(verbatim) => {
                map.serialize_entry("name", "text")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", &verbatim.content)?;
                map.serialize_entry("location", &verbatim.location)?;
            }
            InlineNode::Highlight(highlight) => {
                serialize_inline_format!(map, highlight, "mark");
            }
            InlineNode::Italic(italic) => {
                serialize_inline_format!(map, italic, "emphasis");
            }
            InlineNode::Bold(bold) => {
                serialize_inline_format!(map, bold, "strong");
            }
            InlineNode::Monospace(monospace) => {
                serialize_inline_format!(map, monospace, "code");
            }
            InlineNode::Subscript(subscript) => {
                serialize_inline_format!(map, subscript, "subscript");
            }
            InlineNode::Superscript(superscript) => {
                serialize_inline_format!(map, superscript, "superscript");
            }
            InlineNode::DoubleQuotedSpan(curved_quotation) => {
                serialize_inline_format!(map, curved_quotation, "curved_quotation");
            }
            InlineNode::SingleQuotedSpan(curved_apostrophe) => {
                serialize_inline_format!(map, curved_apostrophe, "curved_apostrophe");
            }
            InlineNode::StandaloneCurvedApostrophe(standalone) => {
                map.serialize_entry("name", "curved_apostrophe")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("location", &standalone.location)?;
            }
            InlineNode::CharacterReference(reference) => {
                map.serialize_entry("name", "charref")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", &reference.kind())?;
                map.serialize_entry("location", &reference.location)?;
            }
            InlineNode::Symbol(symbol) => {
                map.serialize_entry("name", "symbol")?;
                map.serialize_entry("type", "string")?;
                map.serialize_entry("value", symbol.kind.text())?;
                map.serialize_entry("location", &symbol.location)?;
            }
            InlineNode::LineBreak(line_break) => {
                map.serialize_entry("name", "break")?;
                map.serialize_entry("type", "inline")?;
                map.serialize_entry("location", &line_break.location)?;
            }
            InlineNode::Anchor(anchor) => {
                map.serialize_entry("name", "anchor")?;
                map.serialize_entry("type", "inline")?;
                anchor.serialize_fields(&mut map)?;
            }
            InlineNode::Footnote(f) => serialize_footnote::<S>(f, &mut map)?,
            InlineNode::Icon(i) => serialize_icon::<S>(i, &mut map)?,
            InlineNode::Image(i) => serialize_image::<S>(i, &mut map)?,
            InlineNode::KeyboardShortcut(k) => serialize_keyboard::<S>(k, &mut map)?,
            InlineNode::Button(b) => serialize_button::<S>(b, &mut map)?,
            InlineNode::MenuPath(m) => serialize_menu::<S>(m, &mut map)?,
            InlineNode::UrlMacro(u) => serialize_url::<S>(u, &mut map)?,
            InlineNode::Mailto(m) => serialize_mailto::<S>(m, &mut map)?,
            InlineNode::Link(l) => serialize_link::<S>(l, &mut map)?,
            InlineNode::Autolink(a) => serialize_autolink::<S>(a, &mut map)?,
            InlineNode::CrossReference(x) => serialize_xref::<S>(x, &mut map)?,
            InlineNode::Stem(s) => serialize_stem::<S>(s, &mut map)?,
            InlineNode::IndexTerm(i) => serialize_indexterm::<S>(i, &mut map)?,
            InlineNode::Passthrough(pass) => serialize_pass::<S>(pass, &mut map)?,
            InlineNode::Callout(callout_ref) => {
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

macro_rules! serialize_inline_owner {
    ($($owner:ident => $function:ident),+ $(,)?) => { $(
        impl Serialize for $owner<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(None)?;
                $function::<S>(self, &mut map)?;
                map.end()
            }
        }
    )+ };
}

serialize_inline_owner! {
    Raw => serialize_raw,
    Footnote => serialize_footnote,
    Icon => serialize_icon,
    InlineImage => serialize_image,
    KeyboardShortcut => serialize_keyboard,
    Button => serialize_button,
    MenuPath => serialize_menu,
    UrlMacro => serialize_url,
    Link => serialize_link,
    Mailto => serialize_mailto,
    Autolink => serialize_autolink,
    CrossReference => serialize_xref,
    Stem => serialize_stem,
    IndexTerm => serialize_indexterm,
    InlinePassthrough => serialize_pass,
}

fn serialize_footnote<S>(f: &Footnote<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "footnote")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("id", &f.id)?;
    if let Some(content) = &f.content {
        map.serialize_entry("inlines", content)?;
    }
    map.serialize_entry("location", &f.location)
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

fn serialize_keyboard<S>(
    k: &KeyboardShortcut<'_>,
    map: &mut S::SerializeMap,
) -> Result<(), S::Error>
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

fn serialize_menu<S>(m: &MenuPath<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
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

macro_rules! serialize_hyperlink {
    ($owner:ident, $function:ident, $variant:literal $(, $field:ident)*) => {
        fn $function<S: Serializer>(node: &$owner<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error> {
            map.serialize_entry("name", "ref")?;
            map.serialize_entry("type", "inline")?;
            map.serialize_entry("variant", $variant)?;
            if !node.metadata.is_empty() || !node.macro_attributes.is_empty() {
                map.serialize_entry("metadata", &node.metadata_view())?;
            }
            if let Some(tooltip) = &node.tooltip { map.serialize_entry("tooltip", tooltip)?; }
            map.serialize_entry("target", &node.target)?;
            if !node.text.is_empty() { map.serialize_entry("inlines", &node.text)?; }
            $(if let Some(value) = &node.$field { map.serialize_entry(stringify!($field), value)?; })*
            map.serialize_entry("location", &node.location)
        }
    };
}

serialize_hyperlink!(Link, serialize_link, "link");
serialize_hyperlink!(UrlMacro, serialize_url, "link");
serialize_hyperlink!(Mailto, serialize_mailto, "mailto", subject, body);

fn serialize_autolink<S>(a: &Autolink<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "ref")?;
    map.serialize_entry("type", "inline")?;
    map.serialize_entry("variant", "autolink")?;
    map.serialize_entry("target", &a.url)?;
    if a.bracketed {
        map.serialize_entry("bracketed", &a.bracketed)?;
    }
    map.serialize_entry("location", &a.location)
}

fn serialize_xref<S>(x: &CrossReference<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "xref")?;
    map.serialize_entry("type", "inline")?;
    if !x.metadata.is_empty() || !x.macro_attributes.is_empty() {
        map.serialize_entry("metadata", &x.metadata_view())?;
    }
    map.serialize_entry("target", &x.target)?;
    map.serialize_entry("target_is_local", &x.target_is_local)?;
    if let Some(display) = &x.generated_display {
        map.serialize_entry(
            "generated_display",
            &crate::extensions::GeneratedDisplayRef::new(display),
        )?;
    }
    if let Some(style) = x.xrefstyle.attribute_value() {
        map.serialize_entry("xrefstyle", style)?;
    }
    if !x.text.is_empty() {
        map.serialize_entry("inlines", &x.text)?;
    }
    if let Some(content) = &x.document_reftext {
        map.serialize_entry("document_reftext", content.as_ref())?;
    }
    map.serialize_entry("location", &x.location)
}

fn serialize_raw<S: Serializer>(raw: &Raw<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error> {
    map.serialize_entry("name", "raw")?;
    map.serialize_entry("type", "string")?;
    map.serialize_entry("value", raw.content)?;
    if let Some(metadata) = raw
        .metadata
        .as_ref()
        .filter(|metadata| !metadata.is_empty())
    {
        map.serialize_entry("metadata", metadata)?;
    }
    if !raw.subs.is_empty() {
        map.serialize_entry("substitutions", &raw.subs)?;
    }
    map.serialize_entry("location", &raw.location)
}

fn serialize_pass<S: Serializer>(
    pass: &InlinePassthrough<'_>,
    map: &mut S::SerializeMap,
) -> Result<(), S::Error> {
    map.serialize_entry("name", "pass")?;
    map.serialize_entry("type", "inline")?;
    if let Some(text) = pass.text {
        map.serialize_entry("text", text)?;
    }
    if !pass.substitutions.is_empty() {
        map.serialize_entry("substitutions", &pass.substitutions)?;
    }
    map.serialize_entry("location", &pass.location)
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

#[derive(Serialize)]
struct IndexCatalog<'node, 'a> {
    kind: &'node crate::IndexTermKind<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relationship: &'node Option<crate::IndexTermRelationship<'a>>,
    location: &'node crate::Location,
}

fn serialize_indexterm<S>(i: &IndexTerm<'_>, map: &mut S::SerializeMap) -> Result<(), S::Error>
where
    S: Serializer,
{
    map.serialize_entry("name", "indexterm")?;
    map.serialize_entry("type", "inline")?;
    if !i.metadata.is_empty() || !i.macro_attributes.is_empty() {
        map.serialize_entry("metadata", &i.metadata_view())?;
    }
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
        map.serialize_entry(
            "catalog",
            &IndexCatalog {
                kind: &catalog.kind,
                relationship: &catalog.relationship,
                location: &catalog.location,
            },
        )?;
    }
    map.serialize_entry("visible", &i.is_visible())?;
    map.serialize_entry("location", &i.location)
}

impl<'a> InlineNode<'a> {
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
            Self::UrlMacro(url) => {
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
            Self::MenuPath(menu) => {
                content(&mut menu.target)?;
                for item in &mut menu.items {
                    content(item)?;
                }
            }
            Self::KeyboardShortcut(keyboard) => {
                for key in &mut keyboard.keys {
                    content(key)?;
                }
            }
            Self::Anchor(_)
            | Self::Autolink(_)
            | Self::Bold(_)
            | Self::Callout(_)
            | Self::CharacterReference(_)
            | Self::CrossReference(_)
            | Self::DoubleQuotedSpan(_)
            | Self::Extension(_)
            | Self::Footnote(_)
            | Self::Highlight(_)
            | Self::IndexTerm(_)
            | Self::Italic(_)
            | Self::LineBreak(_)
            | Self::Monospace(_)
            | Self::Passthrough(_)
            | Self::Plain(_)
            | Self::Raw(_)
            | Self::SingleQuotedSpan(_)
            | Self::StandaloneCurvedApostrophe(_)
            | Self::Stem(_)
            | Self::Subscript(_)
            | Self::Superscript(_)
            | Self::Symbol(_)
            | Self::Verbatim(_) => {}
        }
        Ok(())
    }
}
