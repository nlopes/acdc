use crate::{
    ListItemCheckedStatus,
    ast::{AnchorRef, CalloutRef},
};

use super::{
    AttributesRef, BlockRef, CalloutListRef, ContentRef, DescriptionListRef, ElementIdRef,
    LocationRef, OrderedListRef, TokensRef, UnorderedListRef,
};

node_ref!(
    ListItemRef<'doc>,
    crate::ListItem<'doc>,
    "An ordered or unordered list item."
);
node_ref!(
    DescriptionListItemRef<'doc>,
    crate::DescriptionListItem<'doc>,
    "A term and its description."
);
node_ref!(
    CalloutListItemRef<'doc>,
    crate::CalloutListItem<'doc>,
    "A callout explanation."
);

macro_rules! list_reads {
    ($($name:ident, $owner:ident, $item:ident, $attributes:ident);+ $(;)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// Items in their parsed order, without grouping adjacent entries.
                pub fn items(self) -> impl Iterator<Item = $item<'doc>> + Send + Sync + 'doc {
                    self.node.items.iter().map($item::from)
                }

                /// Read this element's attributes.
                #[must_use]
                pub fn attributes(self) -> AttributesRef<'doc> {
                    AttributesRef::$attributes(self.node)
                }

                /// The title, including an explicitly empty title.
                #[must_use]
                pub fn title(self) -> Option<ContentRef<'doc>> {
                    self.node.title.content()
                }

                /// The primary ID, if present.
                #[must_use]
                pub fn id(self) -> Option<ElementIdRef<'doc>> {
                    self.attributes().id()
                }

                /// The retained list style.
                #[must_use]
                pub fn style(self) -> Option<&'doc str> {
                    self.attributes().style()
                }

                /// Role tokens in their retained order.
                #[must_use]
                pub fn roles(self) -> TokensRef<'doc> {
                    self.attributes().roles()
                }

                /// Option tokens in their retained order.
                #[must_use]
                pub fn options(self) -> TokensRef<'doc> {
                    self.attributes().options()
                }
            }

            impl<'doc> From<&'doc crate::$owner<'doc>> for $name<'doc> {
                fn from(owner: &'doc crate::$owner<'doc>) -> Self {
                    Self::new(owner)
                }
            }
        )+
    };
}

list_reads!(
    UnorderedListRef, UnorderedList, ListItemRef, unordered_list;
    OrderedListRef, OrderedList, ListItemRef, ordered_list;
    DescriptionListRef, DescriptionList, DescriptionListItemRef, description_list;
    CalloutListRef, CalloutList, CalloutListItemRef, callout_list;
);

macro_rules! marked_list {
    ($($name:ident),+ $(,)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// The retained list marker, without rendered numbering.
                #[must_use]
                pub fn marker(self) -> &'doc str {
                    self.node.marker
                }
            }
        )+
    };
}

marked_list!(UnorderedListRef, OrderedListRef);

macro_rules! principal_item {
    ($($name:ident, $owner:ident);+ $(;)?) => {
        $(
            impl<'doc> $name<'doc> {
                /// Inline content after the marker, excluding attached blocks.
                #[must_use]
                pub fn principal(self) -> ContentRef<'doc> {
                    ContentRef::new(&self.node.principal)
                }

                /// Blocks attached by nesting or continuation, in content order.
                pub fn blocks(self) -> impl Iterator<Item = BlockRef<'doc>> + Send + Sync + 'doc {
                    self.node.blocks.iter().map(BlockRef::from)
                }
            }

            impl<'doc> From<&'doc crate::$owner<'doc>> for $name<'doc> {
                fn from(owner: &'doc crate::$owner<'doc>) -> Self {
                    Self::new(owner)
                }
            }
        )+
    };
}

principal_item!(ListItemRef, ListItem; CalloutListItemRef, CalloutListItem);

impl<'doc> ListItemRef<'doc> {
    /// The item's retained nesting level.
    #[must_use]
    pub fn level(self) -> u8 {
        self.node.level
    }

    /// The item's retained marker, without rendered numbering.
    #[must_use]
    pub fn marker(self) -> &'doc str {
        self.node.marker
    }

    /// Checklist state; `None` distinguishes an ordinary item from an unchecked one.
    #[must_use]
    pub fn checked(self) -> Option<ListItemCheckedStatus> {
        self.node.checked.clone()
    }
}

impl<'doc> DescriptionListItemRef<'doc> {
    /// The term's inline content, excluding the delimiter.
    #[must_use]
    pub fn term(self) -> ContentRef<'doc> {
        ContentRef::new(&self.node.term)
    }

    /// Inline description after the delimiter, excluding attached blocks.
    #[must_use]
    pub fn principal(self) -> ContentRef<'doc> {
        ContentRef::new(&self.node.principal_text)
    }

    /// Attached description blocks in content order.
    pub fn blocks(self) -> impl Iterator<Item = BlockRef<'doc>> + Send + Sync + 'doc {
        self.node.description.iter().map(BlockRef::from)
    }

    /// The retained term delimiter, such as `::` or `;;`.
    #[must_use]
    pub fn delimiter(self) -> &'doc str {
        self.node.delimiter
    }

    /// The delimiter's source span, if retained.
    #[must_use]
    pub fn delimiter_location(self) -> Option<LocationRef<'doc>> {
        self.node.delimiter_location.as_ref().map(LocationRef::from)
    }

    /// Item anchors in their retained order.
    pub fn anchors(self) -> impl Iterator<Item = AnchorRef<'doc>> + Send + Sync + 'doc {
        self.node.anchors.iter().map(AnchorRef::from)
    }
}

impl<'doc> From<&'doc crate::DescriptionListItem<'doc>> for DescriptionListItemRef<'doc> {
    fn from(owner: &'doc crate::DescriptionListItem<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> CalloutListItemRef<'doc> {
    /// The explanation's callout marker and resolved number.
    #[must_use]
    pub fn callout(self) -> CalloutRef<'doc> {
        CalloutRef::from(&self.node.callout)
    }
}
