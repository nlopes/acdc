use crate::{
    ListItemCheckedStatus,
    ast::{AnchorRef, CalloutMarkerRef},
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
    "An ordered term group and its optional shared definition."
);
node_ref!(
    DescriptionListTermRef<'doc>,
    crate::DescriptionListTerm<'doc>,
    "One term in a description-list group."
);
node_ref!(
    DescriptionListDefinitionRef<'doc>,
    crate::DescriptionListDefinition<'doc>,
    "The definition shared by a description-list group's terms."
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
                /// Items in their retained order.
                /// Clones keep independent positions over the same items.
                pub fn items(self) -> impl Iterator<Item = $item<'doc>> + Clone + Send + Sync + 'doc {
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
    /// Terms in retained order, each with its own content and identity.
    pub fn terms(
        self,
    ) -> impl DoubleEndedIterator<Item = DescriptionListTermRef<'doc>>
    + ExactSizeIterator
    + Clone
    + Send
    + Sync
    + 'doc {
        self.node.terms.iter().map(DescriptionListTermRef::from)
    }

    /// The shared definition; absence differs from a present empty definition.
    #[must_use]
    pub fn definition(self) -> Option<DescriptionListDefinitionRef<'doc>> {
        self.node
            .definition
            .as_ref()
            .map(DescriptionListDefinitionRef::from)
    }
}

impl<'doc> DescriptionListTermRef<'doc> {
    /// The term's content, excluding its delimiter.
    #[must_use]
    pub fn content(self) -> ContentRef<'doc> {
        ContentRef::new(&self.node.content)
    }

    /// The retained delimiter, such as `::` or `;;`.
    #[must_use]
    pub fn delimiter(self) -> &'doc str {
        self.node.delimiter
    }

    /// The delimiter's original source span, if retained.
    #[must_use]
    pub fn delimiter_location(self) -> Option<LocationRef<'doc>> {
        self.node.delimiter_location.as_ref().map(LocationRef::from)
    }

    /// Term anchors in their retained order.
    pub fn anchors(self) -> impl Iterator<Item = AnchorRef<'doc>> + Send + Sync + 'doc {
        self.node.anchors.iter().map(AnchorRef::from)
    }
}

impl<'doc> DescriptionListDefinitionRef<'doc> {
    /// The principal, preserving absent versus authored empty content.
    #[must_use]
    pub fn principal(self) -> Option<ContentRef<'doc>> {
        self.node.principal.as_deref().map(ContentRef::new)
    }

    /// Attached definition blocks in content order.
    pub fn blocks(self) -> impl Iterator<Item = BlockRef<'doc>> + Send + Sync + 'doc {
        self.node.blocks.iter().map(BlockRef::from)
    }
}

impl<'doc> From<&'doc crate::DescriptionListItem<'doc>> for DescriptionListItemRef<'doc> {
    fn from(owner: &'doc crate::DescriptionListItem<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc crate::DescriptionListTerm<'doc>> for DescriptionListTermRef<'doc> {
    fn from(owner: &'doc crate::DescriptionListTerm<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc crate::DescriptionListDefinition<'doc>>
    for DescriptionListDefinitionRef<'doc>
{
    fn from(owner: &'doc crate::DescriptionListDefinition<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> CalloutListItemRef<'doc> {
    /// The explanation's callout marker and resolved number.
    #[must_use]
    pub fn callout(self) -> CalloutMarkerRef<'doc> {
        CalloutMarkerRef::from(&self.node.callout)
    }
}

macro_rules! item_copies {
    ($($read:ident, $owner:ident);+ $(;)?) => {
        $(
            impl<'doc> From<$read<'doc>> for crate::$owner<'doc> {
                /// Copy only this item's prepared values, without rebuilding registrations.
                fn from(item: $read<'doc>) -> Self {
                    item.node.clone()
                }
            }
        )+
    };
}

item_copies!(
    ListItemRef, ListItem;
    DescriptionListItemRef, DescriptionListItem;
    DescriptionListTermRef, DescriptionListTerm;
    DescriptionListDefinitionRef, DescriptionListDefinition;
    CalloutListItemRef, CalloutListItem;
);
