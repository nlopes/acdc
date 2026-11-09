use crate::{Button, KeyboardShortcut, MenuPath, ast::ContentRef};

use super::{ButtonRef, KeyboardShortcutRef, MenuPathRef};

impl<'doc> ButtonRef<'doc> {
    /// The parsed label, including an empty label.
    #[must_use]
    pub fn label(self) -> ContentRef<'doc> {
        ContentRef::from(&self.node.label)
    }
}

impl<'doc> KeyboardShortcutRef<'doc> {
    /// Key labels in sequence order, including empty labels.
    pub fn keys(self) -> impl Iterator<Item = ContentRef<'doc>> + Send + Sync + 'doc {
        self.node.keys.iter().map(ContentRef::from)
    }
}

impl<'doc> MenuPathRef<'doc> {
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

impl<'doc> From<&'doc KeyboardShortcut<'doc>> for KeyboardShortcutRef<'doc> {
    fn from(owner: &'doc KeyboardShortcut<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<&'doc MenuPath<'doc>> for MenuPathRef<'doc> {
    fn from(owner: &'doc MenuPath<'doc>) -> Self {
        Self::new(owner)
    }
}

impl<'doc> From<ButtonRef<'doc>> for Button<'doc> {
    fn from(read: ButtonRef<'doc>) -> Self {
        read.node.clone()
    }
}

impl<'doc> From<MenuPathRef<'doc>> for MenuPath<'doc> {
    fn from(read: MenuPathRef<'doc>) -> Self {
        read.node.clone()
    }
}

impl<'doc> From<KeyboardShortcutRef<'doc>> for KeyboardShortcut<'doc> {
    fn from(read: KeyboardShortcutRef<'doc>) -> Self {
        read.node.clone()
    }
}
