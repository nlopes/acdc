use crate::{Button, Keyboard, Menu, ast::ContentRef};

use super::{ButtonRef, KeyboardRef, MenuRef};

impl<'doc> ButtonRef<'doc> {
    /// The parsed label, including an empty label.
    #[must_use]
    pub fn label(self) -> ContentRef<'doc> {
        ContentRef::from(&self.node.label)
    }
}

impl<'doc> KeyboardRef<'doc> {
    /// Key labels in sequence order, including empty labels.
    pub fn keys(self) -> impl Iterator<Item = ContentRef<'doc>> + Send + Sync + 'doc {
        self.node.keys.iter().map(ContentRef::from)
    }
}

impl<'doc> MenuRef<'doc> {
    /// The parsed menu name before the item sequence.
    #[must_use]
    pub fn target(self) -> ContentRef<'doc> {
        ContentRef::from(&self.node.target)
    }

    /// Item labels in menu-path order, including empty labels.
    pub fn items(self) -> impl Iterator<Item = ContentRef<'doc>> + Send + Sync + 'doc {
        self.node.items.iter().map(ContentRef::from)
    }
}

impl<'doc> From<&'doc Button<'doc>> for ButtonRef<'doc> {
    fn from(owner: &'doc Button<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc Keyboard<'doc>> for KeyboardRef<'doc> {
    fn from(owner: &'doc Keyboard<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc Menu<'doc>> for MenuRef<'doc> {
    fn from(owner: &'doc Menu<'doc>) -> Self {
        Self::new(owner)
    }
}
