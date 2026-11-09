use crate::Substitution;

/// A borrowed piece of text with its remaining output substitutions.
///
/// Read attribute pieces through [`crate::DocumentAttributeValue::parts`] and
/// raw inline text through [`super::RawRef::text_part`]. The view borrows existing
/// text without allocating or carrying an inline node's identity and location.
/// Attribute rules that produce inline structure remain internal to the parser;
/// reading these pieces does not apply those rules.
#[derive(Clone, Copy, Debug)]
pub struct TextPartRef<'doc> {
    text: &'doc str,
    substitutions: RetainedSubstitutions<'doc>,
}

#[derive(Clone, Copy, Debug)]
enum RetainedSubstitutions<'doc> {
    Sequence(&'doc [Substitution]),
    SpecialChars(usize),
}

impl<'doc> TextPartRef<'doc> {
    pub(crate) const fn new(text: &'doc str, substitutions: &'doc [Substitution]) -> Self {
        Self {
            text,
            substitutions: RetainedSubstitutions::Sequence(substitutions),
        }
    }

    pub(crate) const fn with_special_chars(text: &'doc str, count: usize) -> Self {
        Self {
            text,
            substitutions: RetainedSubstitutions::SpecialChars(count),
        }
    }

    /// Retained text, without applying the remaining substitutions.
    #[must_use]
    pub const fn text(self) -> &'doc str {
        self.text
    }

    /// Remaining substitutions in order, including repeated entries.
    ///
    /// An empty sequence leaves the text unchanged for raw output. Converters
    /// interpret the retained stages for their target format.
    pub fn substitutions(self) -> impl Iterator<Item = Substitution> + Send + Sync + 'doc {
        let (sequence, special_chars) = match self.substitutions {
            RetainedSubstitutions::Sequence(sequence) => (sequence, 0),
            RetainedSubstitutions::SpecialChars(count) => (&[][..], count),
        };
        sequence.iter().cloned().chain(std::iter::repeat_n(
            Substitution::SpecialChars,
            special_chars,
        ))
    }
}
