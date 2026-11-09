//! List types for `AsciiDoc` documents.

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use super::{
    Block,
    anchor::Anchor,
    inlines::{CalloutMarker, InlineNode},
    location::Location,
    metadata::BlockMetadata,
    title::Title,
};

pub(crate) type ListLevel = u8;

/// A `ListItemCheckedStatus` represents the checked status of a list item.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ListItemCheckedStatus {
    Checked,
    Unchecked,
}

/// A `ListItem` represents a list item in a document.
///
/// List items have principal text (inline content immediately after the marker) and
/// optionally attached blocks (via continuation or nesting). This matches Asciidoctor's
/// AST structure where principal text renders as bare `<p>` and attached blocks render
/// with their full wrapper divs.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ListItem<'a> {
    pub(crate) level: ListLevel,
    pub(crate) marker: &'a str,
    pub(crate) checked: Option<ListItemCheckedStatus>,
    /// Principal text - inline content that appears immediately after the list marker
    pub(crate) principal: Vec<InlineNode<'a>>,
    /// Attached blocks - blocks attached via continuation (+) or nesting
    pub(crate) blocks: Vec<Block<'a>>,
    pub(crate) location: Location,
}

/// A `DescriptionList` represents a description list in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DescriptionList<'a> {
    pub(crate) title: Title<'a>,
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) items: Vec<DescriptionListItem<'a>>,
    pub(crate) location: Location,
}

/// An ordered group of terms that share one optional definition.
///
/// Read the shared body through [`crate::ast::DescriptionListItemRef::definition`].
///
/// ```
/// # use acdc_parser::ast::DescriptionListItemRef;
/// fn has_attached_blocks(item: DescriptionListItemRef<'_>) -> bool {
///     item.definition()
///         .is_some_and(|definition| definition.blocks().next().is_some())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct DescriptionListItem<'a> {
    pub(crate) terms: Vec<DescriptionListTerm<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) definition: Option<DescriptionListDefinition<'a>>,
    pub(crate) location: Location,
}

/// One authored term, with its own anchors, delimiter, and source location.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct DescriptionListTerm<'a> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) anchors: Vec<Anchor<'a>>,
    pub(crate) content: Vec<InlineNode<'a>>,
    pub(crate) delimiter: &'a str,
    #[serde(skip)]
    pub(crate) delimiter_location: Option<Location>,
    pub(crate) location: Location,
}

/// The single definition shared by a description item's terms.
///
/// An absent principal differs from authored text that expands to empty.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct DescriptionListDefinition<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) principal: Option<Vec<InlineNode<'a>>>,
    pub(crate) blocks: Vec<Block<'a>>,
    pub(crate) location: Location,
}

/// A `UnorderedList` represents an unordered list in a document.
///
/// Use [`Self::take_items`] and [`Self::set_items`] for owned item edits.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Location, UnorderedList};
/// let mut list = UnorderedList::new(Vec::new(), "*", Location::default());
/// list.items.clear();
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct UnorderedList<'a> {
    pub(crate) title: Title<'a>,
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) items: Vec<ListItem<'a>>,
    pub(crate) marker: &'a str,
    pub(crate) location: Location,
}

/// An `OrderedList` represents an ordered list in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct OrderedList<'a> {
    pub(crate) title: Title<'a>,
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) items: Vec<ListItem<'a>>,
    pub(crate) marker: &'a str,
    pub(crate) location: Location,
}

/// A `CalloutList` represents a callout list in a document.
///
/// Callout lists are used to annotate code blocks with numbered references.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CalloutList<'a> {
    pub(crate) title: Title<'a>,
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) items: Vec<CalloutListItem<'a>>,
    pub(crate) location: Location,
}

/// A `CalloutListItem` represents an item in a callout list.
///
/// Unlike [`ListItem`], callout list items have a structured [`CalloutMarker`] that
/// preserves whether the original marker was explicit (`<1>`) or auto-numbered (`<.>`).
///
/// # Example
///
/// ```asciidoc
/// <1> First explanation
/// <.> Auto-numbered explanation
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CalloutListItem<'a> {
    /// The callout reference (explicit or auto-numbered), numbered by item position.
    pub(crate) callout: CalloutMarker,
    /// Principal text - inline content that appears after the callout marker.
    pub(crate) principal: Vec<InlineNode<'a>>,
    /// Attached blocks - blocks attached via continuation (though rarely used for callouts).
    pub(crate) blocks: Vec<Block<'a>>,
    /// Source location of this item.
    pub(crate) location: Location,
}

macro_rules! list_edits {
    ($($owner:ident, $item:ident);+ $(;)?) => {
        $(
            impl<'a> $owner<'a> {
                /// Replace metadata without shadowing dedicated fields.
                ///
                /// # Errors
                /// Rejects conflicting scalar attributes without changing the list.
                pub fn set_metadata(
                    &mut self,
                    metadata: BlockMetadata<'a>,
                ) -> Result<(), crate::ReservedAttribute> {
                    metadata.validate_attributes(&["title"])?;
                    self.metadata = metadata;
                    Ok(())
                }

                /// Attach checked metadata without interpreting its content.
                ///
                /// # Errors
                /// Rejects scalar attributes for dedicated fields.
                pub fn with_metadata(
                    mut self,
                    metadata: BlockMetadata<'a>,
                ) -> Result<Self, crate::ReservedAttribute> {
                    self.set_metadata(metadata)?;
                    Ok(self)
                }

                /// Copy metadata without copying items or their attached blocks.
                #[must_use]
                pub fn clone_metadata(&self) -> BlockMetadata<'a> {
                    self.metadata.clone()
                }

                /// Remove metadata without renumbering items or rebuilding registrations.
                #[must_use]
                pub fn take_metadata(&mut self) -> BlockMetadata<'a> {
                    std::mem::take(&mut self.metadata)
                }

                /// Replace the prepared title, including an explicitly empty value.
                pub fn set_title(&mut self, title: Title<'a>) {
                    self.title = title;
                }

                /// Attach a prepared title without parsing or registering content.
                #[must_use]
                pub fn with_title(mut self, title: Title<'a>) -> Self {
                    self.set_title(title);
                    self
                }

                /// Remove the title, leaving it absent.
                #[must_use]
                pub fn take_title(&mut self) -> Title<'a> {
                    std::mem::take(&mut self.title)
                }

                /// Replace prepared items without renumbering or rebuilding registrations.
                pub fn set_items(&mut self, items: impl IntoIterator<Item = $item<'a>>) {
                    self.items = items.into_iter().collect();
                }

                /// Copy items to prepare an independent replacement, retaining source lifetimes.
                pub fn clone_items(
                    &self,
                ) -> impl DoubleEndedIterator<Item = $item<'a>> + ExactSizeIterator + use<'a> {
                    self.items.clone().into_iter()
                }

                /// Append a prepared item without renumbering or rebuilding registrations.
                pub fn push_item(&mut self, item: $item<'a>) {
                    self.items.push(item);
                }

                /// Remove items, retaining their content and original source lifetimes.
                pub fn take_items(
                    &mut self,
                ) -> impl DoubleEndedIterator<Item = $item<'a>> + ExactSizeIterator + use<'a> {
                    std::mem::take(&mut self.items).into_iter()
                }

                /// Replace the list span without relocating items or their attached blocks.
                pub fn set_location(&mut self, location: Location) {
                    self.location = location;
                }
            }
        )+
    };
}

list_edits!(
    UnorderedList, ListItem;
    OrderedList, ListItem;
    CalloutList, CalloutListItem;
    DescriptionList, DescriptionListItem;
);

macro_rules! marked_lists {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'a> $name<'a> {
                /// Construct a list from prepared items and retained marker spelling.
                #[must_use]
                pub fn new(items: Vec<ListItem<'a>>, marker: &'a str, location: Location) -> Self {
                    Self {
                        title: Title::default(),
                        metadata: BlockMetadata::default(),
                        items,
                        marker,
                        location,
                    }
                }

                /// Replace marker spelling without changing item markers, levels or numbering.
                pub fn replace_marker(&mut self, marker: &'a str) -> &'a str {
                    std::mem::replace(&mut self.marker, marker)
                }
            }
        )+
    };
}

marked_lists!(UnorderedList, OrderedList);

impl<'a> DescriptionList<'a> {
    /// Construct a description list from prepared entries without registering their content.
    #[must_use]
    pub fn new(items: Vec<DescriptionListItem<'a>>, location: Location) -> Self {
        Self {
            title: Title::default(),
            metadata: BlockMetadata::default(),
            items,
            location,
        }
    }
}

impl<'a> CalloutList<'a> {
    /// Construct a callout list without recomputing the supplied item numbers.
    #[must_use]
    pub fn new(items: Vec<CalloutListItem<'a>>, location: Location) -> Self {
        Self {
            title: Title::default(),
            metadata: BlockMetadata::default(),
            items,
            location,
        }
    }
}

macro_rules! item_content_edits {
    ($($owner:ident, $principal:ident, $blocks:ident);+ $(;)?) => {
        $(
            impl<'a> $owner<'a> {
                /// Replace prepared principal text without rebuilding registrations.
                pub fn set_principal(&mut self, content: impl IntoIterator<Item = InlineNode<'a>>) {
                    self.$principal = content.into_iter().collect();
                }

                /// Remove principal text, retaining each inline's original source lifetime.
                pub fn take_principal(
                    &mut self,
                ) -> impl DoubleEndedIterator<Item = InlineNode<'a>> + ExactSizeIterator + use<'a> {
                    std::mem::take(&mut self.$principal).into_iter()
                }

                /// Replace attached blocks without renesting or rebuilding registrations.
                pub fn set_blocks(&mut self, blocks: impl IntoIterator<Item = Block<'a>>) {
                    self.$blocks = blocks.into_iter().collect();
                }

                /// Append a prepared attached block without rebuilding registrations.
                pub fn push_block(&mut self, block: Block<'a>) {
                    self.$blocks.push(block);
                }

                /// Remove attached blocks, retaining their original source lifetimes.
                pub fn take_blocks(
                    &mut self,
                ) -> impl DoubleEndedIterator<Item = Block<'a>> + ExactSizeIterator + use<'a> {
                    std::mem::take(&mut self.$blocks).into_iter()
                }

                /// Replace the item span without relocating its content, markers or anchors.
                pub fn set_location(&mut self, location: Location) {
                    self.location = location;
                }
            }
        )+
    };
}

item_content_edits!(
    ListItem, principal, blocks;
    CalloutListItem, principal, blocks;
);

impl<'a> ListItem<'a> {
    /// Construct a list item without deriving nesting or checklist state from its marker.
    #[must_use]
    pub fn new(
        principal: Vec<InlineNode<'a>>,
        level: ListLevel,
        marker: &'a str,
        location: Location,
    ) -> Self {
        Self {
            principal,
            level,
            marker,
            checked: None,
            blocks: Vec::new(),
            location,
        }
    }

    /// Replace the retained level without moving or renesting attached blocks.
    pub fn set_level(&mut self, level: ListLevel) {
        self.level = level;
    }

    /// Replace marker spelling without deriving a new level or checklist state.
    pub fn replace_marker(&mut self, marker: &'a str) -> &'a str {
        std::mem::replace(&mut self.marker, marker)
    }

    /// Replace checklist state; `None` restores an ordinary list item.
    pub fn set_checked(&mut self, checked: Option<ListItemCheckedStatus>) {
        self.checked = checked;
    }
}

impl<'a> CalloutListItem<'a> {
    /// Construct an explanation with a prepared marker and number.
    #[must_use]
    pub fn new(callout: CalloutMarker, principal: Vec<InlineNode<'a>>, location: Location) -> Self {
        Self {
            callout,
            principal,
            blocks: Vec::new(),
            location,
        }
    }

    /// Replace the prepared marker without changing other items or source-code references.
    pub fn replace_callout(&mut self, callout: CalloutMarker) -> CalloutMarker {
        std::mem::replace(&mut self.callout, callout)
    }
}

impl<'a> DescriptionListItem<'a> {
    /// Construct a group without reparsing terms or rebuilding registrations.
    #[must_use]
    pub fn new(
        terms: Vec<DescriptionListTerm<'a>>,
        definition: Option<DescriptionListDefinition<'a>>,
        location: Location,
    ) -> Self {
        Self {
            terms,
            definition,
            location,
        }
    }

    /// Replace terms in order without changing the shared definition.
    pub fn set_terms(&mut self, terms: impl IntoIterator<Item = DescriptionListTerm<'a>>) {
        self.terms = terms.into_iter().collect();
    }

    /// Append a prepared term to this group without rebuilding registrations.
    pub fn push_term(&mut self, term: DescriptionListTerm<'a>) {
        self.terms.push(term);
    }

    /// Remove terms, retaining their source lifetimes and the shared definition.
    pub fn take_terms(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = DescriptionListTerm<'a>> + ExactSizeIterator + use<'a>
    {
        std::mem::take(&mut self.terms).into_iter()
    }

    /// Replace the shared definition; `None` leaves it absent.
    pub fn set_definition(&mut self, definition: Option<DescriptionListDefinition<'a>>) {
        self.definition = definition;
    }

    /// Remove the shared definition without copying its content.
    #[must_use]
    pub fn take_definition(&mut self) -> Option<DescriptionListDefinition<'a>> {
        self.definition.take()
    }

    /// Replace the group span without relocating terms or the definition.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

impl<'a> DescriptionListDefinition<'a> {
    /// Construct a definition with an optional principal and prepared attached blocks.
    #[must_use]
    pub fn new(
        principal: Option<Vec<InlineNode<'a>>>,
        blocks: Vec<Block<'a>>,
        location: Location,
    ) -> Self {
        Self {
            principal,
            blocks,
            location,
        }
    }

    /// Replace principal content, preserving absent versus explicitly empty content.
    pub fn set_principal(&mut self, principal: Option<Vec<InlineNode<'a>>>) {
        self.principal = principal;
    }

    /// Remove the principal without copying its content.
    #[must_use]
    pub fn take_principal(&mut self) -> Option<Vec<InlineNode<'a>>> {
        self.principal.take()
    }

    /// Replace attached blocks without renesting or rebuilding registrations.
    pub fn set_blocks(&mut self, blocks: impl IntoIterator<Item = Block<'a>>) {
        self.blocks = blocks.into_iter().collect();
    }

    /// Append a prepared block without rebuilding registrations.
    pub fn push_block(&mut self, block: Block<'a>) {
        self.blocks.push(block);
    }

    /// Remove attached blocks, retaining their source lifetimes.
    pub fn take_blocks(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = Block<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.blocks).into_iter()
    }

    /// Replace the definition span without relocating its content.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

impl<'a> DescriptionListTerm<'a> {
    /// Construct a term with prepared content and retained delimiter spelling.
    #[must_use]
    pub fn new(term: Vec<InlineNode<'a>>, delimiter: &'a str, location: Location) -> Self {
        Self {
            content: term,
            delimiter,
            location,
            anchors: Vec::new(),
            delimiter_location: None,
        }
    }

    /// Replace prepared term content without rebuilding anchor or reference registrations.
    pub fn set_content(&mut self, term: impl IntoIterator<Item = InlineNode<'a>>) {
        self.content = term.into_iter().collect();
    }

    /// Remove term content, retaining each inline's original source lifetime.
    pub fn take_content(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = InlineNode<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.content).into_iter()
    }

    /// Replace the term span without relocating its content or anchors.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }

    /// Replace delimiter spelling without relocating its span or reparsing the entry.
    pub fn replace_delimiter(&mut self, delimiter: &'a str) -> &'a str {
        std::mem::replace(&mut self.delimiter, delimiter)
    }

    /// Replace the delimiter span without relocating term or principal content.
    pub fn set_delimiter_location(&mut self, location: Option<Location>) {
        self.delimiter_location = location;
    }

    /// Replace anchors in the supplied order without rebuilding registered targets.
    pub fn set_anchors(&mut self, anchors: impl IntoIterator<Item = Anchor<'a>>) {
        self.anchors = anchors.into_iter().collect();
    }

    /// Remove anchors, retaining their content and original source lifetimes.
    pub fn take_anchors(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = Anchor<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.anchors).into_iter()
    }

    /// Append an anchor without rebuilding reference registrations.
    pub fn push_anchor(&mut self, anchor: Anchor<'a>) {
        self.anchors.push(anchor);
    }
}

// =============================================================================
// Serialization
// =============================================================================

impl Serialize for UnorderedList<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "list")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", "unordered")?;
        state.serialize_entry("marker", &self.marker)?;
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.metadata.is_empty_for_serialization() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("items", &self.items)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for OrderedList<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "list")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", "ordered")?;
        state.serialize_entry("marker", &self.marker)?;
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.metadata.is_empty_for_serialization() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("items", &self.items)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for CalloutList<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "list")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", "callout")?;
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.metadata.is_empty_for_serialization() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("items", &self.items)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for DescriptionList<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "dlist")?;
        state.serialize_entry("type", "block")?;
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.metadata.is_empty_for_serialization() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("items", &self.items)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for ListItem<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "listItem")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("marker", &self.marker)?;
        if let Some(checked) = &self.checked {
            state.serialize_entry("checked", checked)?;
        }
        // The TCK doesn't contain level information for list items, so we don't serialize
        // it.
        //
        // Uncomment the line below if level information is added in the future.
        //
        // state.serialize_entry("level", &self.level)?;
        state.serialize_entry("principal", &self.principal)?;
        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for ListItemCheckedStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            ListItemCheckedStatus::Checked => serializer.serialize_bool(true),
            ListItemCheckedStatus::Unchecked => serializer.serialize_bool(false),
        }
    }
}

impl Serialize for CalloutListItem<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "listItem")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("callout", &self.callout)?;
        state.serialize_entry("principal", &self.principal)?;
        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}
