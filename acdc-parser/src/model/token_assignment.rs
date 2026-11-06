use serde::Serialize;

/// An absent, explicitly empty, or populated role/option assignment.
///
/// Tokens retain their order, duplicates and spelling. Construction does not
/// split or interpret text. Read an owner's tokens through its typed AST view.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TokenAssignment<'a>(Option<Vec<&'a str>>);

impl<'a> TokenAssignment<'a> {
    /// Create a present assignment, including an explicitly empty one.
    #[must_use]
    pub fn new(tokens: Vec<&'a str>) -> Self {
        Self(Some(tokens))
    }

    /// Whether no assignment is present.
    #[must_use]
    pub const fn is_absent(&self) -> bool {
        self.0.is_none()
    }

    /// Whether there are no tokens. Absence and explicit emptiness both qualify.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Consume the assignment without losing its presence.
    #[must_use]
    pub fn into_tokens(self) -> Option<Vec<&'a str>> {
        self.0
    }

    /// Append one prepared token, making the assignment present.
    pub fn push(&mut self, token: &'a str) {
        self.0.get_or_insert_with(Vec::new).push(token);
    }

    /// Clear the tokens while retaining an explicit empty assignment.
    pub fn clear(&mut self) {
        self.0.get_or_insert_with(Vec::new).clear();
    }

    pub(crate) fn as_slice(&self) -> &[&'a str] {
        self.0.as_deref().unwrap_or_default()
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, &'a str> {
        self.as_slice().iter()
    }

    pub(crate) fn contains(&self, token: &&str) -> bool {
        self.as_slice().contains(token)
    }

    pub(crate) fn retain(&mut self, keep: impl FnMut(&&'a str) -> bool) {
        if let Some(tokens) = &mut self.0 {
            tokens.retain(keep);
        }
    }

    pub(crate) fn append(&mut self, other: Self) {
        if let Some(tokens) = other.0 {
            self.0.get_or_insert_with(Vec::new).extend(tokens);
        }
    }
}

impl<'a> From<Vec<&'a str>> for TokenAssignment<'a> {
    fn from(tokens: Vec<&'a str>) -> Self {
        Self::new(tokens)
    }
}

impl<'a> FromIterator<&'a str> for TokenAssignment<'a> {
    fn from_iter<T: IntoIterator<Item = &'a str>>(tokens: T) -> Self {
        Self::new(tokens.into_iter().collect())
    }
}

impl<'a> Extend<&'a str> for TokenAssignment<'a> {
    fn extend<T: IntoIterator<Item = &'a str>>(&mut self, tokens: T) {
        self.0.get_or_insert_with(Vec::new).extend(tokens);
    }
}

impl<'a> IntoIterator for TokenAssignment<'a> {
    type Item = &'a str;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.unwrap_or_default().into_iter()
    }
}

impl<'node, 'a> IntoIterator for &'node TokenAssignment<'a> {
    type Item = &'node &'a str;
    type IntoIter = std::slice::Iter<'node, &'a str>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
